//! Device operations: run and upload code, flash firmware, deploy to
//! CircuitPython, build native projects, and drive `mpremote` for the device
//! file tree.
//!
//! Every function here blocks and spawns processes, so all of it runs on a
//! background thread. Progress reaches the UI as [`DeviceEvent`] lines.

use crate::boards::{BoardDef, Chip, FlashMethod, Runtime};
use crate::profiles::CircuitPythonDrive;
use crate::toolchain::{self, Tool};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// A progress line for the device log (already cleaned of ANSI codes).
    Progress(String),
}

/// Emit a progress line without the `DeviceEvent::Progress(...)` ceremony.
macro_rules! progress {
    ($emit:expr, $($arg:tt)*) => {
        $emit(DeviceEvent::Progress(format!($($arg)*)))
    };
}

/// Strip ANSI escape sequences so they don't pollute the log.
pub fn strip_ansi(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\x1b\[\d+(;\d+)*[a-zA-Z]").unwrap())
        .replace_all(text, "")
        .to_string()
}

/// What a streamed command left behind: how it exited, and the tail of its
/// output for an error message.
pub struct CommandOutcome {
    pub status: std::process::ExitStatus,
    pub tail: String,
}

impl CommandOutcome {
    fn into_result(self, failure: &str) -> Result<(), String> {
        if self.status.success() {
            Ok(())
        } else {
            let tail = self.tail.trim();
            Err(if tail.is_empty() {
                failure.to_string()
            } else {
                format!("{failure}\n{tail}")
            })
        }
    }
}

// ---------------------------------------------------------------------------
// mpremote
// ---------------------------------------------------------------------------

/// Result of an mpremote invocation with cleaned-up stderr.
struct MpremoteResult {
    success: bool,
    stdout: String,
    stderr: String,
}

impl MpremoteResult {
    /// The message worth showing, preferring the cleaned stderr.
    fn error(self) -> String {
        if self.stderr.is_empty() {
            self.stdout
        } else {
            self.stderr
        }
    }
}

/// Clean tracebacks out of mpremote stderr — keep only the meaningful error.
fn clean_mpremote_stderr(stderr: &str) -> String {
    if stderr.trim().is_empty() {
        return String::new();
    }
    if stderr.contains("could not enter raw repl") {
        return "could not enter raw REPL (board may be booting or port in use)".into();
    }
    if stderr.contains("it may be in use by another program") {
        return "serial port in use (disconnect REPL first)".into();
    }
    if stderr.contains("No module named mpremote") {
        return "mpremote not installed (pip install mpremote)".into();
    }
    if stderr.contains("Device not configured") {
        return "device disconnected during operation".into();
    }
    let lines: Vec<&str> = stderr
        .trim()
        .lines()
        .filter(|line| {
            let text = line.trim();
            !text.is_empty()
                && !text.starts_with("File \"")
                && !text.starts_with("~~~")
                && !text.starts_with('^')
                && !text.contains("runpy")
                && !text.contains("mpremote/")
                && !text.contains("site-packages")
                && !text.contains("Traceback")
                && !text.contains("During handling")
        })
        .collect();
    lines
        .last()
        .map(|line| line.to_string())
        .unwrap_or_else(|| stderr.trim().lines().next_back().unwrap_or("").to_string())
}

fn missing_tool(tool: Tool) -> String {
    format!("{} is not installed. {}", tool.name(), tool.install_hint())
}

fn run_mpremote(port_path: &str, sub_args: &[&str]) -> MpremoteResult {
    let Some(mut command) = toolchain::command_for(Tool::Mpremote) else {
        return MpremoteResult {
            success: false,
            stdout: String::new(),
            stderr: missing_tool(Tool::Mpremote),
        };
    };
    let output = command
        .args(["connect", port_path])
        .args(sub_args)
        .stdin(Stdio::null())
        .output();

    match output {
        Ok(out) => MpremoteResult {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: clean_mpremote_stderr(&String::from_utf8_lossy(&out.stderr)),
        },
        Err(error) => MpremoteResult {
            success: false,
            stdout: String::new(),
            stderr: error.to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// CircuitPython: deploy via the mounted CIRCUITPY drive
// ---------------------------------------------------------------------------

/// Roots under which removable volumes mount, per OS. macOS mounts them flat
/// under /Volumes; Linux distros use /media/<user> or /run/media/<user>.
fn volume_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    match std::env::consts::OS {
        "macos" => roots.push(PathBuf::from("/Volumes")),
        "linux" => {
            for root in ["/media", "/run/media"] {
                let root = Path::new(root);
                if !root.exists() {
                    continue;
                }
                // Both /media/<label> and /media/<user>/<label> are in use.
                roots.push(root.to_path_buf());
                if let Ok(users) = std::fs::read_dir(root) {
                    roots.extend(users.flatten().map(|user| user.path()));
                }
            }
        }
        // Windows mounts volumes as drive letters; enumerating them by label
        // needs platform APIs this build does not link.
        _ => {}
    }
    roots
}

/// Detect a connected CircuitPython board by its hallmark: a mounted USB
/// drive named CIRCUITPY containing boot_out.txt (board name + CP version).
pub fn detect_circuitpython() -> CircuitPythonDrive {
    for root in volume_roots() {
        let mount = root.join("CIRCUITPY");
        let boot_out = mount.join("boot_out.txt");
        if !boot_out.exists() {
            continue;
        }
        let (board_name, version) = std::fs::read_to_string(&boot_out)
            .map(|text| parse_boot_out(&text))
            .unwrap_or((None, None));
        return CircuitPythonDrive {
            present: true,
            mount_path: Some(mount.to_string_lossy().to_string()),
            board_name,
            version,
        };
    }
    CircuitPythonDrive::default()
}

/// Parse `boot_out.txt` into (board name, version).
pub fn parse_boot_out(text: &str) -> (Option<String>, Option<String>) {
    let mut board_name = None;
    let mut version = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("ADAFRUIT") {
            // e.g. "ADAFRUIT Feather ESP32-S3 ..."
            board_name = Some(
                rest.split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        if let Some(rest) = line.strip_prefix("Board ID:") {
            if board_name.is_none() {
                board_name = Some(rest.trim().to_string());
            }
        }
        if let Some(rest) = line.strip_prefix("Adafruit CircuitPython ") {
            version = Some(rest.split_whitespace().next().unwrap_or("").to_string());
        }
    }
    (board_name, version)
}

/// Copy a file onto the mounted CIRCUITPY drive. Writing triggers the board's
/// auto-reload, which re-runs code.py — no serial protocol, no port handoff.
pub fn circuitpython_deploy(
    drive: &CircuitPythonDrive,
    file_name: &str,
    code: &str,
) -> Result<String, String> {
    let Some(mount) = &drive.mount_path else {
        return Err("CIRCUITPY drive not mounted (reconnect the board)".into());
    };
    // Guard against a drive ejected between detection and this call.
    let mount = Path::new(mount);
    if !mount.join("boot_out.txt").exists() {
        return Err("CIRCUITPY drive not mounted (reconnect the board)".into());
    }
    // Disallow path traversal — write only to the drive root.
    let safe_name = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "invalid file name".to_string())?;
    let dest = mount.join(safe_name);
    std::fs::write(&dest, code).map_err(|error| error.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Run / upload via mpremote
// ---------------------------------------------------------------------------

/// Run Python code on the board via `mpremote run`, streaming progress.
pub fn run_code(
    port_path: &str,
    code: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let tmp = temporary_transfer_file(".py")?;
    std::fs::write(&tmp, code).map_err(|error| error.to_string())?;

    progress!(emit, "Running code on device…");
    let result = run_mpremote(port_path, &["run", tmp.to_string_lossy().as_ref()]);

    if result.success {
        for line in result.stdout.lines() {
            progress!(emit, "{line}");
        }
        progress!(emit, "[OK] Code execution finished");
        Ok(())
    } else {
        let error = result.error();
        Err(if error.is_empty() {
            "Run failed".into()
        } else {
            error
        })
    }
}

/// Soft-reset the board (the Stop button).
pub fn stop_code(port_path: &str, emit: &mut dyn FnMut(DeviceEvent)) -> Result<(), String> {
    progress!(emit, "Sending soft reset to device…");
    let result = run_mpremote(port_path, &["soft-reset"]);
    if result.success {
        progress!(emit, "[OK] Device reset");
        Ok(())
    } else {
        Err(result.error())
    }
}

/// Upload one file to the board via `mpremote cp`.
pub fn upload_code(
    port_path: &str,
    file_name: &str,
    code: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let tmp = temporary_transfer_file(".py")?;
    std::fs::write(&tmp, code).map_err(|error| error.to_string())?;

    progress!(emit, "Uploading {file_name} to {port_path}…");
    let result = run_mpremote(
        port_path,
        &[
            "fs",
            "cp",
            tmp.to_string_lossy().as_ref(),
            &format!(":{file_name}"),
        ],
    );

    if result.success {
        progress!(emit, "[OK] Uploaded {file_name}");
        return Ok(());
    }
    let error = result.error();
    Err(if error.is_empty() {
        format!("Could not upload {file_name}")
    } else {
        error
    })
}

// ---------------------------------------------------------------------------
// Firmware flashing
// ---------------------------------------------------------------------------

pub struct FlashOutcome {
    pub success: bool,
    pub error: Option<String>,
    pub hint: Option<String>,
}

impl FlashOutcome {
    fn ok() -> Self {
        Self {
            success: true,
            error: None,
            hint: None,
        }
    }

    fn failed(error: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error.into()),
            hint: None,
        }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// A failure already read for what it means.
    fn from_diagnosis(diagnosis: crate::esp::Diagnosis) -> Self {
        Self {
            success: false,
            error: Some(diagnosis.what),
            hint: diagnosis.next,
        }
    }

    /// The single message to show for a failure.
    pub fn message(self) -> String {
        let error = self.error.unwrap_or_else(|| "Flashing failed".into());
        match self.hint {
            Some(hint) => format!("{error}\n{hint}"),
            None => error,
        }
    }
}

/// Where the firmware image comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareSource {
    /// The latest MicroPython build for the board's variant.
    MicroPython,
    /// The latest CircuitPython build for the board's slug.
    CircuitPython,
    /// An image the user picked from disk.
    LocalFile(PathBuf),
}

impl FirmwareSource {
    pub fn label(&self) -> String {
        match self {
            FirmwareSource::MicroPython => "MicroPython".into(),
            FirmwareSource::CircuitPython => "CircuitPython".into(),
            FirmwareSource::LocalFile(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "Local file".into()),
        }
    }
}

/// One firmware write, with everything the board already told us about
/// itself.
///
/// The chip report and the connection kind are what let the write choose its
/// own chip argument, offset, baud rate and firmware variant instead of
/// asking, so they travel with the request rather than being looked up again
/// on the worker thread.
#[derive(Clone)]
pub struct FlashRequest {
    pub port_path: String,
    pub board: &'static BoardDef,
    pub source: FirmwareSource,
    /// What `esptool` reported about the chip, when it was asked.
    pub report: Option<crate::esp::ChipReport>,
    /// True when the port is the chip's own USB peripheral rather than a
    /// USB-UART bridge in front of it.
    pub native_usb: bool,
}

impl FlashRequest {
    /// The settings this write will use, for the confirmation that precedes
    /// it. `None` for a board that is not written over serial by esptool.
    pub fn plan(&self) -> Option<crate::esp::FlashPlan> {
        (self.board.flash == FlashMethod::Esptool)
            .then(|| crate::esp::FlashPlan::full_image(self.chip(), self.native_usb))
    }

    /// The chip to write, preferring what the silicon said over what the
    /// catalog entry assumes.
    fn chip(&self) -> Chip {
        self.report
            .as_ref()
            .and_then(|report| report.chip)
            .filter(|chip| chip.is_espressif())
            .unwrap_or(self.board.chip)
    }

    /// The firmware image this write will fetch, named as the user sees it.
    pub fn image_label(&self) -> String {
        match &self.source {
            FirmwareSource::MicroPython => {
                crate::esp::micropython_variant(self.board, self.report.as_ref())
                    .unwrap_or_else(|| "MicroPython".into())
            }
            other => other.label(),
        }
    }
}

/// Download (or take) a firmware image and write it to the board.
pub fn flash_firmware(request: FlashRequest, emit: &mut dyn FnMut(DeviceEvent)) -> FlashOutcome {
    let FlashRequest {
        ref port_path,
        board,
        ref source,
        ref report,
        ..
    } = request;
    let port_path = port_path.as_str();
    progress!(
        emit,
        "Flashing {} on {} · {}",
        board.name,
        port_path,
        source.label()
    );

    if board.flash == FlashMethod::Unsupported {
        return FlashOutcome::failed(format!(
            "{} has no firmware-flashing path in Anode.",
            board.name
        ))
        .with_hint("Compile and upload a sketch or a native project instead.");
    }

    // A temp path has to outlive the write, so keep the guard in scope.
    let downloaded;
    let firmware: &Path = match &source {
        FirmwareSource::LocalFile(path) => {
            if !path.is_file() {
                return FlashOutcome::failed(format!("{} no longer exists.", path.display()));
            }
            path
        }
        FirmwareSource::MicroPython => {
            // The chip's own report decides the build: an ESP32-S3 with octal
            // PSRAM wants a different image from the same board without it.
            let Some(variant) = crate::esp::micropython_variant(board, report.as_ref()) else {
                return FlashOutcome::failed(format!(
                    "No MicroPython build is published for {}.",
                    board.name
                ))
                .with_hint("Pick a different firmware source, or flash an image from disk.");
            };
            match download_micropython(&variant, board.flash.firmware_extension(), emit) {
                Ok(path) => {
                    downloaded = path;
                    downloaded.as_ref()
                }
                Err(error) => return FlashOutcome::failed(error),
            }
        }
        FirmwareSource::CircuitPython => {
            let Some(slug) = board.cpy_board else {
                return FlashOutcome::failed(format!(
                    "Anode does not know the CircuitPython board id for {}.",
                    board.name
                ))
                .with_hint("Download the image from circuitpython.org, then flash it from disk.");
            };
            match download_circuitpython(slug, board.flash.firmware_extension(), emit) {
                Ok(path) => {
                    downloaded = path;
                    downloaded.as_ref()
                }
                Err(error) => return FlashOutcome::failed(error),
            }
        }
    };

    write_firmware(&request, firmware, emit)
}

/// Write an image that is already on disk, using the board's flash method.
fn write_firmware(
    request: &FlashRequest,
    firmware: &Path,
    emit: &mut dyn FnMut(DeviceEvent),
) -> FlashOutcome {
    let board = request.board;
    let port_path = request.port_path.as_str();
    match board.flash {
        FlashMethod::Esptool => write_with_esptool(
            port_path,
            crate::esp::FlashPlan::full_image(request.chip(), request.native_usb),
            firmware,
            emit,
        ),
        FlashMethod::MassStorage { volume, marker } => {
            copy_to_boot_volume(volume, marker, firmware, emit)
        }
        FlashMethod::DfuUtil => write_with_dfu_util(firmware, emit),
        FlashMethod::Bossac => write_with_bossac(port_path, firmware, emit),
        FlashMethod::TeensyLoader => write_with_teensy_loader(board.chip, firmware, emit),
        FlashMethod::Unsupported => FlashOutcome::failed("This board cannot be flashed directly."),
    }
}

/// Write a full image with `esptool`, using the settings the chip and the
/// connection already decided.
///
/// A marginal link — a long cable, a hub, a bridge that cannot hold the
/// faster rate — is the one failure worth answering without the user: the
/// write is retried once at the conservative rate before anything is
/// reported, because "try a slower speed" is advice Anode can simply follow.
fn write_with_esptool(
    port_path: &str,
    plan: crate::esp::FlashPlan,
    firmware: &Path,
    emit: &mut dyn FnMut(DeviceEvent),
) -> FlashOutcome {
    progress!(
        emit,
        "Writing {} to the {} at {} ({} baud){}",
        firmware.display(),
        plan.chip.label(),
        plan.offset,
        plan.baud,
        if plan.erase_first {
            ", erasing first"
        } else {
            ""
        }
    );

    match esptool_write(port_path, &plan, firmware, emit) {
        Ok(()) => {
            progress!(emit, "[OK] Firmware written and verified");
            FlashOutcome::ok()
        }
        Err(diagnosis) if diagnosis.kind == crate::esp::FailureKind::LinkUnstable => {
            let slower = plan.slowed();
            progress!(
                emit,
                "The link dropped packets at {} baud. Retrying at {}…",
                plan.baud,
                slower.baud
            );
            match esptool_write(port_path, &slower, firmware, emit) {
                Ok(()) => {
                    progress!(
                        emit,
                        "[OK] Firmware written and verified at {} baud",
                        slower.baud
                    );
                    FlashOutcome::ok()
                }
                Err(diagnosis) => FlashOutcome::from_diagnosis(diagnosis),
            }
        }
        Err(diagnosis) => FlashOutcome::from_diagnosis(diagnosis),
    }
}

/// One `esptool write_flash` run.
fn esptool_write(
    port_path: &str,
    plan: &crate::esp::FlashPlan,
    firmware: &Path,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), crate::esp::Diagnosis> {
    let Some(mut command) = toolchain::command_for(Tool::Esptool) else {
        return Err(crate::esp::Diagnosis {
            kind: crate::esp::FailureKind::Other,
            what: missing_tool(Tool::Esptool),
            next: None,
        });
    };
    let syntax = crate::esp::syntax();
    let baud = plan.baud.to_string();
    command.args([
        "--port",
        port_path,
        "--baud",
        &baud,
        // Naming the chip is what turns a mismatched board model into a
        // refusal with a clear message instead of a half-written flash.
        "--chip",
        plan.chip.esptool_name(),
        "--before",
        &syntax.spell("default_reset"),
        "--after",
        &syntax.spell("hard_reset"),
        &syntax.spell("write_flash"),
        "-z",
    ]);
    if plan.erase_first {
        // The image replaces the bootloader, the partition table and the
        // application, but not a filesystem the previous firmware left
        // behind — and a stale filesystem is the usual reason a freshly
        // flashed board does not come back.
        command.arg("--erase-all");
    }
    command.args([plan.offset, firmware.to_string_lossy().as_ref()]);

    match run_streaming(&mut command, emit) {
        Ok(outcome) if outcome.status.success() => Ok(()),
        Ok(outcome) => Err(crate::esp::diagnose(&outcome.tail, Some(plan.chip))),
        Err(error) => Err(crate::esp::Diagnosis {
            kind: crate::esp::FailureKind::Other,
            what: error,
            next: None,
        }),
    }
}

/// Copy an image onto a bootloader volume the board mounts as a disk.
fn copy_to_boot_volume(
    volume: Option<&'static str>,
    marker: &'static str,
    firmware: &Path,
    emit: &mut dyn FnMut(DeviceEvent),
) -> FlashOutcome {
    let wanted = volume.unwrap_or("bootloader");
    progress!(emit, "Looking for the {wanted} volume…");

    // The board reboots into its bootloader as the user watches, so give the
    // volume a few seconds to appear rather than failing on the first look.
    let mut found = Vec::new();
    for _ in 0..20 {
        found = boot_volumes(volume, marker);
        if !found.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    match found.len() {
        0 => FlashOutcome::failed(format!("No {wanted} volume is mounted.")).with_hint(
            FlashMethod::MassStorage { volume, marker }
                .preparation()
                .unwrap_or("Put the board into its bootloader and try again."),
        ),
        1 => {
            let dest = &found[0];
            let name = firmware.file_name().unwrap_or_default().to_string_lossy();
            match std::fs::copy(firmware, dest.join(&*name)) {
                Ok(_) => {
                    progress!(emit, "[OK] Copied {name} to {}", dest.display());
                    progress!(
                        emit,
                        "[OK] The board restarts on its own once the copy lands"
                    );
                    FlashOutcome::ok()
                }
                Err(error) => FlashOutcome::failed(error.to_string()),
            }
        }
        count => FlashOutcome::failed(format!("{count} bootloader volumes are mounted."))
            .with_hint("Disconnect the other boards, leaving only the one you are flashing."),
    }
}

/// Mounted bootloader volumes: those carrying `marker`, optionally narrowed
/// to a volume-name prefix.
pub fn boot_volumes(volume: Option<&str>, marker: &str) -> Vec<PathBuf> {
    let mut mounts: Vec<PathBuf> = volume_roots()
        .into_iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flatten()
        .flatten()
        .filter(|entry| match volume {
            Some(prefix) => entry.file_name().to_string_lossy().starts_with(prefix),
            None => true,
        })
        .map(|entry| entry.path())
        .filter(|path| path.join(marker).exists())
        .collect();
    mounts.sort();
    mounts.dedup();
    mounts
}

fn write_with_dfu_util(firmware: &Path, emit: &mut dyn FnMut(DeviceEvent)) -> FlashOutcome {
    let Some(mut command) = toolchain::command_for(Tool::DfuUtil) else {
        return FlashOutcome::failed(missing_tool(Tool::DfuUtil));
    };
    progress!(emit, "Writing {} with dfu-util…", firmware.display());
    command.args([
        "-a",
        "0",
        "-s",
        "0x08000000:leave",
        "-D",
        firmware.to_string_lossy().as_ref(),
    ]);
    match run_streaming(&mut command, emit) {
        Ok(outcome) if outcome.status.success() => {
            progress!(emit, "[OK] Firmware written");
            FlashOutcome::ok()
        }
        Ok(outcome) => FlashOutcome::failed(outcome.tail.trim().to_string())
            .with_hint("Put the board in DFU mode (BOOT0 high, then reset) and try again."),
        Err(error) => FlashOutcome::failed(error),
    }
}

fn write_with_bossac(
    port_path: &str,
    firmware: &Path,
    emit: &mut dyn FnMut(DeviceEvent),
) -> FlashOutcome {
    let Some(mut command) = toolchain::command_for(Tool::Bossac) else {
        return FlashOutcome::failed(missing_tool(Tool::Bossac));
    };
    // bossac wants the device name, not the full path.
    let device = port_path.rsplit('/').next().unwrap_or(port_path);
    progress!(emit, "Writing {} with bossac…", firmware.display());
    command.args([
        "-i",
        "-d",
        &format!("--port={device}"),
        "-U",
        "-e",
        "-w",
        "-v",
        firmware.to_string_lossy().as_ref(),
        "-R",
    ]);
    match run_streaming(&mut command, emit) {
        Ok(outcome) if outcome.status.success() => {
            progress!(emit, "[OK] Firmware written");
            FlashOutcome::ok()
        }
        Ok(outcome) => FlashOutcome::failed(outcome.tail.trim().to_string())
            .with_hint("Double-tap RESET to start the bootloader, then try again."),
        Err(error) => FlashOutcome::failed(error),
    }
}

fn write_with_teensy_loader(
    chip: Chip,
    firmware: &Path,
    emit: &mut dyn FnMut(DeviceEvent),
) -> FlashOutcome {
    let Some(mut command) = toolchain::command_for(Tool::TeensyLoader) else {
        return FlashOutcome::failed(missing_tool(Tool::TeensyLoader));
    };
    let mcu = match chip {
        Chip::Imxrt1062 => "TEENSY40",
        _ => "TEENSY31",
    };
    progress!(
        emit,
        "Writing {} with teensy_loader_cli…",
        firmware.display()
    );
    command.args([
        &format!("--mcu={mcu}"),
        "-w",
        "-v",
        firmware.to_string_lossy().as_ref(),
    ]);
    match run_streaming(&mut command, emit) {
        Ok(outcome) if outcome.status.success() => {
            progress!(emit, "[OK] Firmware written");
            FlashOutcome::ok()
        }
        Ok(outcome) => FlashOutcome::failed(outcome.tail.trim().to_string())
            .with_hint("Press the button on the Teensy to put it in programming mode."),
        Err(error) => FlashOutcome::failed(error),
    }
}

// ---------------------------------------------------------------------------
// After the write
// ---------------------------------------------------------------------------

/// How long to wait for a board to come back after a reset. A chip on its own
/// USB peripheral drops off the bus while it reboots and re-enumerates, which
/// takes a couple of seconds; behind a bridge the port never went away.
const REAPPEAR_TIMEOUT: Duration = Duration::from_secs(20);

/// Wait for the board to come back and say whether the new firmware is
/// actually running.
///
/// "Firmware written" is the flasher's result, not the user's. The question
/// they asked was whether the board works now, and the honest answer needs the
/// board to answer for itself.
pub fn settle_after_flash(
    port_path: &str,
    runtime: Runtime,
    emit: &mut dyn FnMut(DeviceEvent),
) -> bool {
    progress!(emit, "Waiting for the board to restart…");
    if !wait_for_port(port_path, REAPPEAR_TIMEOUT) {
        progress!(
            emit,
            "The board has not come back on {port_path} yet. If it stays away, unplug it and \
             connect it again."
        );
        return false;
    }
    if runtime != Runtime::MicroPython {
        progress!(emit, "[OK] {port_path} is back");
        return true;
    }
    if toolchain::locate(Tool::Mpremote) == toolchain::Location::Missing {
        progress!(
            emit,
            "[OK] {port_path} is back. Install mpremote to have Anode confirm the firmware booted."
        );
        return true;
    }
    match running_firmware(port_path) {
        Some(description) => {
            progress!(emit, "[OK] {description} is running on {port_path}");
            true
        }
        None => {
            progress!(
                emit,
                "{port_path} is back, but it did not answer a REPL prompt. Tap RESET, then open \
                 the REPL to check."
            );
            false
        }
    }
}

/// Wait for a serial device node to exist again, polling rather than assuming.
fn wait_for_port(port_path: &str, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if Path::new(port_path).exists() {
            // The node appears slightly before the CDC endpoint is ready to
            // talk, so give the board the moment it needs.
            std::thread::sleep(Duration::from_millis(600));
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Ask the board which firmware it is running, in its own words.
pub fn running_firmware(port_path: &str) -> Option<String> {
    // Two tries: the first REPL attempt often lands while the board is still
    // finishing its boot.
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(1200));
        }
        let result = run_mpremote(
            port_path,
            &[
                "exec",
                "import sys; v = sys.implementation.version; \
                 print(sys.implementation.name, '.'.join(str(part) for part in v[:3]))",
            ],
        );
        if !result.success {
            continue;
        }
        let answer = result.stdout.trim();
        let (name, version) = answer.split_once(char::is_whitespace)?;
        let label = match name {
            "micropython" => "MicroPython",
            "circuitpython" => "CircuitPython",
            other => other,
        };
        return Some(format!("{label} {version}"));
    }
    None
}

// ---------------------------------------------------------------------------
// Firmware downloads
// ---------------------------------------------------------------------------

fn temporary_transfer_file(ext: &str) -> Result<tempfile::TempPath, String> {
    tempfile::Builder::new()
        .prefix("anode-transfer-")
        .suffix(ext)
        .tempfile()
        .map(|file| file.into_temp_path())
        .map_err(|error| error.to_string())
}

/// Minimal HTTP(S) GET following redirects. `curl` ships with macOS, nearly
/// every Linux image and Windows 10+; shelling out to it keeps a full HTTP
/// client out of the dependency tree for the one thing that needs the network.
fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let curl = toolchain::locate_executable("curl")
        .ok_or_else(|| "curl is not installed, so firmware cannot be downloaded".to_string())?;
    let output = Command::new(curl)
        .args(["-fsSL", "--connect-timeout", "15", "--max-time", "300", url])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Could not download {url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// Pull every `href`-like value out of a page. Firmware index pages are plain
/// enough that splitting on quotes finds the links without a HTML parser.
fn page_links(body: &str) -> Vec<&str> {
    body.split(['"', '\''])
        .filter(|candidate| candidate.contains("/firmware/") || candidate.contains("/bin/"))
        .collect()
}

/// Find the firmware link on a micropython.org download page.
///
/// One page lists every build in a variant family, so `ESP32_GENERIC_S3` and
/// `ESP32_GENERIC_S3-SPIRAM_OCT` both appear on the `ESP32_GENERIC_S3` page.
/// Matching the file name against the exact variant is what keeps a PSRAM
/// board from being handed the plain build, or the other way round.
fn micropython_url(variant: &str, extension: &str) -> Result<String, String> {
    let page = crate::esp::micropython_download_page(variant);
    let body = String::from_utf8_lossy(&http_get(&format!(
        "https://micropython.org/download/{page}/"
    ))?)
    .to_string();
    let links: Vec<&str> = page_links(&body)
        .into_iter()
        .filter(|link| link.starts_with("/resources/firmware/"))
        .collect();
    match pick_micropython_link(&links, variant, extension) {
        Some(link) => Ok(format!("https://micropython.org{link}")),
        None => Err(format!(
            "micropython.org lists no {extension} build for {variant}. Pick another board model, \
             or flash an image from disk."
        )),
    }
}

/// Choose the newest full image for exactly `variant`. The page is ordered
/// newest first, so the first match is the current release.
fn pick_micropython_link<'a>(links: &[&'a str], variant: &str, extension: &str) -> Option<&'a str> {
    let prefix = format!("{variant}-");
    links
        .iter()
        .copied()
        // `.app-bin` is the OTA image, not the full flash image.
        .filter(|link| link.ends_with(extension) && !link.ends_with(".app-bin"))
        .find(|link| {
            link.rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with(&prefix))
        })
}

