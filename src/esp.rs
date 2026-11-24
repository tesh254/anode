//! What `esptool` knows about an Espressif board, and what Anode does with it.
//!
//! A USB-UART bridge — a CH343, a CP210x, an FTDI — says nothing about the
//! chip behind it. The same `1a86:55d3` descriptor fronts an ESP32, an
//! ESP32-S3 and plenty of boards that are not Espressif at all. The silicon
//! itself is the only authority, and `esptool` can ask it in a couple of
//! seconds, so this module asks rather than guesses: the chip, its revision,
//! its flash size and its PSRAM all come from the board.
//!
//! Everything the answer decides — the firmware variant, the write offset,
//! the baud rate, whether to erase first — is derived here, so no flash ever
//! has to stop and ask the user for a setting the board already knows.
//!
//! Every function that spawns `esptool` blocks, so all of them belong on a
//! background thread.

use crate::boards::{BoardDef, Chip, PortIdentity};
use crate::device::{run_streaming, DeviceEvent};
use crate::toolchain::{self, Tool};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// esptool's two command vocabularies
// ---------------------------------------------------------------------------

/// esptool v5 renamed every command and reset mode from `write_flash` to
/// `write-flash`. v4 understands only the old spelling and v5 only warns about
/// it, so either spelling works everywhere — but the warnings are the first
/// three lines of every flash, which is exactly the noise this module exists
/// to remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    /// esptool 4 and earlier: `write_flash`, `default_reset`.
    Underscore,
    /// esptool 5 and later: `write-flash`, `default-reset`.
    Hyphen,
}

impl Syntax {
    /// The spelling for a command or reset mode written here with underscores.
    pub fn spell(self, name: &'static str) -> String {
        match self {
            Syntax::Underscore => name.to_string(),
            Syntax::Hyphen => name.replace('_', "-"),
        }
    }
}

/// Pick the spelling from a reported version string, e.g. `esptool v5.2.0` or
/// `esptool.py v4.7.0`. An unreadable version keeps the older spelling, which
/// both majors accept.
pub fn syntax_for_version(version: &str) -> Syntax {
    let major = version
        .split_whitespace()
        .filter_map(|word| word.trim_start_matches('v').split('.').next())
        .filter_map(|digits| digits.parse::<u32>().ok())
        .next_back();
    match major {
        Some(major) if major >= 5 => Syntax::Hyphen,
        _ => Syntax::Underscore,
    }
}

/// The installed esptool's spelling, asked for once per process.
pub fn syntax() -> Syntax {
    static SYNTAX: OnceLock<Syntax> = OnceLock::new();
    *SYNTAX.get_or_init(|| {
        let reported = toolchain::command_for(Tool::Esptool)
            .and_then(|mut command| command.arg("version").output().ok())
            .map(|output| {
                let text = if output.stdout.is_empty() {
                    output.stderr
                } else {
                    output.stdout
                };
                String::from_utf8_lossy(&text).to_string()
            })
            .unwrap_or_default();
        let first = reported
            .lines()
            .find(|line| line.to_ascii_lowercase().contains("esptool"))
            .unwrap_or("");
        syntax_for_version(first)
    })
}

// ---------------------------------------------------------------------------
// What the chip reports about itself
// ---------------------------------------------------------------------------

/// The PSRAM a chip reports, which decides the MicroPython build it wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Psram {
    pub megabytes: u32,
    /// Octal PSRAM needs its own firmware build; quad runs the plain one.
    pub octal: bool,
}

/// What `esptool` found on the other end of the port.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChipReport {
    pub chip: Option<Chip>,
    /// The chip name exactly as esptool printed it, kept for the cases where
    /// Anode's catalog has no entry for that part yet.
    pub reported_name: Option<String>,
    pub revision: Option<String>,
    pub flash_size: Option<String>,
    pub psram: Option<Psram>,
    pub mac: Option<String>,
}

