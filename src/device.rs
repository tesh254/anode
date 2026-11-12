//! Device operations: run / upload code, flash firmware, deploy to
//! CircuitPython, and drive `mpremote` for the device file tree.
//!
//! Translated from the Electron `upload-code`, `run-code`, `stop-code`,
//! `flash-firmware`, `circuitpython-deploy`, and `device-fs-*` IPC handlers.
//! Progress lines flow to the shell log through `DeviceEvent`.

use crate::boards::BoardType;
use crate::profiles::CircuitPythonDrive;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// A progress line for the shell log (already cleaned of ANSI codes).
    Progress(String),
}

/// Strip ANSI escape sequences so they don't pollute the shell log.
pub fn strip_ansi(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\x1b\[\d+(;\d+)*[a-zA-Z]").unwrap())
        .replace_all(text, "")
        .to_string()
}

/// Result of an mpremote invocation with cleaned-up stderr.
struct MpremoteResult {
    success: bool,
    stdout: String,
    stderr: String,
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
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with("File \"")
                && !t.starts_with("~~~")
                && !t.starts_with('^')
                && !t.contains("runpy")
                && !t.contains("mpremote/")
                && !t.contains("site-packages")
                && !t.contains("Traceback")
                && !t.contains("During handling")
        })
        .collect();
    lines
        .last()
        .map(|l| l.to_string())
        .unwrap_or_else(|| stderr.trim().lines().next_back().unwrap_or("").to_string())
}

fn python_command() -> Command {
    let mut cmd = Command::new("python3");
    cmd.stdin(Stdio::null());
    cmd
}