/// Find the firmware link on a circuitpython.org board page.
fn circuitpython_url(slug: &str, extension: &str) -> Result<String, String> {
    let page = format!("https://circuitpython.org/board/{slug}/");
    let body = String::from_utf8_lossy(&http_get(&page)?).to_string();
    let links: Vec<&str> = page_links(&body)
        .into_iter()
        .filter(|link| link.starts_with("https://downloads.circuitpython.org/bin/"))
        .collect();
    // Prefer the English build in the board's own image format.
    let pick = links
        .iter()
        .find(|link| link.ends_with(extension) && link.contains("en_US"))
        .or_else(|| links.iter().find(|link| link.ends_with(extension)));
    match pick {
        Some(link) => Ok((*link).to_string()),
        None => Err(format!(
            "circuitpython.org lists no {extension} build for {slug}. Download the image yourself, \
             then flash it from disk."
        )),
    }
}

fn download(
    url: &str,
    extension: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<tempfile::TempPath, String> {
    progress!(emit, "Downloading {url}");
    let bytes = http_get(url)?;
    let dest = temporary_transfer_file(extension)?;
    std::fs::write(&dest, &bytes).map_err(|error| error.to_string())?;
    progress!(emit, "Downloaded {} KB", bytes.len() / 1024);
    Ok(dest)
}

fn download_micropython(
    variant: &str,
    extension: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<tempfile::TempPath, String> {
    progress!(
        emit,
        "Looking up the latest MicroPython build for {variant}…"
    );
    let url = micropython_url(variant, extension)?;
    download(&url, extension, emit)
}

fn download_circuitpython(
    slug: &str,
    extension: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<tempfile::TempPath, String> {
    progress!(
        emit,
        "Looking up the latest CircuitPython build for {slug}…"
    );
    let url = circuitpython_url(slug, extension)?;
    download(&url, extension, emit)
}

// ---------------------------------------------------------------------------
// Process streaming
// ---------------------------------------------------------------------------

/// Drain both pipes concurrently. esptool uses carriage returns for progress;
/// bounded chunks also keep unbroken output from accumulating indefinitely.
fn forward_process_lines(
    reader: impl Read,
    sender: std::sync::mpsc::Sender<String>,
) -> std::io::Result<()> {
    let mut line = Vec::new();
    for byte in BufReader::new(reader).bytes() {
        let byte = byte?;
        if byte == b'\n' || byte == b'\r' || line.len() >= 8192 {
            if !line.is_empty() {
                let cleaned = strip_ansi(&String::from_utf8_lossy(&line));
                if !cleaned.trim().is_empty() {
                    let _ = sender.send(cleaned);
                }
                line.clear();
            }
            if byte == b'\n' || byte == b'\r' {
                continue;
            }
        }
        line.push(byte);
    }
    if !line.is_empty() {
        let _ = sender.send(strip_ansi(&String::from_utf8_lossy(&line)));
    }
    Ok(())
}

/// Run a command, forwarding each output line to `emit` as it arrives, and
/// keeping the tail for an error message.
pub fn run_streaming(
    command: &mut Command,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<CommandOutcome, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut tail = String::new();
    let read_result = std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let errors = sender.clone();
        let out = scope.spawn(move || forward_process_lines(stdout, sender));
        let err = scope.spawn(move || forward_process_lines(stderr, errors));
        for line in receiver {
            tail.push_str(&line);
            tail.push('\n');
            // Preserve a useful tail without retaining the entire process log.
            if tail.len() > 65536 {
                let mut start = tail.len() - 49152;
                while !tail.is_char_boundary(start) {
                    start += 1;
                }
                tail.drain(..start);
            }
            emit(DeviceEvent::Progress(line));
        }
        let out = out
            .join()
            .map_err(|_| "Output reader stopped".to_string())?;
        let err = err.join().map_err(|_| "Error reader stopped".to_string())?;
        out.and(err).map_err(|error| error.to_string())
    });
    let status = child.wait().map_err(|error| error.to_string())?;
    read_result?;
    Ok(CommandOutcome { status, tail })
}

// ---------------------------------------------------------------------------
// Arduino: compile and upload through arduino-cli
// ---------------------------------------------------------------------------

/// Add a third-party package index to `arduino-cli`, so its core can be
/// installed. Indexes already present are left alone.
fn add_board_index(index_url: &str, emit: &mut dyn FnMut(DeviceEvent)) -> Result<(), String> {
    let Some(mut command) = toolchain::command_for(Tool::ArduinoCli) else {
        return Err(missing_tool(Tool::ArduinoCli));
    };
    progress!(emit, "Registering board index {index_url}");
    command.args(["config", "add", "board_manager.additional_urls", index_url]);
    // A duplicate entry is reported as an error; the outcome we want is the
    // index being present either way, so only a failed update is fatal.
    let _ = run_streaming(&mut command, emit);

    let Some(mut update) = toolchain::command_for(Tool::ArduinoCli) else {
        return Err(missing_tool(Tool::ArduinoCli));
    };
    update.args(["core", "update-index"]);
    run_streaming(&mut update, emit)?.into_result("Could not refresh the board index")
}

/// Install the `arduino-cli` core a board's FQBN comes from.
pub fn arduino_core_install(
    board: &BoardDef,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let Some(core) = board.core else {
        return Err(format!("{} has no Arduino core in Anode.", board.name));
    };
    if let Some(index_url) = core.index_url {
        add_board_index(index_url, emit)?;
    }
    let Some(mut command) = toolchain::command_for(Tool::ArduinoCli) else {
        return Err(missing_tool(Tool::ArduinoCli));
    };
    progress!(emit, "Installing the {} core…", core.id);
    command.args(["core", "install", core.id]);
    run_streaming(&mut command, emit)?
        .into_result(&format!("Could not install the {} core", core.id))?;
    progress!(emit, "[OK] {} installed", core.id);
    Ok(())
}

pub fn arduino_compile(
    sketch_path: &Path,
    fqbn: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let Some(mut command) = toolchain::command_for(Tool::ArduinoCli) else {
        return Err(missing_tool(Tool::ArduinoCli));
    };
    progress!(emit, "Compiling {} for {fqbn}…", sketch_path.display());
    command.args(["compile", "--fqbn", fqbn]).arg(sketch_path);
    run_streaming(&mut command, emit)?.into_result("Compile failed")?;
    progress!(emit, "[OK] Compile finished");
    Ok(())
}

pub fn arduino_upload(
    port_path: &str,
    sketch_path: &Path,
    fqbn: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let Some(mut command) = toolchain::command_for(Tool::ArduinoCli) else {
        return Err(missing_tool(Tool::ArduinoCli));
    };
    progress!(emit, "Uploading to {port_path} for {fqbn}…");
    command
        .args(["upload", "-p", port_path, "--fqbn", fqbn])
        .arg(sketch_path);
    run_streaming(&mut command, emit)?.into_result("Upload failed")?;
    progress!(emit, "[OK] Upload finished");
    Ok(())
}

// ---------------------------------------------------------------------------
// PlatformIO
// ---------------------------------------------------------------------------

fn platformio(project: &Path) -> Result<Command, String> {
    let mut command =
        toolchain::command_for(Tool::PlatformIo).ok_or_else(|| missing_tool(Tool::PlatformIo))?;
    command.current_dir(project);
    Ok(command)
}

/// Build a PlatformIO project. `environment` selects one `[env:…]` section;
/// `None` builds the default environments.
pub fn platformio_build(
    project: &Path,
    environment: Option<&str>,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let mut command = platformio(project)?;
    command.arg("run");
    if let Some(environment) = environment {
        command.args(["-e", environment]);
    }
    progress!(emit, "Building {} with PlatformIO…", project.display());
    run_streaming(&mut command, emit)?.into_result("PlatformIO build failed")?;
    progress!(emit, "[OK] Build finished");
    Ok(())
}

/// Build and upload a PlatformIO project to `port_path`.
pub fn platformio_upload(
    project: &Path,
    environment: Option<&str>,
    port_path: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let mut command = platformio(project)?;
    command.args(["run", "-t", "upload", "--upload-port", port_path]);
    if let Some(environment) = environment {
        command.args(["-e", environment]);
    }
    progress!(emit, "Uploading {} to {port_path}…", project.display());
    run_streaming(&mut command, emit)?.into_result("PlatformIO upload failed")?;
    progress!(emit, "[OK] Upload finished");
    Ok(())
}

/// The `[env:…]` names declared in a `platformio.ini`.
pub fn platformio_environments(project: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(project.join("platformio.ini")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("[env:")?
                .strip_suffix(']')
                .map(str::to_string)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ESP-IDF
// ---------------------------------------------------------------------------

fn idf(project: &Path) -> Result<Command, String> {
    let mut command =
        toolchain::command_for(Tool::EspIdf).ok_or_else(|| missing_tool(Tool::EspIdf))?;
    command.current_dir(project);
    Ok(command)
}

/// Build an ESP-IDF project for the board's chip.
pub fn idf_build(
    project: &Path,
    target: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    // `set-target` reconfigures the build directory; it is a no-op when the
    // target already matches, so running it first keeps the build honest
    // after the user switches boards.
    let mut set_target = idf(project)?;
    set_target.args(["set-target", target]);
    progress!(emit, "Setting the ESP-IDF target to {target}…");
    run_streaming(&mut set_target, emit)?.into_result("Could not set the ESP-IDF target")?;

    let mut command = idf(project)?;
    command.arg("build");
    progress!(emit, "Building {} with ESP-IDF…", project.display());
    run_streaming(&mut command, emit)?.into_result("ESP-IDF build failed")?;
    progress!(emit, "[OK] Build finished");
    Ok(())
}

/// Build and flash an ESP-IDF project over `port_path`.
pub fn idf_flash(
    project: &Path,
    target: &str,
    port_path: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    idf_build(project, target, emit)?;
    let mut command = idf(project)?;
    command.args(["-p", port_path, "flash"]);
    progress!(emit, "Flashing {} to {port_path}…", project.display());
    run_streaming(&mut command, emit)?.into_result("ESP-IDF flash failed")?;
    progress!(emit, "[OK] Flash finished");
    Ok(())
}

// ---------------------------------------------------------------------------
// Device filesystem (mpremote fs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
}

pub fn device_fs_ls(port_path: &str, remote_path: &str) -> Result<Vec<DeviceFileEntry>, String> {
    let result = run_mpremote(port_path, &["fs", "ls", remote_path]);
    if !result.success {
        return Err(result.error());
    }
    let mut entries = Vec::new();
    for line in result.stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Listing") || trimmed.contains("files)") {
            continue;
        }
        let is_directory = trimmed.ends_with('/');
        let name = trimmed
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        entries.push(DeviceFileEntry {
            name,
            path: trimmed.to_string(),
            is_directory,
            size: 0,
        });
    }
    Ok(entries)
}