impl ChipReport {
    /// One line naming what is on the port, in the words the board used.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        parts.push(
            self.chip
                .map(|chip| chip.label().to_string())
                .or_else(|| self.reported_name.clone())
                .unwrap_or_else(|| "Unknown Espressif chip".into()),
        );
        if let Some(revision) = &self.revision {
            parts.push(format!("rev {revision}"));
        }
        if let Some(flash) = &self.flash_size {
            parts.push(format!("{flash} flash"));
        }
        if let Some(psram) = self.psram {
            parts.push(format!(
                "{} MB {} PSRAM",
                psram.megabytes,
                if psram.octal { "octal" } else { "quad" }
            ));
        }
        parts.join(" · ")
    }
}

/// esptool's chip names, as `--chip` spells them.
pub fn chip_from_esptool_name(name: &str) -> Option<Chip> {
    let normalized: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    Some(match normalized.as_str() {
        "esp8266" => Chip::Esp8266,
        "esp32" => Chip::Esp32,
        "esp32s2" => Chip::Esp32S2,
        "esp32s3" => Chip::Esp32S3,
        "esp32c3" => Chip::Esp32C3,
        "esp32c6" => Chip::Esp32C6,
        "esp32h2" => Chip::Esp32H2,
        _ => return None,
    })
}

/// Read a chip report out of an esptool run's output. Both the `Chip type:`
/// line of a modern run and the older `Chip is …` line are accepted, so the
/// parse does not depend on which esptool is installed.
pub fn parse_report(output: &str) -> ChipReport {
    let mut report = ChipReport::default();
    for line in output.lines() {
        let line = line.trim();
        let described = line
            .strip_prefix("Chip type:")
            .or_else(|| line.strip_prefix("Chip is"))
            .or_else(|| line.strip_prefix("Detecting chip type..."));
        if let Some(described) = described {
            let described = described.trim();
            if described.is_empty() || described.eq_ignore_ascii_case("unsupported detection") {
                continue;
            }
            // e.g. "ESP32-S3 (QFN56) (revision v0.2)"
            let name = described.split_whitespace().next().unwrap_or("");
            if report.chip.is_none() {
                report.chip = chip_from_esptool_name(name);
            }
            if report.reported_name.is_none() && !name.is_empty() {
                report.reported_name = Some(name.to_string());
            }
            if report.revision.is_none() {
                if let Some(revision) = described.split("revision ").nth(1) {
                    report.revision = Some(
                        revision
                            .trim_end_matches(')')
                            .trim()
                            .trim_start_matches('v')
                            .into(),
                    );
                }
            }
        }
        if let Some(rest) = line.strip_prefix("Detected flash size:") {
            report.flash_size = Some(rest.trim().to_string());
        }
        if let Some(rest) = line.strip_prefix("MAC:") {
            report.mac = Some(rest.trim().to_string());
        }
        if line.starts_with("Features:") && report.psram.is_none() {
            report.psram = parse_psram(line);
        }
    }
    report
}

/// Pull the PSRAM out of esptool's `Features:` line, e.g.
/// `Features: Wi-Fi, BT 5 (LE), Dual Core + LP Core, 240MHz, Embedded PSRAM 8MB (AP_3v3)`.
///
/// Espressif ships the ESP32-S3 with either 2 MB of quad PSRAM (the R2 part)
/// or 8 MB of octal PSRAM (the R8); the size is the reliable way to tell them
/// apart from the outside, and the two want different MicroPython builds.
fn parse_psram(features: &str) -> Option<Psram> {
    let lowered = features.to_ascii_lowercase();
    let at = lowered.find("psram")?;
    let after = &features[at + "psram".len()..];
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    let megabytes: u32 = digits.parse().ok()?;
    if megabytes == 0 {
        return None;
    }
    let octal = lowered.contains("octal") || megabytes >= 8;
    Some(Psram { megabytes, octal })
}

// ---------------------------------------------------------------------------
// Probing a port
// ---------------------------------------------------------------------------

/// Why a probe did not come back with a chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// esptool is not installed, so nothing can be asked.
    ToolMissing,
    /// The port is held by something else.
    PortBusy(String),
    /// Nothing answered the bootloader protocol. On a port that was only
    /// *suspected* of fronting an Espressif chip, this is the normal answer
    /// for "it is something else".
    NoAnswer(String),
    /// esptool ran and failed for a reason worth repeating.
    Failed(String),
}

