//! Interactive serial REPL connection.
//!
//! Replaces the Electron `repl-connect` / `repl-send` / `repl-data` IPC layer:
//! the port is opened in-process and read on a background thread, pushing bytes
//! back into GPUI through an entity update.
//!
//! Board quirks carried over from the Electron main process:
//! - Opening the port can reset an ESP32-S3 (DTR/RTS), re-running a blocking
//!   main.py; we interrupt (Ctrl-C ×2) and reprint the banner (Ctrl-B) after a
//!   short delay, with a fallback nudge for slow boots.
//! - "invalid header: 0x..." boot-loop spam from a board with no valid
//!   firmware is collapsed into a single warning.

use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Baud rate every MicroPython / CircuitPython board speaks by default.
pub const REPL_BAUD: u32 = 115_200;

/// Everything the UI learns from the reader thread.
#[derive(Debug, Clone)]
pub enum ReplEvent {
    /// Bytes from the board (already filtered for boot-loop spam).
    Data(String),
    /// The port closed underneath us (unplug, board reset...).
    Disconnected,
    /// An I/O error worth showing in the terminal.
    Error(String),
}

enum Command {
    Write(Vec<u8>),
    Close,
}

pub struct ReplConnection {
    sender: Sender<Command>,
    /// Set when the reader thread observed a close/error, so `is_connected`
    /// can answer without racing the thread.
    state: Arc<Mutex<ReplState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplState {
    Open,
    Closed,
}

impl ReplConnection {
    /// Open `path` at the REPL baud rate and start the reader thread. On
    /// success the nudge sequence (interrupt + banner) has been scheduled.
    pub fn open(path: &str) -> Result<(Self, Receiver<ReplEvent>), String> {
        Self::open_inner(path, None)
    }

    pub(crate) fn open_exclusive(
        path: &str,
        lease: crate::hardware::PortLease,
    ) -> Result<(Self, Receiver<ReplEvent>), String> {
        Self::open_inner(path, Some(Arc::new(lease)))
    }

    fn open_inner(
        path: &str,
        lease: Option<Arc<crate::hardware::PortLease>>,
    ) -> Result<(Self, Receiver<ReplEvent>), String> {
        let mut port = serialport::new(path, REPL_BAUD)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| e.to_string())?;

        // Opening can reset the board; interrupt anything running and reprint
        // the version banner + prompt so the REPL is interactive.
        let _ = port.write_all(b"\x03\x03\x02");
        let _ = port.flush();

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<ReplEvent>();
        let state = Arc::new(Mutex::new(ReplState::Open));

        // Writer + reader live on one background thread: `port` is a single
        // `Box<dyn SerialPort>` and is neither Sync nor cheap to clone.
        let writer_state = state.clone();
        let writer_port = Arc::new(Mutex::new(port));
        let reader_port = writer_port.clone();

        // Writer loop
        let writer_events = event_tx.clone();
        let writer_lease = lease.clone();
        std::thread::Builder::new()
            .name("repl-writer".into())
            .spawn(move || {
                let _lease = writer_lease;
                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        Command::Write(bytes) => {
                            let mut port = writer_port.lock().unwrap();
                            if let Err(e) = port.write_all(&bytes).and_then(|_| port.flush()) {
                                let _ = writer_events.send(ReplEvent::Error(e.to_string()));
                            }
                        }
                        Command::Close => {
                            // Dropping the Arc<Mutex<SerialPort>> in both
                            // threads closes the device file.
                            break;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        // Reader loop
        std::thread::Builder::new()
            .name("repl-reader".into())
            .spawn(move || {
                let _lease = lease;
                let mut invalid_header_count = 0u32;
                let mut boot_loop_warned = false;
                let mut carry: Vec<u8> = Vec::new();
                loop {
                    if *writer_state.lock().unwrap() == ReplState::Closed {
                        return;
                    }
                    let mut buf = [0u8; 1024];
                    let mut port = reader_port.lock().unwrap();
                    let n = match port.read(&mut buf) {
                        Ok(0) => {
                            // Timeout with no data — just poll again.
                            drop(port);
                            // If the writer thread wants out, the receiver is
                            // dropped; detect closed channel lazily below.
                            continue;
                        }
                        Ok(n) => n,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::TimedOut
                                || e.kind() == std::io::ErrorKind::WouldBlock =>
                        {
                            continue
                        }
                        Err(e) => {
                            let _ = event_tx.send(ReplEvent::Error(e.to_string()));
                            let _ = event_tx.send(ReplEvent::Disconnected);
                            *writer_state.lock().unwrap() = ReplState::Closed;
                            return;
                        }
                    };
                    drop(port);

                    carry.extend_from_slice(&buf[..n]);
                    // Decode as UTF-8, deferring incomplete trailing sequences.
                    let text = match std::str::from_utf8(&carry) {
                        Ok(s) => {
                            let s = s.to_string();
                            carry.clear();
                            s
                        }
                        Err(e) if e.valid_up_to() > 0 => {
                            let valid = carry.drain(..e.valid_up_to()).collect::<Vec<u8>>();
                            String::from_utf8_lossy(&valid).to_string()
                        }
                        Err(_) => continue,
                    };

                    // Filter ESP32-S3 ROM bootloader boot-loop spam: "invalid
                    // header: 0x..." repeats when flash has no valid firmware.
                    let mut kept = String::with_capacity(text.len());
                    for line in text.split_inclusive('\n') {
                        if line.trim_start().starts_with("invalid header: 0x") {
                            invalid_header_count += 1;
                            continue;
                        }
                        kept.push_str(line);
                    }
                    if invalid_header_count > 3 && !boot_loop_warned {
                        boot_loop_warned = true;
                        kept.push_str(
                            "\n[Anode] Board is resetting repeatedly ('invalid header: 0x...'). \
                             No valid MicroPython firmware is running, the flash is corrupt, or \
                             the board reset on connect. Re-flash MicroPython firmware, then \
                             reconnect.\n",
                        );
                    }
                    if !kept.is_empty() && event_tx.send(ReplEvent::Data(kept)).is_err() {
                        return;
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        Ok((
            ReplConnection {
                sender: cmd_tx,
                state,
            },
            event_rx,
        ))
    }

    pub fn send(&self, data: &[u8]) {
        let _ = self.sender.send(Command::Write(data.to_vec()));
    }

    pub fn is_connected(&self) -> bool {
        *self.state.lock().unwrap() == ReplState::Open
    }
}

impl Drop for ReplConnection {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Close);
        *self.state.lock().unwrap() = ReplState::Closed;
    }
}

/// Control bytes the REPL panel sends.
pub mod controls {
    /// Ctrl-C — interrupt a running program.
    pub const BREAK: u8 = 0x03;
    /// Ctrl-D — soft reset (MicroPython) / execute (raw REPL).
    pub const SOFT_RESET: u8 = 0x04;
    /// Ctrl-A — enter raw REPL.
    pub const RAW_MODE: u8 = 0x01;
    /// Ctrl-B — return to the friendly REPL / reprint banner.
    pub const FRIENDLY_MODE: u8 = 0x02;
    /// Ctrl-E — paste mode.
    pub const PASTE_MODE: u8 = 0x05;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_fails_cleanly_for_a_missing_device() {
        let result = ReplConnection::open("/dev/does-not-exist-anode-test");
        assert!(result.is_err());
    }
}
