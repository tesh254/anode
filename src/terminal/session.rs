//! PTY ownership and terminal emulation, independent of the native view.
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

const SCROLLBACK: usize = 10_000;
const READ_SIZE: usize = 8192;

enum Event {
    Output(Vec<u8>),
    Exited(String),
    Error(String),
}

/// Respond to terminal queries without touching application or clipboard state.
struct Replies(Sender<Vec<u8>>);
impl vt100::Callbacks for Replies {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        intermediate: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        command: char,
    ) {
        let param = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        let response = match (intermediate, command, param) {
            (None, 'n', 5) => "\x1b[0n".to_string(),
            (None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                format!("\x1b[{};{}R", row + 1, col + 1)
            }
            (None, 'c', 0) => "\x1b[?1;2c".to_string(),
            (Some(b'>'), 'c', _) => "\x1b[>0;1;0c".to_string(),
            _ => return,
        };
        _ = self.0.send(response.into_bytes());
    }
}

pub(super) struct Session {
    captured: Vec<u8>,
    parser: vt100::Parser<Replies>,
    master: Box<dyn MasterPty + Send>,
    input: Sender<Vec<u8>>,
    events: Receiver<Event>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pub(super) shell: String,
    pub(super) exit: Option<String>,
    pub(super) error: Option<String>,
}

pub(super) fn starting_directory(project: Option<&Path>) -> PathBuf {
    project
        .filter(|path| path.is_dir())
        .map(Path::to_path_buf)
        .or_else(dirs::home_dir)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn nonzero(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value.max(1)).expect("clamped to one")
}

impl Session {
    pub(super) fn spawn(directory: &Path) -> Result<Self, String> {
        Self::spawn_command(directory, CommandBuilder::new_default_prog())
    }