impl ProbeError {
    pub fn message(&self) -> String {
        match self {
            ProbeError::ToolMissing => format!(
                "{} is not installed. {}",
                Tool::Esptool.name(),
                Tool::Esptool.install_hint()
            ),
            ProbeError::PortBusy(detail) | ProbeError::NoAnswer(detail) => detail.clone(),
            ProbeError::Failed(detail) => detail.clone(),
        }
    }
}

/// Ask the chip on `port_path` what it is.
///
/// `attempts` bounds how long a port that is *not* an Espressif chip can hold
/// this up: esptool retries the bootloader handshake, and a port fronting an
/// AVR or an STM32 will never answer, so a speculative probe passes a small
/// number and a deliberate one can afford more.
pub fn probe(
    port_path: &str,
    attempts: u32,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<ChipReport, ProbeError> {
    let Some(mut command) = toolchain::command_for(Tool::Esptool) else {
        return Err(ProbeError::ToolMissing);
    };
    let syntax = syntax();
    command.args([
        "--port",
        port_path,
        "--connect-attempts",
        &attempts.to_string(),
        "--before",
        &syntax.spell("default_reset"),
        "--after",
        &syntax.spell("hard_reset"),
        &syntax.spell("flash_id"),
    ]);
    let outcome = run_streaming(&mut command, emit).map_err(ProbeError::Failed)?;
    if !outcome.status.success() {
        return Err(classify_probe_failure(&outcome.tail));
    }
    let report = parse_report(&outcome.tail);
    if report.chip.is_none() && report.reported_name.is_none() {
        return Err(ProbeError::NoAnswer(
            "esptool connected but did not name a chip.".into(),
        ));
    }
    Ok(report)
}

fn classify_probe_failure(tail: &str) -> ProbeError {
    let diagnosis = diagnose(tail, None);
    match diagnosis.kind {
        FailureKind::PortBusy => ProbeError::PortBusy(diagnosis.what),
        FailureKind::NoAnswer => ProbeError::NoAnswer(diagnosis.what),
        _ => ProbeError::Failed(diagnosis.what),
    }
}

// ---------------------------------------------------------------------------
// The settings a write needs, all derived
// ---------------------------------------------------------------------------

/// How fast a write can run on this connection.
///
/// A chip on its own USB-serial-JTAG peripheral is a CDC device: the baud rate
/// is a formality and the link runs at USB speed either way, so the higher
/// number costs nothing. Behind a UART bridge the number is real, and 460800
/// is the rate Espressif's own tooling treats as safe across the bridges in
/// circulation.
pub const NATIVE_USB_BAUD: u32 = 921_600;
pub const BRIDGE_BAUD: u32 = 460_800;
/// The rate to fall back to when a faster one could not keep a connection.
pub const SLOW_BAUD: u32 = 115_200;

/// Everything a firmware write needs, worked out from the board and the chip
/// rather than asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashPlan {
    pub chip: Chip,
    /// The offset a full image is written at, decided by the chip's
    /// second-stage bootloader layout.
    pub offset: &'static str,
    pub baud: u32,
    /// Whether to erase the whole flash before writing. A full image replaces
    /// the bootloader, the partition table and the application, but not a
    /// filesystem left behind by the firmware being replaced — and a stale
    /// filesystem is the usual reason a freshly flashed board will not boot.
    pub erase_first: bool,
}

impl FlashPlan {
    /// The plan for writing a complete firmware image to `chip` on a port with
    /// the given connection kind.
    pub fn full_image(chip: Chip, native_usb: bool) -> Self {
        Self {
            chip,
            offset: chip.flash_offset(),
            baud: if native_usb {
                NATIVE_USB_BAUD
            } else {
                BRIDGE_BAUD
            },
            erase_first: true,
        }
    }

    /// The same plan at the conservative rate, for one retry after a
    /// connection that could not be held.
    pub fn slowed(&self) -> Self {
        Self {
            baud: SLOW_BAUD,
            ..self.clone()
        }
    }

    /// The settings, as the confirmation dialog states them.
    pub fn detail(&self) -> Vec<String> {
        vec![
            format!("Chip: {}", self.chip.label()),
            format!(
                "Write: {} at {}",
                if self.erase_first {
                    "erase, then"
                } else {
                    "image"
                },
                self.offset
            ),
            format!("Speed: {} baud", self.baud),
        ]
    }
}