pub fn device_fs_cat(port_path: &str, remote_path: &str) -> Result<String, String> {
    let result = run_mpremote(port_path, &["fs", "cat", remote_path]);
    if result.success {
        Ok(result.stdout)
    } else {
        Err(result.error())
    }
}

pub fn device_fs_put(port_path: &str, local_path: &Path, remote_path: &str) -> Result<(), String> {
    let result = run_mpremote(
        port_path,
        &[
            "fs",
            "cp",
            local_path.to_string_lossy().as_ref(),
            remote_path,
        ],
    );
    if result.success {
        Ok(())
    } else {
        Err(result.error())
    }
}

pub fn device_fs_rm(port_path: &str, remote_path: &str) -> Result<(), String> {
    let result = run_mpremote(port_path, &["fs", "rm", remote_path]);
    if result.success {
        Ok(())
    } else {
        Err(result.error())
    }
}

/// The runtime a board is actually running right now, as far as a quick probe
/// can tell. Used to warn before overwriting a different runtime's firmware.
pub fn probe_runtime(port_path: &str) -> Option<Runtime> {
    let result = run_mpremote(
        port_path,
        &["exec", "import sys; print(sys.implementation.name)"],
    );
    if !result.success {
        return None;
    }
    match result.stdout.trim() {
        "micropython" => Some(Runtime::MicroPython),
        "circuitpython" => Some(Runtime::CircuitPython),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_transfers_have_independent_files_and_cleanup() {
        let first = temporary_transfer_file(".bin").unwrap();
        let second = temporary_transfer_file(".bin").unwrap();
        assert_ne!(first.to_path_buf(), second.to_path_buf());
        std::fs::write(&first, b"first board").unwrap();
        std::fs::write(&second, b"second board").unwrap();
        let first_path = first.to_path_buf();
        drop(first);
        assert!(!first_path.exists());
        assert_eq!(std::fs::read(&second).unwrap(), b"second board");
    }

    #[test]
    fn streaming_splits_progress_and_preserves_unterminated_errors() {
        let (sender, receiver) = std::sync::mpsc::channel();
        forward_process_lines(
            &b"\x1b[32mWriting 10%\x1b[0m\rWriting 100%\r\nerror without newline"[..],
            sender,
        )
        .unwrap();
        assert_eq!(
            receiver.into_iter().collect::<Vec<_>>(),
            vec!["Writing 10%", "Writing 100%", "error without newline"]
        );
    }

    #[test]
    fn process_output_drains_both_pipes_and_returns_failure() {
        let mut command = Command::new("python3");
        command.args(["-u", "-c", "import sys; print('progress', flush=True); sys.stderr.write('x' * 131072 + '\\nlast error'); sys.exit(3)"]);
        let mut messages = Vec::new();
        let outcome = run_streaming(&mut command, &mut |DeviceEvent::Progress(line)| {
            messages.push(line)
        })
        .unwrap();
        assert_eq!(outcome.status.code(), Some(3));
        assert!(messages.iter().any(|line| line == "progress"));
        assert!(outcome.tail.ends_with("last error\n"));
        assert!(outcome.tail.len() <= 65536);
    }

    /// esptool reports a long write by rewriting one line with carriage
    /// returns and no newline until the end. If those only reached the UI
    /// when the process exited, a three-minute flash would look frozen.
    #[test]
    fn esptool_style_progress_arrives_while_the_write_is_still_running() {
        let mut command = Command::new("python3");
        command.args([
            "-u",
            "-c",
            concat!(
                "import sys, time\n",
                "for pct in (12, 47, 100):\n",
                "    sys.stdout.write('Writing at 0x0001%04x... (%d %%)\\r' % (pct, pct))\n",
                "    sys.stdout.flush()\n",
                "    time.sleep(0.12)\n",
                "sys.stdout.write('\\nHash of data verified.\\n')\n",
            ),
        ]);

        let started = std::time::Instant::now();
        let mut seen: Vec<(Duration, String)> = Vec::new();
        let outcome = run_streaming(&mut command, &mut |DeviceEvent::Progress(line)| {
            seen.push((started.elapsed(), line))
        })
        .unwrap();

        assert!(outcome.status.success());
        let lines: Vec<&str> = seen.iter().map(|(_, line)| line.as_str()).collect();
        assert_eq!(
            lines,
            vec![
                "Writing at 0x0001000c... (12 %)",
                "Writing at 0x0001002f... (47 %)",
                "Writing at 0x00010064... (100 %)",
                "Hash of data verified.",
            ],
            "each carriage-return update is its own progress line"
        );
        assert!(
            seen[0].0 + Duration::from_millis(100) < seen[2].0,
            "progress was buffered instead of streamed: {:?}",
            seen.iter().map(|(at, _)| *at).collect::<Vec<_>>()
        );
    }

    #[test]
    fn strip_ansi_removes_color_codes() {
        assert_eq!(strip_ansi("\x1b[1;31merror\x1b[0m"), "error");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn parses_boot_out() {
        let text = "ADAFRUIT Feather ESP32-S3\nUID: 1234\nAdafruit CircuitPython 9.1.0 on 2024-01-01; Board ID:feather_esp32s3";
        let (board, version) = parse_boot_out(text);
        assert_eq!(board.as_deref(), Some("Feather ESP32-S3"));
        assert_eq!(version.as_deref(), Some("9.1.0"));
    }

    #[test]
    fn micropython_link_picking_prefers_the_full_image() {
        let body = r#"<a href="/resources/firmware/ESP32_GENERIC_S3-20240101-v1.22.0.app-bin">ota</a>
                      <a href="/resources/firmware/ESP32_GENERIC_S3-20240101-v1.22.0.bin">bin</a>"#;
        let links: Vec<&str> = page_links(body)
            .into_iter()
            .filter(|link| link.starts_with("/resources/firmware/"))
            .collect();
        let pick = links
            .iter()
            .find(|link| link.ends_with(".bin") && !link.ends_with(".app-bin"));
        assert_eq!(
            *pick.unwrap(),
            "/resources/firmware/ESP32_GENERIC_S3-20240101-v1.22.0.bin"
        );
    }

    #[test]
    fn circuitpython_link_picking_prefers_english() {
        let body = r#"<a href="https://downloads.circuitpython.org/bin/x/de_DE/a-de_DE-9.0.0.uf2">de</a>
                      <a href="https://downloads.circuitpython.org/bin/x/en_US/a-en_US-9.0.0.uf2">en</a>"#;
        let links: Vec<&str> = page_links(body)
            .into_iter()
            .filter(|link| link.starts_with("https://downloads.circuitpython.org/bin/"))
            .collect();
        let pick = links
            .iter()
            .find(|link| link.ends_with(".uf2") && link.contains("en_US"));
        assert!(pick.unwrap().contains("en_US"));
    }

    #[test]
    fn firmware_extension_follows_the_flash_method() {
        assert_eq!(FlashMethod::Esptool.firmware_extension(), ".bin");
        assert_eq!(
            FlashMethod::MassStorage {
                volume: Some("RPI-RP2"),
                marker: crate::boards::UF2_MARKER
            }
            .firmware_extension(),
            ".uf2"
        );
        assert_eq!(
            FlashMethod::MassStorage {
                volume: Some("MICROBIT"),
                marker: crate::boards::DAPLINK_MARKER
            }
            .firmware_extension(),
            ".hex"
        );
    }

    #[test]
    fn boot_volumes_tolerate_an_absent_mount_root() {
        // No board is attached in CI; the probe must come back empty rather
        // than failing.
        let _ = boot_volumes(Some("RPI-RP2"), crate::boards::UF2_MARKER);
    }

    #[test]
    fn platformio_environments_are_read_from_the_ini() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("platformio.ini"),
            "[platformio]\ndefault_envs = esp32\n\n[env:esp32]\nboard = esp32dev\n\n[env:pico]\nboard = pico\n",
        )
        .unwrap();
        assert_eq!(
            platformio_environments(dir.path()),
            vec!["esp32".to_string(), "pico".to_string()]
        );
    }

    #[test]
    fn a_failed_outcome_joins_the_error_and_its_tail() {
        let outcome = FlashOutcome::failed("Could not connect").with_hint("Hold BOOT.");
        assert_eq!(outcome.message(), "Could not connect\nHold BOOT.");
    }
}