    fn spawn_command(directory: &Path, mut command: CommandBuilder) -> Result<Self, String> {
        command.cwd(directory);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "Anode");
        command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        command.env_remove("COLUMNS");
        command.env_remove("LINES");
        let shell = command.get_shell();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Could not create a terminal: {error}"))?;
        // Take the I/O handles before spawning, so setup failure cannot orphan a shell.
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|error| error.to_string())?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Could not start {shell}: {error}"))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let (input, writes) = mpsc::channel::<Vec<u8>>();
        // Bound unread output so a noisy hidden shell cannot grow memory indefinitely.
        let (output, events) = mpsc::sync_channel(64);
        let read_output = output.clone();
        std::thread::spawn(move || {
            let mut buffer = [0; READ_SIZE];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if read_output
                            .send(Event::Output(buffer[..count].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    // Unix PTYs commonly return EIO when the slave closes.
                    Err(_) => break,
                }
            }
        });
        let write_output = output.clone();
        std::thread::spawn(move || {
            while let Ok(bytes) = writes.recv() {
                if let Err(error) = writer.write_all(&bytes).and_then(|_| writer.flush()) {
                    _ = write_output.send(Event::Error(format!("Shell input failed: {error}")));
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let status = child
                .wait()
                .map(|status| status.to_string())
                .unwrap_or_else(|error| error.to_string());
            _ = output.send(Event::Exited(status));
        });
        Ok(Self {
            captured: Vec::new(),
            parser: vt100::Parser::new_with_callbacks(
                nonzero(24),
                nonzero(80),
                SCROLLBACK,
                Replies(input.clone()),
            ),
            master: pair.master,
            input,
            events,
            killer,
            shell,
            exit: None,
            error: None,
        })
    }

    pub(super) fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub(super) fn drain(&mut self) -> bool {
        let mut changed = false;
        // Give other sessions and the UI a chance to run between output bursts.
        for _ in 0..16 {
            let Ok(event) = self.events.try_recv() else {
                break;
            };
            changed = true;
            match event {
                Event::Output(bytes) => {
                    self.parser.process(&bytes);
                    self.captured.extend_from_slice(&bytes);
                    if self.captured.len() > 256 * 1024 {
                        let mut start = self.captured.len() - 256 * 1024;
                        while start < self.captured.len() && self.captured[start] & 0xc0 == 0x80 {
                            start += 1;
                        }
                        self.captured.drain(..start);
                    }
                }
                Event::Exited(status) => self.exit = Some(status),
                Event::Error(error) => self.error = Some(error),
            }
        }
        changed
    }

    pub(super) fn send(&mut self, bytes: Vec<u8>) {
        if self.exit.is_some() {
            return;
        }
        self.parser.screen_mut().set_scrollback(0);
        if self.input.send(bytes).is_err() {
            self.error = Some(
                "The shell is no longer accepting input. Restart the shell to reconnect.".into(),
            );
        }
    }

    pub(super) fn captured_output(&self) -> String {
        String::from_utf8_lossy(&self.captured).into_owned()
    }

    pub(super) fn paste(&mut self, text: &str) {
        // Bracketed paste prevents pasted newlines from being mistaken for typed Enter.
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if self.screen().bracketed_paste() {
            let text = text.replace('\x1b', "");
            self.send(format!("\x1b[200~{text}\x1b[201~").into_bytes());
        } else {
            self.send(text.replace('\n', "\r").into_bytes());
        }
    }

    pub(super) fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (nonzero(rows), nonzero(cols));
        if self.screen().size() == (rows, cols) {
            return;
        }
        match self.master.resize(PtySize {
            rows: rows.get(),
            cols: cols.get(),
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(()) => self.parser.set_size(rows, cols),
            Err(error) => self.error = Some(format!("Could not resize the terminal: {error}")),
        }
    }

    pub(super) fn scroll(&mut self, rows: i32) {
        let offset = self
            .screen()
            .scrollback()
            .saturating_add_signed(rows as isize);
        self.parser.screen_mut().set_scrollback(offset);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The waiting thread reaps the shell; closing the PTY releases its I/O threads.
        if self.exit.is_none() {
            _ = self.killer.kill();
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn shell(directory: &Path) -> Session {
        let mut command = CommandBuilder::new("/bin/sh");
        command.arg("-i");
        command.env("PS1", "anode-test> ");
        Session::spawn_command(directory, command).unwrap()
    }

    fn wait_for(session: &mut Session, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            session.drain();
            if session.screen().contents().contains(expected) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Missing {expected:?}: {}",
                session.screen().contents()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn real_shell_uses_project_cwd_and_preserves_state() {
        let directory = tempfile::Builder::new()
            .prefix("anode shell ")
            .tempdir()
            .unwrap();
        let mut session = shell(directory.path());
        wait_for(&mut session, "anode-test>");
        session
            .send(b"test -t 0 && printf 'PTY_%s\\n' ready; pwd; ANODE_VALUE=persistent\r".to_vec());
        wait_for(&mut session, "PTY_ready");
        wait_for(
            &mut session,
            directory.path().file_name().unwrap().to_str().unwrap(),
        );
        session.send(b"printf 'VALUE_%s\\n' \"$ANODE_VALUE\"\r".to_vec());
        wait_for(&mut session, "VALUE_persistent");
    }

    #[test]
    fn resize_and_interrupt_reach_the_foreground_process() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = shell(directory.path());
        wait_for(&mut session, "anode-test>");
        session.resize(30, 100);
        session.send(b"stty size; sleep 30; printf 'SLEEP_%s\\n' done\r".to_vec());
        wait_for(&mut session, "30 100");
        session.send(vec![3]);
        session.send(b"printf 'INTERRUPT_%s\\n' recovered\r".to_vec());
        wait_for(&mut session, "INTERRUPT_recovered");
    }

    #[test]
    fn separate_project_sessions_keep_independent_shell_state() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let mut a = shell(first.path());
        let mut b = shell(second.path());
        a.send(b"ANODE_SESSION=first\r".to_vec());
        b.send(b"ANODE_SESSION=second\r".to_vec());
        a.send(b"printf 'SESSION_%s\\n' \"$ANODE_SESSION\"\r".to_vec());
        b.send(b"printf 'SESSION_%s\\n' \"$ANODE_SESSION\"\r".to_vec());
        wait_for(&mut a, "SESSION_first");
        wait_for(&mut b, "SESSION_second");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn zsh_supports_completion_history_and_bracketed_paste() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("completion-fixture.txt"),
            "COMPLETION_ready\n",
        )
        .unwrap();
        let mut command = CommandBuilder::new("/bin/zsh");
        command.args(["-f", "-i"]);
        command.env("PS1", "anode-test> ");
        let mut session = Session::spawn_command(directory.path(), command).unwrap();
        wait_for(&mut session, "anode-test>");
        session.send(b"cat completion-f\t\r".to_vec());
        wait_for(&mut session, "COMPLETION_ready");
        // A real line editor handles history and bracketed paste, not application code.
        session.send(b"\x1b[A\r".to_vec());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            session.drain();
            if session
                .screen()
                .contents()
                .matches("COMPLETION_ready")
                .count()
                >= 2
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "history did not replay the command"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        session.paste("printf 'PASTE_%s\\n' ready");
        session.send(b"\r".to_vec());
        wait_for(&mut session, "PASTE_ready");
    }

    #[test]
    fn no_project_starts_in_home_directory() {
        assert_eq!(starting_directory(None), dirs::home_dir().unwrap());
    }
}