/// True when this port is the Espressif chip's own USB peripheral rather than
/// a bridge in front of it.
pub fn is_native_usb(port: &PortIdentity) -> bool {
    port.vendor_id
        .as_deref()
        .is_some_and(|vid| vid.eq_ignore_ascii_case("303a"))
}

/// The MicroPython build this board wants, given what the chip reported.
///
/// micropython.org publishes one page per board variant family and lists the
/// PSRAM builds on it under their own `-SPIRAM_OCT` name. A generic ESP32-S3
/// entry covers both the plain part and the R8 with 8 MB of octal PSRAM, so
/// the chip's own report is what picks between them.
pub fn micropython_variant(board: &BoardDef, report: Option<&ChipReport>) -> Option<String> {
    let variant = board.mpy_variant?;
    // Only the generic per-chip entries are ambiguous. A named board already
    // states the build its maker ships.
    if !board.generic || board.chip != Chip::Esp32S3 {
        return Some(variant.to_string());
    }
    match report.and_then(|report| report.psram) {
        Some(psram) if psram.octal => Some(format!("{variant}-SPIRAM_OCT")),
        _ => Some(variant.to_string()),
    }
}

/// The download page that lists a variant's builds. A PSRAM build lives on its
/// family's page, under its own file name, not on a page of its own.
pub fn micropython_download_page(variant: &str) -> &str {
    variant.split('-').next().unwrap_or(variant)
}

// ---------------------------------------------------------------------------
// What a failure means
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Another program holds the serial port.
    PortBusy,
    /// The operating system refused the port.
    Denied,
    /// Nothing answered the bootloader handshake.
    NoAnswer,
    /// The chip is not the one the image was built for.
    WrongChip,
    /// The port went away mid-operation.
    Disconnected,
    /// The link could not be held at this rate.
    LinkUnstable,
    /// Something else esptool reported.
    Other,
}

/// A failure stated as what happened and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    pub kind: FailureKind,
    pub what: String,
    pub next: Option<String>,
}

impl Diagnosis {
    /// The one message to show.
    pub fn message(&self) -> String {
        match &self.next {
            Some(next) => format!("{}\n{next}", self.what),
            None => self.what.clone(),
        }
    }
}

/// Read an esptool failure. `expected` is the chip the image was built for,
/// which lets a chip mismatch name both sides.
pub fn diagnose(tail: &str, expected: Option<Chip>) -> Diagnosis {
    let text = tail.to_ascii_lowercase();
    let last = tail
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("Flashing failed")
        .to_string();

    if text.contains("resource busy")
        || text.contains("errno 16")
        || text.contains("device or resource busy")
        || (text.contains("could not open") && text.contains("busy"))
    {
        return Diagnosis {
            kind: FailureKind::PortBusy,
            what: "The serial port is open in another program.".into(),
            next: Some(
                "Close any serial monitor, REPL or terminal using this board, then flash again."
                    .into(),
            ),
        };
    }
    if text.contains("permission denied") || text.contains("errno 13") {
        return Diagnosis {
            kind: FailureKind::Denied,
            what: "The operating system refused access to the serial port.".into(),
            next: Some(
                "Check that no other account has the board open, and that your user may use \
                 serial devices."
                    .into(),
            ),
        };
    }
    if text.contains("no such file or directory")
        || text.contains("device not configured")
        || text.contains("could not open port")
    {
        return Diagnosis {
            kind: FailureKind::Disconnected,
            what: "The serial port disappeared.".into(),
            next: Some("Reconnect the board over USB, then flash again.".into()),
        };
    }
    if let Some(found) = wrong_chip(tail) {
        let expected = expected
            .map(|chip| chip.label().to_string())
            .unwrap_or_else(|| "the selected board's chip".into());
        return Diagnosis {
            kind: FailureKind::WrongChip,
            what: format!("This board is an {found}, not {expected}."),
            next: Some(format!(
                "Change the board model to an {found} board and flash again."
            )),
        };
    }
    if text.contains("timed out waiting for packet header")
        || text.contains("invalid head of packet")
        || text.contains("corrupt data")
    {
        return Diagnosis {
            kind: FailureKind::LinkUnstable,
            what: "The link to the board dropped packets.".into(),
            next: Some(
                "Try a shorter or different USB cable, or a port directly on the computer rather \
                 than through a hub."
                    .into(),
            ),
        };
    }
    if text.contains("no serial data received")
        || text.contains("failed to connect")
        || text.contains("chip stopped responding")
        || text.contains("wrong boot mode detected")
    {
        return Diagnosis {
            kind: FailureKind::NoAnswer,
            what: "The board did not answer its bootloader.".into(),
            next: Some(
                "Hold BOOT, tap RESET, release BOOT, then flash again. Some boards also need a \
                 USB cable that carries data rather than power only."
                    .into(),
            ),
        };
    }
    Diagnosis {
        kind: FailureKind::Other,
        what: last,
        next: None,
    }
}

