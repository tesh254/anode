//! External tools Anode drives, and what it takes to get them installed.
//!
//! Anode does not reimplement flashers or compilers; it drives the ones each
//! ecosystem already ships. This module names them, finds them, reports what
//! is missing for a given board, and installs the ones that can be installed
//! safely from here.
//!
//! Probing spawns processes and touches the filesystem, so every function
//! here is blocking and belongs on a background thread, never in `render`.

use crate::boards::{BoardDef, FlashMethod, Runtime};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// An external program Anode drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tool {
    Python,
    Mpremote,
    Esptool,
    ArduinoCli,
    PlatformIo,
    EspIdf,
    DfuUtil,
    Bossac,
    Picotool,
    TeensyLoader,
    Avrdude,
}

impl Tool {
    /// Every tool, in the order the toolchain list shows them.
    pub const ALL: &'static [Tool] = &[
        Tool::Python,
        Tool::Mpremote,
        Tool::Esptool,
        Tool::ArduinoCli,
        Tool::PlatformIo,
        Tool::EspIdf,
        Tool::DfuUtil,
        Tool::Bossac,
        Tool::Picotool,
        Tool::TeensyLoader,
        Tool::Avrdude,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Python => "python",
            Tool::Mpremote => "mpremote",
            Tool::Esptool => "esptool",
            Tool::ArduinoCli => "arduino-cli",
            Tool::PlatformIo => "platformio",
            Tool::EspIdf => "esp-idf",
            Tool::DfuUtil => "dfu-util",
            Tool::Bossac => "bossac",
            Tool::Picotool => "picotool",
            Tool::TeensyLoader => "teensy-loader",
            Tool::Avrdude => "avrdude",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Tool::ALL.iter().copied().find(|tool| tool.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Python => "Python 3",
            Tool::Mpremote => "mpremote",
            Tool::Esptool => "esptool",
            Tool::ArduinoCli => "Arduino CLI",
            Tool::PlatformIo => "PlatformIO Core",
            Tool::EspIdf => "ESP-IDF",
            Tool::DfuUtil => "dfu-util",
            Tool::Bossac => "BOSSA",
            Tool::Picotool => "picotool",
            Tool::TeensyLoader => "Teensy Loader",
            Tool::Avrdude => "avrdude",
        }
    }

    /// What Anode uses this tool for, in the user's terms.
    pub fn purpose(self) -> &'static str {
        match self {
            Tool::Python => "Runs mpremote and esptool",
            Tool::Mpremote => "Uploads files and runs code on MicroPython boards",
            Tool::Esptool => "Flashes firmware to Espressif chips",
            Tool::ArduinoCli => "Compiles and uploads Arduino sketches",
            Tool::PlatformIo => "Builds and uploads native C and C++ projects",
            Tool::EspIdf => "Builds and flashes Espressif native projects",
            Tool::DfuUtil => "Flashes boards in USB DFU mode",
            Tool::Bossac => "Flashes SAM-BA bootloader boards",
            Tool::Picotool => "Inspects and loads RP2040 and RP2350 boards",
            Tool::TeensyLoader => "Flashes Teensy boards",
            Tool::Avrdude => "Flashes AVR boards directly",
        }
    }

    /// The executable to look for on `PATH`.
    fn binary(self) -> &'static str {
        match self {
            Tool::Python => "python3",
            Tool::Mpremote => "mpremote",
            Tool::Esptool => "esptool",
            Tool::ArduinoCli => "arduino-cli",
            Tool::PlatformIo => "pio",
            Tool::EspIdf => "idf.py",
            Tool::DfuUtil => "dfu-util",
            Tool::Bossac => "bossac",
            Tool::Picotool => "picotool",
            Tool::TeensyLoader => "teensy_loader_cli",
            Tool::Avrdude => "avrdude",
        }
    }

    /// The Python module this tool can also be reached through.
    fn python_module(self) -> Option<&'static str> {
        match self {
            Tool::Mpremote => Some("mpremote"),
            Tool::Esptool => Some("esptool"),
            Tool::PlatformIo => Some("platformio"),
            _ => None,
        }
    }

    /// The pip distribution that provides this tool.
    fn pip_package(self) -> Option<&'static str> {
        match self {
            Tool::Mpremote => Some("mpremote"),
            Tool::Esptool => Some("esptool"),
            Tool::PlatformIo => Some("platformio"),
            _ => None,
        }
    }

    /// The Homebrew formula that provides this tool, where one exists.
    fn brew_formula(self) -> Option<&'static str> {
        match self {
            Tool::ArduinoCli => Some("arduino-cli"),
            Tool::DfuUtil => Some("dfu-util"),
            Tool::Bossac => Some("bossa"),
            Tool::Picotool => Some("picotool"),
            Tool::Avrdude => Some("avrdude"),
            Tool::TeensyLoader => Some("teensy_loader_cli"),
            _ => None,
        }
    }

    /// Whether Anode can install this tool itself.
    pub fn is_installable(self) -> bool {
        self.pip_package().is_some() || (cfg!(target_os = "macos") && self.brew_formula().is_some())
    }

    /// What to do when Anode cannot install it.
    pub fn install_hint(self) -> String {
        if let Some(package) = self.pip_package() {
            return format!("python3 -m pip install --user {package}");
        }
        if let Some(formula) = self.brew_formula() {
            return match std::env::consts::OS {
                "macos" => format!("brew install {formula}"),
                "linux" => format!("sudo apt install {formula}"),
                _ => format!("Install {} from {}", self.name(), self.docs_url()),
            };
        }
        match self {
            Tool::Python => "Install Python 3 from python.org".into(),
            Tool::EspIdf => {
                "Install ESP-IDF, then run its export script in this shell before building".into()
            }
            _ => format!("Install {} from {}", self.name(), self.docs_url()),
        }
    }

    pub fn docs_url(self) -> &'static str {
        match self {
            Tool::Python => "https://www.python.org/downloads/",
            Tool::Mpremote => "https://docs.micropython.org/en/latest/reference/mpremote.html",
            Tool::Esptool => "https://docs.espressif.com/projects/esptool/",
            Tool::ArduinoCli => "https://arduino.github.io/arduino-cli/",
            Tool::PlatformIo => "https://docs.platformio.org/en/latest/core/installation/",
            Tool::EspIdf => "https://docs.espressif.com/projects/esp-idf/en/latest/",
            Tool::DfuUtil => "https://dfu-util.sourceforge.net/",
            Tool::Bossac => "https://github.com/shumatech/BOSSA",
            Tool::Picotool => "https://github.com/raspberrypi/picotool",
            Tool::TeensyLoader => "https://www.pjrc.com/teensy/loader_cli.html",
            Tool::Avrdude => "https://github.com/avrdudes/avrdude",
        }
    }
}