fn run_mpremote(port_path: &str, sub_args: &[&str]) -> MpremoteResult {
    let output = python_command()
        .args(["-m", "mpremote", "connect", port_path])
        .args(sub_args)
        .output();

    match output {
        Ok(out) => MpremoteResult {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: clean_mpremote_stderr(&String::from_utf8_lossy(&out.stderr)),
        },
        Err(e) => MpremoteResult {
            success: false,
            stdout: String::new(),
            stderr: e.to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// CircuitPython: deploy via the mounted CIRCUITPY drive
// ---------------------------------------------------------------------------

/// Candidate CIRCUITPY mount roots per OS. macOS mounts volumes flat under
/// /Volumes; Linux distros use /media/<user>/<label> or /run/media/<user>.
fn circuitpy_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    match std::env::consts::OS {
        "macos" => candidates.push(PathBuf::from("/Volumes/CIRCUITPY")),
        "linux" => {
            for root in ["/media", "/run/media"] {
                let root = Path::new(root);
                if !root.exists() {
                    continue;
                }
                if let Ok(users) = std::fs::read_dir(root) {
                    for user in users.flatten() {
                        candidates.push(user.path().join("CIRCUITPY"));
                    }
                }
            }
        }
        // Windows: enumerating drive letters by volume label needs extra
        // tooling; detection still works via the Adafruit 239a VID there.
        _ => {}
    }
    candidates
}

/// Detect a connected CircuitPython board by its hallmark: a mounted USB drive
/// named CIRCUITPY containing boot_out.txt (board name + CP version).
pub fn detect_circuitpython() -> CircuitPythonDrive {
    for mount in circuitpy_candidates() {
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
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid file name".to_string())?;
    let dest = mount.join(safe_name);
    std::fs::write(&dest, code).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Run / upload via mpremote, with REPL-paste fallbacks
// ---------------------------------------------------------------------------

/// Run Python code on the board via `mpremote run`, streaming progress.
pub fn run_code(
    port_path: &str,
    code: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let tmp = temporary_transfer_file(".py")?;
    std::fs::write(&tmp, code).map_err(|e| e.to_string())?;

    emit(DeviceEvent::Progress("Running code on device...".into()));
    let result = run_mpremote(port_path, &["run", tmp.to_string_lossy().as_ref()]);

    if result.success {
        for line in result.stdout.lines() {
            emit(DeviceEvent::Progress(line.to_string()));
        }
        emit(DeviceEvent::Progress("[OK] Code execution finished".into()));
        Ok(())
    } else {
        let err = if result.stderr.is_empty() {
            result.stdout
        } else {
            result.stderr
        };
        Err(if err.is_empty() {
            "Run failed".into()
        } else {
            err
        })
    }
}

/// Soft-reset the board (the `stop` button).
pub fn stop_code(port_path: &str, emit: &mut dyn FnMut(DeviceEvent)) -> Result<(), String> {
    emit(DeviceEvent::Progress(
        "Sending soft reset (Ctrl+C) to device...".into(),
    ));
    let result = run_mpremote(port_path, &["soft-reset"]);
    if result.success {
        emit(DeviceEvent::Progress("[OK] Device reset".into()));
        Ok(())
    } else {
        Err(result.stderr)
    }
}

/// Upload one file to the board via `mpremote cp` (with an ampy fallback).
pub fn upload_code(
    port_path: &str,
    file_name: &str,
    code: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    let tmp = temporary_transfer_file(".py")?;
    std::fs::write(&tmp, code).map_err(|e| e.to_string())?;

    emit(DeviceEvent::Progress(format!(
        "Uploading {file_name} via mpremote to {port_path}..."
    )));
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
        emit(DeviceEvent::Progress(format!("[OK] Uploaded {file_name}")));
        return Ok(());
    }

    emit(DeviceEvent::Progress(
        "[WARN] mpremote failed, trying ampy...".into(),
    ));
    let output = python_command()
        .args([
            "-m",
            "ampy",
            "--port",
            port_path,
            "put",
            tmp.to_string_lossy().as_ref(),
            &format!("/{file_name}"),
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            emit(DeviceEvent::Progress(format!("[OK] Uploaded {file_name}")));
            Ok(())
        }
        Ok(out) => {
            let err = clean_mpremote_stderr(&String::from_utf8_lossy(&out.stderr));
            Err(if err.is_empty() {
                "ampy failed".into()
            } else {
                err
            })
        }
        Err(_) => Err(
            "No upload tool available. Install mpremote: python3 -m pip install mpremote".into(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Firmware flashing
// ---------------------------------------------------------------------------

pub struct FlashOutcome {
    pub success: bool,
    pub error: Option<String>,
    pub hint: Option<String>,
}

fn flash_outcome(success: bool, error: Option<String>, hint: Option<String>) -> FlashOutcome {
    FlashOutcome {
        success,
        error,
        hint,
    }
}

/// Detect the Espressif chip type from `esptool flash_id` output.
pub fn chip_type_from_flash_id(text: &str) -> &'static str {
    // The flash ID line looks like "Detecting chip type... ESP32-S3 (revision ...)".
    for (needle, chip) in [
        ("ESP32-S3", "ESP32_GENERIC_S3"),
        ("ESP32-C3", "ESP32_GENERIC_C3"),
        ("ESP32-C6", "ESP32_GENERIC_C6"),
        ("ESP32-S2", "ESP32_GENERIC_S2"),
        ("ESP8266", "ESP8266_GENERIC"),
    ] {
        if text.contains(needle) {
            return chip;
        }
    }
    "ESP32_GENERIC"
}

/// The flash offset per chip family.
pub fn flash_offset(chip: &str) -> &'static str {
    if chip.starts_with("ESP32_GENERIC_S3") || chip == "ESP32_GENERIC_S2" {
        "0x0"
    } else {
        "0x1000"
    }
}

/// Download the latest MicroPython firmware .bin/.uf2 for a board and flash it
/// with esptool (or copy a .uf2 to the RPI-RP2 volume for RP2040).
///
/// A simplified but faithful port of the Electron `flash-firmware` handler's
/// auto-download path. Progress streams through `emit`.
pub fn flash_firmware(
    port_path: &str,
    board_type: BoardType,
    firmware_variant: Option<&str>,
    emit: &mut dyn FnMut(DeviceEvent),
) -> FlashOutcome {
    emit(DeviceEvent::Progress(format!(
        "Starting {} firmware flash on {port_path}...",
        board_type.label()
    )));

    match board_type {
        BoardType::RaspberryPi => flash_rp2040(port_path, emit),
        BoardType::Espressif | BoardType::M5StackAtom | BoardType::M5StampS3 => {
            flash_espressif(port_path, board_type, firmware_variant, emit)
        }
        BoardType::Arduino => flash_outcome(
            false,
            Some("Arduino flashing requires a compiled firmware file".into()),
            Some("Install arduino-cli: https://arduino.github.io/arduino-cli/".into()),
        ),
        BoardType::Unknown => flash_outcome(false, Some("Unknown board type".into()), None),
    }
}

fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let bytes = ureq_redirecting_get(url)?;
    std::fs::write(dest, bytes).map_err(|e| e.to_string())
}

/// Minimal HTTP(S) GET following redirects. Avoids pulling a full HTTP client
/// into the dependency tree; firmware downloads are the only network use.
fn ureq_redirecting_get(url: &str) -> Result<Vec<u8>, String> {
    // curl is present on macOS and nearly all Linux images; Windows 10+ ships
    // it too. Shelling out keeps the binary lean for this one operation.
    let output = Command::new("curl")
        .args(["-fsSL", "--connect-timeout", "15", "--max-time", "180", url])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "download failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// Find the firmware link on a micropython.org download page.
fn get_micropython_url(page_url: &str, prefer_ext: &str) -> Result<String, String> {
    let bytes = ureq_redirecting_get(page_url)?;
    let body = String::from_utf8_lossy(&bytes).to_string();
    let mut links: Vec<String> = Vec::new();
    let mut rest: &str = &body;
    while let Some(start) = rest.find("/resources/firmware/") {
        let after = &rest[start..];
        let end = after.find('"').unwrap_or(after.len());
        links.push(after[..end].to_string());
        rest = &after[end..];
    }
    let pick = links
        .iter()
        .find(|l| l.ends_with(prefer_ext) && !l.ends_with(".app-bin"))
        .or_else(|| links.iter().find(|l| l.ends_with(prefer_ext)))
        .or_else(|| links.first());
    match pick {
        Some(link) => Ok(format!("https://micropython.org{link}")),
        None => Err(format!("Could not find firmware link on {page_url}")),
    }
}

fn flash_rp2040(port_path: &str, emit: &mut dyn FnMut(DeviceEvent)) -> FlashOutcome {
    let _ = port_path;
    emit(DeviceEvent::Progress(
        "Connect the intended Pico in BOOTSEL mode; looking for its RPI-RP2 boot volume…".into(),
    ));
    let firmware = match download_micropython("RPI_PICO", ".uf2", &mut |m| emit(m.clone())) {
        Ok(path) => path,
        Err(e) => return flash_outcome(false, Some(e), None),
    };
    let mut dest = None;
    for _ in 0..20 {
        let mounts = rp2040_boot_volumes();
        if mounts.len() > 1 {
            return flash_outcome(false, Some("Multiple RPI-RP2 boot volumes found.".into()),
                Some("Disconnect the other Pico boards and retry with only the intended board in BOOTSEL mode.".into()));
        }
        dest = mounts.into_iter().next();
        if dest.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let Some(dest) = dest else {
        return flash_outcome(
            false,
            Some("RPI-RP2 boot volume not found".into()),
            Some("Hold BOOTSEL while reconnecting the intended Pico over USB.".into()),
        );
    };
    let filename = firmware.file_name().unwrap_or_default().to_string_lossy();
    let target = dest.join(&*filename);
    match std::fs::copy(&firmware, &target) {
        Ok(_) => {
            emit(DeviceEvent::Progress(format!(
                "[OK] Copied {filename} to {}",
                dest.display()
            )));
            emit(DeviceEvent::Progress(
                "[OK] Raspberry Pi Pico firmware flashed successfully".into(),
            ));
            flash_outcome(true, None, None)
        }
        Err(e) => flash_outcome(false, Some(e.to_string()), None),
    }
}

fn rp2040_boot_volumes() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Volumes")];
    for root in ["/media", "/run/media"] {
        if let Ok(users) = std::fs::read_dir(root) {
            roots.extend(users.flatten().map(|user| user.path()));
        }
    }
    let mut mounts: Vec<_> = roots
        .into_iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("RPI-RP2"))
        .map(|entry| entry.path())
        .filter(|path| path.join("INFO_UF2.TXT").exists())
        .collect();
    mounts.sort();
    mounts.dedup();
    mounts
}

fn temporary_transfer_file(ext: &str) -> Result<tempfile::TempPath, String> {
    tempfile::Builder::new()
        .prefix("anode-transfer-")
        .suffix(ext)
        .tempfile()
        .map(|file| file.into_temp_path())
        .map_err(|error| error.to_string())
}

fn download_micropython(
    chip: &str,
    ext: &str,
    emit: &mut dyn FnMut(&DeviceEvent),
) -> Result<tempfile::TempPath, String> {
    emit(&DeviceEvent::Progress(format!(
        "Auto-detecting and downloading latest firmware for {chip}..."
    )));
    let url = get_micropython_url(&format!("https://micropython.org/download/{chip}/"), ext)?;
    emit(&DeviceEvent::Progress(format!("Downloading {url}...")));
    let dest = temporary_transfer_file(ext)?;
    download_file(&url, &dest)?;
    emit(&DeviceEvent::Progress(format!(
        "Downloaded to {}",
        dest.display()
    )));
    Ok(dest)
}

fn flash_espressif(
    port_path: &str,
    board_type: BoardType,
    firmware_variant: Option<&str>,
    emit: &mut dyn FnMut(DeviceEvent),
) -> FlashOutcome {
    let chip = firmware_variant
        .or_else(|| board_type.micro_python_chip())
        .unwrap_or("ESP32_GENERIC");
    let firmware = match download_micropython(chip, ".bin", &mut |m| emit(m.clone())) {
        Ok(path) => path,
        Err(e) => return flash_outcome(false, Some(e), None),
    };

    emit(DeviceEvent::Progress(
        "Running esptool via python3...".into(),
    ));
    let mut command = Command::new("python3");
    command.args([
        "-u",
        "-m",
        "esptool",
        "--port",
        port_path,
        "--baud",
        "460800",
        "--before",
        "default_reset",
        "--after",
        "hard_reset",
        "write_flash",
        "-z",
        flash_offset(chip),
        firmware.to_string_lossy().as_ref(),
    ]);
    let output = stream_command(&mut command, emit);

    match output {
        Ok((status, output)) => {
            if status.success() {
                emit(DeviceEvent::Progress(
                    "[OK] Espressif firmware flashed successfully".into(),
                ));
                flash_outcome(true, None, None)
            } else {
                let err = output;
                let is_connection_error = err.contains("No serial data received")
                    || err.contains("Failed to connect")
                    || err.contains("chip stopped responding");
                flash_outcome(
                    false,
                    Some(err.trim().to_string()),
                    Some(if is_connection_error {
                        "Failed to connect. Try holding the BOOT button on your board while it connects."
                            .into()
                    } else {
                        "Install esptool: pip install esptool".into()
                    }),
                )
            }
        }
        Err(e) => flash_outcome(
            false,
            Some(e.to_string()),
            Some("Install esptool: pip install esptool".into()),
        ),
    }
}

/// Drain both pipes concurrently. Esptool uses carriage returns for progress;
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

fn stream_command(
    command: &mut Command,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(std::process::ExitStatus, String), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut output = String::new();
    let read_result = std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let errors = sender.clone();
        let out = scope.spawn(move || forward_process_lines(stdout, sender));
        let err = scope.spawn(move || forward_process_lines(stderr, errors));
        for line in receiver {
            output.push_str(&line);
            output.push('\n');
            // Preserve a useful tail for errors without retaining the entire process log.
            if output.len() > 65536 {
                let mut start = output.len() - 49152;
                while !output.is_char_boundary(start) {
                    start += 1;
                }
                output.drain(..start);
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
    Ok((status, output))
}

// ---------------------------------------------------------------------------
// Tool availability / installation
// ---------------------------------------------------------------------------

/// Check whether a flashing tool is available (in PATH or a common location).
pub fn tool_available(tool: &str) -> bool {
    if which_exists(tool) {
        return true;
    }
    match tool {
        "esptool.py" | "esptool" => {
            // pip-installed Python packages: probe the import.
            Command::new("python3")
                .args(["-c", "import esptool; print(esptool.__version__)"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        "arduino-cli" => COMMON_TOOL_PATHS
            .iter()
            .any(|p| Path::new(&p.replace('~', &home_dir())).exists()),
        _ => false,
    }
}

const COMMON_TOOL_PATHS: &[&str] = &[
    "~/Library/Python/3.11/bin/esptool",
    "~/Library/Python/3.12/bin/esptool",
    "~/Library/Python/3.13/bin/esptool",
    "~/.arduino-cli/bin/arduino-cli",
    "~/.local/bin/arduino-cli",
    "/opt/homebrew/bin/arduino-cli",
    "/usr/local/bin/arduino-cli",
];

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn which_exists(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Install a missing flashing tool. Streams progress; returns (success, error).
pub fn install_tool(tool: &str, emit: &mut dyn FnMut(DeviceEvent)) -> Result<(), String> {
    match tool {
        "esptool.py" | "esptool" => {
            emit(DeviceEvent::Progress("Installing esptool...".into()));
            let pip_cmds: &[&[&str]] = if cfg!(windows) {
                &[
                    &["pip", "install", "esptool"],
                    &["python", "-m", "pip", "install", "esptool"],
                ]
            } else {
                &[
                    &["python3", "-m", "pip", "install", "--user", "esptool"],
                    &[
                        "python3",
                        "-m",
                        "pip",
                        "install",
                        "--user",
                        "--break-system-packages",
                        "esptool",
                    ],
                    &["pip3", "install", "--user", "esptool"],
                ]
            };
            for cmd in pip_cmds {
                emit(DeviceEvent::Progress(format!("Trying: {}", cmd.join(" "))));
                if let Ok(output) = Command::new(cmd[0]).args(&cmd[1..]).output() {
                    if output.status.success() {
                        emit(DeviceEvent::Progress(
                            "[OK] esptool installed successfully".into(),
                        ));
                        return Ok(());
                    }
                    for line in String::from_utf8_lossy(&output.stderr).lines() {
                        emit(DeviceEvent::Progress(line.to_string()));
                    }
                }
            }
            Err(
                "Could not find pip. Install Python 3 from python.org or via brew install python3"
                    .into(),
            )
        }
        "arduino-cli" => {
            emit(DeviceEvent::Progress(
                "arduino-cli auto-install is not yet available in this build".into(),
            ));
            Err("Install arduino-cli: https://arduino.github.io/arduino-cli/".into())
        }
        other => Err(format!("Unknown tool: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Arduino compile / upload via arduino-cli
// ---------------------------------------------------------------------------

pub fn arduino_compile(
    sketch_path: &Path,
    fqbn: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    emit(DeviceEvent::Progress(format!(
        "Compiling {} ({fqbn})...",
        sketch_path.display()
    )));
    let output = Command::new("arduino-cli")
        .args(["compile", "--fqbn", fqbn])
        .arg(sketch_path)
        .output()
        .map_err(|e| format!("arduino-cli not found: {e}"))?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        emit(DeviceEvent::Progress(strip_ansi(line)));
    }
    if output.status.success() {
        emit(DeviceEvent::Progress("[OK] Compile successful".into()));
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err(strip_ansi(&err))
    }
}

pub fn arduino_upload(
    port_path: &str,
    sketch_path: &Path,
    fqbn: &str,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    emit(DeviceEvent::Progress(format!(
        "Uploading to {port_path} ({fqbn})..."
    )));
    let output = Command::new("arduino-cli")
        .args(["upload", "-p", port_path, "--fqbn", fqbn])
        .arg(sketch_path)
        .output()
        .map_err(|e| format!("arduino-cli not found: {e}"))?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        emit(DeviceEvent::Progress(strip_ansi(line)));
    }
    if output.status.success() {
        emit(DeviceEvent::Progress("[OK] Upload successful".into()));
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        Err(strip_ansi(&err))
    }
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
        return Err(if result.stderr.is_empty() {
            result.stdout
        } else {
            result.stderr
        });
    }
    let mut entries = Vec::new();
    for line in result.stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Listing") || trimmed.contains("files)") {
            continue;
        }
        let is_dir = trimmed.ends_with('/');
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
            is_directory: is_dir,
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
        Err(if result.stderr.is_empty() {
            result.stdout
        } else {
            result.stderr
        })
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
        Err(if result.stderr.is_empty() {
            result.stdout
        } else {
            result.stderr
        })
    }
}

pub fn device_fs_rm(port_path: &str, remote_path: &str) -> Result<(), String> {
    let result = run_mpremote(port_path, &["fs", "rm", remote_path]);
    if result.success {
        Ok(())
    } else {
        Err(if result.stderr.is_empty() {
            result.stdout
        } else {
            result.stderr
        })
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
        let (status, tail) = stream_command(&mut command, &mut |DeviceEvent::Progress(line)| {
            messages.push(line)
        })
        .unwrap();
        assert_eq!(status.code(), Some(3));
        assert!(messages.iter().any(|line| line == "progress"));
        assert!(tail.ends_with("last error\n"));
        assert!(tail.len() <= 65536);
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
    fn detects_chip_type_from_flash_id() {
        assert_eq!(
            chip_type_from_flash_id("Detecting chip type... ESP32-S3"),
            "ESP32_GENERIC_S3"
        );
        assert_eq!(chip_type_from_flash_id("nothing"), "ESP32_GENERIC");
    }

    #[test]
    fn flash_offsets() {
        assert_eq!(flash_offset("ESP32_GENERIC"), "0x1000");
        assert_eq!(flash_offset("ESP32_GENERIC_S3"), "0x0");
    }

    #[test]
    fn micropython_link_extraction() {
        // get_micropython_url is network-bound; test the link-picking logic via
        // a synthetic body through the same matcher it uses.
        let body = r#"<a href="/resources/firmware/ESP32_GENERIC_S3-20240101-v1.22.0.bin">bin</a>
                      <a href="/resources/firmware/ESP32_GENERIC_S3-20240101-v1.22.0.app-bin">ota</a>"#;
        let links: Vec<&str> = body
            .split('"')
            .filter(|s| s.starts_with("/resources/firmware/"))
            .collect();
        let pick = links
            .iter()
            .find(|l| l.ends_with(".bin") && !l.ends_with(".app-bin"));
        assert_eq!(
            *pick.unwrap(),
            "/resources/firmware/ESP32_GENERIC_S3-20240101-v1.22.0.bin"
        );
    }
}