/// esptool reports a mismatch as "This chip is ESP32-S3, not ESP32". Pull the
/// chip it actually found out of that sentence.
fn wrong_chip(tail: &str) -> Option<String> {
    let line = tail
        .lines()
        .find(|line| line.to_ascii_lowercase().contains("this chip is"))?;
    let rest = line.split("this chip is").nth(1).or_else(|| {
        let at = line.to_ascii_lowercase().find("this chip is")?;
        Some(&line[at + "this chip is".len()..])
    })?;
    let name = rest
        .trim()
        .trim_start_matches(|character: char| !character.is_ascii_alphanumeric())
        .split([',', ' '])
        .next()?
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

/// Where a flash has got to, for an indicator that is more honest than a
/// spinner. `percent` is present only while esptool is reporting one.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub phase: &'static str,
    pub percent: Option<f32>,
}

/// Read a progress line. Returns `None` for output that does not move the
/// indicator, so the caller keeps the last state rather than flickering.
pub fn progress_from_line(line: &str) -> Option<Progress> {
    let lowered = line.to_ascii_lowercase();
    let percent = percent_in(line);
    let phase = if lowered.starts_with("looking up") || lowered.starts_with("downloading") {
        "Downloading firmware"
    } else if lowered.starts_with("downloaded") {
        "Firmware downloaded"
    } else if lowered.contains("connecting") {
        "Connecting to the board"
    } else if lowered.contains("detecting chip type") || lowered.starts_with("chip type:") {
        "Identifying the chip"
    } else if lowered.contains("stub flasher") || lowered.contains("changing baud") {
        "Preparing the board"
    } else if lowered.contains("erasing") || lowered.contains("flash will be erased") {
        "Erasing flash"
    } else if lowered.starts_with("writing at") || lowered.starts_with("compressed") {
        "Writing firmware"
    } else if lowered.contains("hash of data verified") {
        "Verified"
    } else if lowered.contains("resetting") {
        "Restarting the board"
    } else {
        return percent.map(|percent| Progress {
            phase: "Writing firmware",
            percent: Some(percent),
        });
    };
    Some(Progress { phase, percent })
}