// ---------------------------------------------------------------------------
// Locating tools
// ---------------------------------------------------------------------------

/// Where a tool was found, if it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// An executable resolved on `PATH` or in a well-known install directory.
    Executable(PathBuf),
    /// Reachable as `python3 -m <module>`.
    PythonModule(&'static str),
    Missing,
}

/// A tool's availability, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    pub tool: Tool,
    pub location: Location,
    /// The version string the tool reported, when it reported one.
    pub version: Option<String>,
}

impl ToolStatus {
    pub fn missing(tool: Tool) -> Self {
        Self {
            tool,
            location: Location::Missing,
            version: None,
        }
    }

    pub fn is_available(&self) -> bool {
        self.location != Location::Missing
    }

    /// The one-line detail shown under the tool's name.
    pub fn detail(&self) -> String {
        match (&self.location, &self.version) {
            (Location::Missing, _) => self.tool.install_hint(),
            (_, Some(version)) => version.clone(),
            (Location::PythonModule(module), None) => format!("python3 -m {module}"),
            (Location::Executable(path), None) => path.display().to_string(),
        }
    }
}

/// Directories to search beyond `PATH`. A GUI process started from the Dock
/// inherits a minimal `PATH` that usually omits Homebrew and pip's user
/// install directory, which is exactly where these tools land.
fn extra_search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".arduino-cli/bin"));
        dirs.push(home.join(".platformio/penv/bin"));
        // pip --user on macOS installs under a versioned Python framework dir.
        if let Ok(entries) = std::fs::read_dir(home.join("Library/Python")) {
            for entry in entries.flatten() {
                dirs.push(entry.path().join("bin"));
            }
        }
    }
    dirs
}

/// Find `binary` on `PATH` or in one of the well-known install directories.
pub fn locate_executable(binary: &str) -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    from_path
        .into_iter()
        .chain(extra_search_dirs())
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Resolve the command that runs `tool`, so callers do not have to repeat the
/// executable-or-python-module dance.
///
/// Returns `None` when the tool is not installed.
pub fn command_for(tool: Tool) -> Option<Command> {
    match locate(tool) {
        Location::Executable(path) => {
            let mut command = Command::new(path);
            command.stdin(Stdio::null());
            Some(command)
        }
        Location::PythonModule(module) => {
            let python = locate_executable("python3")?;
            let mut command = Command::new(python);
            command.args(["-u", "-m", module]).stdin(Stdio::null());
            Some(command)
        }
        Location::Missing => None,
    }
}