/// The percentage in a line like `Writing at 0x0001002f... (47 %)`.
fn percent_in(line: &str) -> Option<f32> {
    let at = line.rfind('%')?;
    let digits: String = line[..at]
        .chars()
        .rev()
        .skip_while(|character| character.is_whitespace())
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse::<f32>().ok().filter(|value| *value <= 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::board_by_id;

    /// The real output of `esptool flash-id` against the ESP32-S3 on the desk.
    const S3_REPORT: &str = "\
esptool v5.2.0
Serial port /dev/cu.usbmodem5B5E1061311:
Connecting....
Detecting chip type... ESP32-S3
Connected to ESP32-S3 on /dev/cu.usbmodem5B5E1061311:
Chip type:          ESP32-S3 (QFN56) (revision v0.2)
Features:           Wi-Fi, BT 5 (LE), Dual Core + LP Core, 240MHz, Embedded PSRAM 8MB (AP_3v3)
Crystal frequency:  40MHz
MAC:                1c:db:d4:78:54:ec
Uploading stub flasher...
Detected flash size: 16MB
Hard resetting via RTS pin...
";

    #[test]
    fn a_real_s3_report_is_read_in_full() {
        let report = parse_report(S3_REPORT);
        assert_eq!(report.chip, Some(Chip::Esp32S3));
        assert_eq!(report.revision.as_deref(), Some("0.2"));
        assert_eq!(report.flash_size.as_deref(), Some("16MB"));
        assert_eq!(report.mac.as_deref(), Some("1c:db:d4:78:54:ec"));
        assert_eq!(
            report.psram,
            Some(Psram {
                megabytes: 8,
                octal: true
            })
        );
        assert!(report.summary().contains("ESP32-S3"));
        assert!(report.summary().contains("16MB flash"));
    }

    #[test]
    fn an_older_esptool_phrasing_still_names_the_chip() {
        let report = parse_report("Chip is ESP32-C3 (revision v0.4)\nMAC: aa:bb:cc:dd:ee:ff\n");
        assert_eq!(report.chip, Some(Chip::Esp32C3));
        assert_eq!(report.revision.as_deref(), Some("0.4"));
    }

    #[test]
    fn a_chip_the_catalog_has_no_entry_for_is_still_reported_by_name() {
        let report = parse_report("Chip type:          ESP32-P4 (revision v1.0)\n");
        assert_eq!(report.chip, None);
        assert_eq!(report.reported_name.as_deref(), Some("ESP32-P4"));
        assert!(report.summary().contains("ESP32-P4"));
    }

    #[test]
    fn quad_psram_is_distinguished_from_octal() {
        let quad = parse_report("Features: Wi-Fi, Dual Core, 240MHz, Embedded PSRAM 2MB (AP_3v3)");
        assert_eq!(
            quad.psram,
            Some(Psram {
                megabytes: 2,
                octal: false
            })
        );
        let none = parse_report("Features: Wi-Fi, BT 5 (LE), Dual Core, 240MHz");
        assert_eq!(none.psram, None);
    }

    #[test]
    fn esptool_major_version_chooses_the_command_spelling() {
        assert_eq!(syntax_for_version("esptool v5.2.0"), Syntax::Hyphen);
        assert_eq!(syntax_for_version("esptool.py v4.7.0"), Syntax::Underscore);
        assert_eq!(syntax_for_version("nonsense"), Syntax::Underscore);
        assert_eq!(Syntax::Hyphen.spell("write_flash"), "write-flash");
        assert_eq!(Syntax::Underscore.spell("write_flash"), "write_flash");
    }

    #[test]
    fn the_plan_follows_the_chip_and_the_connection() {
        let native = FlashPlan::full_image(Chip::Esp32S3, true);
        assert_eq!(native.offset, "0x0");
        assert_eq!(native.baud, NATIVE_USB_BAUD);
        assert!(native.erase_first);

        let bridged = FlashPlan::full_image(Chip::Esp32, false);
        assert_eq!(
            bridged.offset, "0x1000",
            "the original ESP32 starts at 0x1000"
        );
        assert_eq!(bridged.baud, BRIDGE_BAUD);
        assert_eq!(bridged.slowed().baud, SLOW_BAUD);
        assert_eq!(bridged.slowed().offset, "0x1000");
    }

    #[test]
    fn a_native_usb_descriptor_is_told_from_a_bridge() {
        let native = PortIdentity {
            vendor_id: Some("303A".into()),
            ..Default::default()
        };
        let bridge = PortIdentity {
            vendor_id: Some("1a86".into()),
            ..Default::default()
        };
        assert!(is_native_usb(&native), "case does not change the vendor");
        assert!(!is_native_usb(&bridge));
        assert!(!is_native_usb(&PortIdentity::default()));
    }

    #[test]
    fn an_s3_with_octal_psram_gets_the_psram_build() {
        let generic = board_by_id("esp32-s3-generic").unwrap();
        let octal = ChipReport {
            psram: Some(Psram {
                megabytes: 8,
                octal: true,
            }),
            ..Default::default()
        };
        assert_eq!(
            micropython_variant(generic, Some(&octal)).as_deref(),
            Some("ESP32_GENERIC_S3-SPIRAM_OCT")
        );
        assert_eq!(
            micropython_download_page("ESP32_GENERIC_S3-SPIRAM_OCT"),
            "ESP32_GENERIC_S3"
        );

        // Without PSRAM, and without a report at all, the plain build stands.
        assert_eq!(
            micropython_variant(generic, None).as_deref(),
            Some("ESP32_GENERIC_S3")
        );
        assert_eq!(
            micropython_variant(generic, Some(&ChipReport::default())).as_deref(),
            Some("ESP32_GENERIC_S3")
        );
    }

    #[test]
    fn a_named_board_keeps_the_build_its_maker_ships() {
        let xiao = board_by_id("xiao-esp32s3").unwrap();
        let octal = ChipReport {
            psram: Some(Psram {
                megabytes: 8,
                octal: true,
            }),
            ..Default::default()
        };
        assert_eq!(
            micropython_variant(xiao, Some(&octal)).as_deref(),
            xiao.mpy_variant
        );
    }

    #[test]
    fn a_held_port_is_named_as_such_rather_than_blamed_on_the_board() {
        let diagnosis = diagnose(
            "serial.serialutil.SerialException: could not open port /dev/cu.usbmodem1: \
             [Errno 16] Resource busy",
            Some(Chip::Esp32S3),
        );
        assert_eq!(diagnosis.kind, FailureKind::PortBusy);
        assert!(diagnosis.message().contains("another program"));
        assert!(diagnosis.message().to_lowercase().contains("close"));
    }

    #[test]
    fn a_silent_bootloader_says_which_buttons_to_press() {
        let diagnosis = diagnose(
            "A fatal error occurred: Failed to connect to ESP32-S3: No serial data received.",
            Some(Chip::Esp32S3),
        );
        assert_eq!(diagnosis.kind, FailureKind::NoAnswer);
        assert!(diagnosis.message().contains("BOOT"));
    }

    #[test]
    fn a_chip_mismatch_names_both_chips() {
        let diagnosis = diagnose(
            "A fatal error occurred: This chip is ESP32-S3, not ESP32. Wrong --chip argument?",
            Some(Chip::Esp32),
        );
        assert_eq!(diagnosis.kind, FailureKind::WrongChip);
        assert!(diagnosis.what.contains("ESP32-S3"));
        assert!(diagnosis.what.contains("ESP32."));
        assert!(diagnosis.next.unwrap().contains("board model"));
    }

    #[test]
    fn an_unstable_link_blames_the_cable_not_the_user() {
        let diagnosis = diagnose("Timed out waiting for packet header", None);
        assert_eq!(diagnosis.kind, FailureKind::LinkUnstable);
        assert!(diagnosis.message().contains("cable"));
    }

    #[test]
    fn an_unrecognised_failure_repeats_the_last_line_rather_than_inventing_one() {
        let diagnosis = diagnose(
            "some output\nA fatal error occurred: unreadable efuse\n",
            None,
        );
        assert_eq!(diagnosis.kind, FailureKind::Other);
        assert_eq!(diagnosis.what, "A fatal error occurred: unreadable efuse");
        assert_eq!(diagnosis.next, None);
    }

    #[test]
    fn progress_follows_a_whole_esptool_run() {
        let phases: Vec<_> = [
            "Looking up the latest MicroPython build for ESP32_GENERIC_S3…",
            "Connecting....",
            "Detecting chip type... ESP32-S3",
            "Uploading stub flasher...",
            "Flash will be erased from 0x00000000 to 0x003fffff...",
            "Writing at 0x0001002f... (47 %)",
            "Hash of data verified.",
            "Hard resetting via RTS pin...",
        ]
        .iter()
        .filter_map(|line| progress_from_line(line))
        .collect();

        assert_eq!(
            phases.iter().map(|p| p.phase).collect::<Vec<_>>(),
            vec![
                "Downloading firmware",
                "Connecting to the board",
                "Identifying the chip",
                "Preparing the board",
                "Erasing flash",
                "Writing firmware",
                "Verified",
                "Restarting the board",
            ]
        );
        assert_eq!(phases[5].percent, Some(47.0));
        assert_eq!(phases[1].percent, None);
    }

    #[test]
    fn a_line_that_means_nothing_leaves_the_indicator_alone() {
        assert_eq!(progress_from_line("Serial port /dev/cu.usbmodem1"), None);
        assert_eq!(progress_from_line(""), None);
        // A bare percentage still moves the bar.
        assert_eq!(
            progress_from_line("  (100 %)").map(|p| p.percent),
            Some(Some(100.0))
        );
    }
}