/// Where `tool` is, without asking it for a version.
pub fn locate(tool: Tool) -> Location {
    if let Some(path) = locate_executable(tool.binary()) {
        return Location::Executable(path);
    }
    if let Some(module) = tool.python_module() {
        if python_module_present(module) {
            return Location::PythonModule(module);
        }
    }
    // ESP-IDF exports `idf.py` through its own shell script rather than
    // installing it on PATH, so an exported environment is the other signal.
    if tool == Tool::EspIdf {
        if let Some(idf_path) = std::env::var_os("IDF_PATH") {
            let script = PathBuf::from(idf_path).join("tools/idf.py");
            if script.is_file() {
                return Location::Executable(script);
            }
        }
    }
    Location::Missing
}

fn python_module_present(module: &str) -> bool {
    let Some(python) = locate_executable("python3") else {
        return false;
    };
    Command::new(python)
        .args(["-c", &format!("import {module}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Locate `tool` and ask it for its version. Blocking: call from a background
/// thread.
pub fn probe(tool: Tool) -> ToolStatus {
    let location = locate(tool);
    if location == Location::Missing {
        return ToolStatus::missing(tool);
    }
    ToolStatus {
        tool,
        version: version_of(tool),
        location,
    }
}

/// Probe several tools. Each probe spawns a process, so this is the call that
/// belongs on a background thread.
pub fn probe_all(tools: &[Tool]) -> Vec<ToolStatus> {
    tools.iter().copied().map(probe).collect()
}

/// The arguments that make a tool print its version.
fn version_args(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Python => &["--version"],
        Tool::Mpremote | Tool::Esptool | Tool::Picotool => &["version"],
        Tool::ArduinoCli | Tool::PlatformIo | Tool::EspIdf | Tool::DfuUtil => &["--version"],
        // BOSSA, avrdude and the Teensy loader have no version subcommand that
        // exits cleanly, so their path is the detail worth showing.
        Tool::Bossac | Tool::Avrdude | Tool::TeensyLoader => &[],
    }
}

fn version_of(tool: Tool) -> Option<String> {
    let args = version_args(tool);
    if args.is_empty() {
        return None;
    }
    let output = command_for(tool)?.args(args).output().ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr)
    } else {
        String::from_utf8_lossy(&output.stdout)
    };
    let first = text.lines().find(|line| !line.trim().is_empty())?;
    Some(crate::device::strip_ansi(first).trim().to_string())
}

// ---------------------------------------------------------------------------
// What a board needs
// ---------------------------------------------------------------------------

/// The tool that writes a firmware image with this method.
pub fn flash_tool(method: FlashMethod) -> Option<Tool> {
    match method {
        FlashMethod::Esptool => Some(Tool::Esptool),
        // A mass-storage bootloader is a file copy; nothing to install.
        FlashMethod::MassStorage { .. } => None,
        FlashMethod::DfuUtil => Some(Tool::DfuUtil),
        FlashMethod::Bossac => Some(Tool::Bossac),
        FlashMethod::TeensyLoader => Some(Tool::TeensyLoader),
        FlashMethod::Unsupported => None,
    }
}

/// The tools needed to put a program on `board` using `runtime`, in the order
/// they come into play.
pub fn required_tools(board: &BoardDef, runtime: Runtime) -> Vec<Tool> {
    let mut tools = Vec::new();
    match runtime {
        Runtime::MicroPython => {
            tools.push(Tool::Python);
            tools.push(Tool::Mpremote);
        }
        // CircuitPython deploys by copying onto the CIRCUITPY drive.
        Runtime::CircuitPython => {}
        Runtime::Arduino => tools.push(Tool::ArduinoCli),
        Runtime::Native => {
            if board.chip.is_espressif() {
                tools.push(Tool::EspIdf);
            }
            tools.push(Tool::PlatformIo);
        }
    }
    if runtime.is_interpreted() {
        if let Some(tool) = flash_tool(board.flash) {
            tools.push(tool);
            if tool == Tool::Esptool && !tools.contains(&Tool::Python) {
                tools.insert(0, Tool::Python);
            }
        }
    }
    tools.dedup();
    tools
}

// ---------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------

/// Install `tool`, streaming progress through `emit`.
///
/// Only two install routes are used, both of which name an exact package from
/// a package manager already on the machine: pip for the Python tools and
/// Homebrew for the native ones. Anything else returns the command for the
/// user to run themselves rather than piping a remote script into a shell.
pub fn install(tool: Tool, emit: &mut dyn FnMut(crate::device::DeviceEvent)) -> Result<(), String> {
    use crate::device::DeviceEvent::Progress;

    if let Some(package) = tool.pip_package() {
        emit(Progress(format!("Installing {} with pip…", tool.name())));
        return pip_install(package, emit);
    }
    if let Some(formula) = tool.brew_formula() {
        if let Some(brew) = locate_executable("brew") {
            emit(Progress(format!(
                "Installing {} with Homebrew…",
                tool.name()
            )));
            let mut command = Command::new(brew);
            command.args(["install", formula]);
            return crate::device::run_streaming(&mut command, emit).map(|_| ());
        }
        return Err(format!(
            "Homebrew is not installed. Install {} yourself: {}",
            tool.name(),
            tool.install_hint()
        ));
    }
    Err(format!(
        "Anode cannot install {}. {}",
        tool.name(),
        tool.install_hint()
    ))
}

/// Try each pip invocation in turn. A managed system Python needs
/// `--break-system-packages`, and some installs only have `pip3`.
fn pip_install(
    package: &str,
    emit: &mut dyn FnMut(crate::device::DeviceEvent),
) -> Result<(), String> {
    use crate::device::DeviceEvent::Progress;

    let attempts: Vec<Vec<String>> = if cfg!(windows) {
        vec![
            vec![
                "python".into(),
                "-m".into(),
                "pip".into(),
                "install".into(),
                package.into(),
            ],
            vec!["pip".into(), "install".into(), package.into()],
        ]
    } else {
        vec![
            vec![
                "python3".into(),
                "-m".into(),
                "pip".into(),
                "install".into(),
                "--user".into(),
                package.into(),
            ],
            vec![
                "python3".into(),
                "-m".into(),
                "pip".into(),
                "install".into(),
                "--user".into(),
                "--break-system-packages".into(),
                package.into(),
            ],
            vec![
                "pip3".into(),
                "install".into(),
                "--user".into(),
                package.into(),
            ],
        ]
    };

    let mut last_error = String::new();
    for attempt in attempts {
        let Some(binary) = locate_executable(&attempt[0]) else {
            continue;
        };
        emit(Progress(format!("Running: {}", attempt.join(" "))));
        let mut command = Command::new(binary);
        command.args(&attempt[1..]);
        match crate::device::run_streaming(&mut command, emit) {
            Ok(output) if output.status.success() => {
                emit(Progress(format!("[OK] {package} installed")));
                return Ok(());
            }
            Ok(output) => last_error = output.tail,
            Err(error) => last_error = error,
        }
    }
    Err(if last_error.is_empty() {
        format!("Could not install {package}. Install Python 3 from python.org, then retry.")
    } else {
        last_error.trim().to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::board_by_id;

    #[test]
    fn tool_ids_round_trip() {
        for tool in Tool::ALL {
            assert_eq!(Tool::from_id(tool.id()), Some(*tool));
        }
    }

    #[test]
    fn micropython_on_an_esp32_needs_mpremote_and_esptool() {
        let board = board_by_id("esp32-generic").unwrap();
        let tools = required_tools(board, Runtime::MicroPython);
        assert!(tools.contains(&Tool::Mpremote));
        assert!(tools.contains(&Tool::Esptool));
        assert!(tools.contains(&Tool::Python));
    }

    #[test]
    fn micropython_on_a_pico_needs_no_flasher() {
        // The Pico's bootloader is a mounted disk, so the firmware step is a
        // file copy and has no tool to install.
        let board = board_by_id("rpi-pico").unwrap();
        let tools = required_tools(board, Runtime::MicroPython);
        assert!(tools.contains(&Tool::Mpremote));
        assert!(!tools.contains(&Tool::Esptool));
    }

    #[test]
    fn arduino_needs_only_the_cli() {
        let board = board_by_id("arduino-uno").unwrap();
        assert_eq!(
            required_tools(board, Runtime::Arduino),
            vec![Tool::ArduinoCli]
        );
    }

    #[test]
    fn native_on_espressif_offers_idf_and_platformio() {
        let board = board_by_id("esp32-s3-devkitc-1").unwrap();
        let tools = required_tools(board, Runtime::Native);
        assert_eq!(tools, vec![Tool::EspIdf, Tool::PlatformIo]);
    }

    #[test]
    fn native_elsewhere_is_platformio_only() {
        let board = board_by_id("rpi-pico").unwrap();
        assert_eq!(
            required_tools(board, Runtime::Native),
            vec![Tool::PlatformIo]
        );
    }

    #[test]
    fn circuitpython_deploys_without_a_tool() {
        let board = board_by_id("adafruit-feather-rp2040").unwrap();
        assert!(required_tools(board, Runtime::CircuitPython).is_empty());
    }

    #[test]
    fn install_hints_name_a_runnable_command() {
        for tool in Tool::ALL {
            let hint = tool.install_hint();
            assert!(!hint.is_empty(), "{} has no install hint", tool.name());
        }
    }

    #[test]
    fn locating_a_missing_binary_finds_nothing() {
        assert!(locate_executable("anode-tool-that-does-not-exist").is_none());
    }
}
