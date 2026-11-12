//! Project files, the kind of project a folder holds, and the templates that
//! create one.

use crate::boards::Runtime;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    pub name: String,
    /// The path shown in the explorer, relative to the project root.
    pub relative: String,
    pub path: PathBuf,
    pub content: String,
    pub language: Language,
}

/// The languages Anode opens, and the highlighter each one uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Python,
    /// An Arduino sketch: C++ with the core's preamble implied.
    Arduino,
    C,
    Cpp,
    Header,
    Cmake,
    Ini,
    Toml,
    Markdown,
    Text,
}

impl Language {
    /// The tree-sitter grammar name the editor installs for this language.
    pub fn highlighter(self) -> &'static str {
        match self {
            Language::Python => "python",
            // The Arduino preprocessor emits C++, and that is what the editor
            // should colour.
            Language::Arduino | Language::Cpp | Language::Header => "cpp",
            Language::C => "c",
            Language::Cmake => "cmake",
            Language::Ini | Language::Toml => "toml",
            Language::Markdown | Language::Text => "text",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Language::Python => "Python",
            Language::Arduino => "Arduino",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Header => "Header",
            Language::Cmake => "CMake",
            Language::Ini => "INI",
            Language::Toml => "TOML",
            Language::Markdown => "Markdown",
            Language::Text => "Text",
        }
    }

    /// True for a file that goes onto a board as source.
    pub fn is_device_source(self) -> bool {
        self == Language::Python
    }
}

pub fn language_for_file(name: &str) -> Option<Language> {
    let lower = name.to_lowercase();
    if lower == "cmakelists.txt" || lower.ends_with(".cmake") {
        return Some(Language::Cmake);
    }
    if lower == "sdkconfig" || lower.starts_with("sdkconfig.") {
        return Some(Language::Ini);
    }
    let (_, extension) = lower.rsplit_once('.')?;
    Some(match extension {
        "py" => Language::Python,
        "ino" | "pde" => Language::Arduino,
        "c" => Language::C,
        "cpp" | "cc" | "cxx" => Language::Cpp,
        "h" | "hpp" | "hh" => Language::Header,
        "ini" => Language::Ini,
        "toml" => Language::Toml,
        "md" => Language::Markdown,
        _ => return None,
    })
}

/// The kind of project a folder holds, which decides how it is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    MicroPython,
    CircuitPython,
    ArduinoSketch,
    PlatformIo,
    EspIdf,
    Unknown,
}

impl ProjectKind {
    pub fn label(self) -> &'static str {
        match self {
            ProjectKind::MicroPython => "MicroPython",
            ProjectKind::CircuitPython => "CircuitPython",
            ProjectKind::ArduinoSketch => "Arduino sketch",
            ProjectKind::PlatformIo => "PlatformIO",
            ProjectKind::EspIdf => "ESP-IDF",
            ProjectKind::Unknown => "Files",
        }
    }

    /// The runtime this project is written for.
    pub fn runtime(self) -> Option<Runtime> {
        Some(match self {
            ProjectKind::MicroPython => Runtime::MicroPython,
            ProjectKind::CircuitPython => Runtime::CircuitPython,
            ProjectKind::ArduinoSketch => Runtime::Arduino,
            ProjectKind::PlatformIo | ProjectKind::EspIdf => Runtime::Native,
            ProjectKind::Unknown => return None,
        })
    }
}

/// Work out what kind of project `dir` holds from the files it contains.
///
/// The order matters: a PlatformIO project can contain a `.ino`, and an
/// ESP-IDF project is a CMake project with a specific shape, so the most
/// specific marker has to be checked first.
pub fn detect_project_kind(dir: &Path) -> ProjectKind {
    if dir.join("platformio.ini").is_file() {
        return ProjectKind::PlatformIo;
    }
    let cmake = dir.join("CMakeLists.txt");
    if cmake.is_file() {
        let declares_idf = std::fs::read_to_string(&cmake)
            .map(|text| text.contains("idf_component_register") || text.contains("$ENV{IDF_PATH}"))
            .unwrap_or(false);
        if declares_idf || dir.join("sdkconfig").is_file() || dir.join("main").is_dir() {
            return ProjectKind::EspIdf;
        }
    }
    let entries: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().to_str().map(str::to_lowercase))
                .collect()
        })
        .unwrap_or_default();
    if entries.iter().any(|name| name.ends_with(".ino")) {
        return ProjectKind::ArduinoSketch;
    }
    if entries
        .iter()
        .any(|name| name == "code.py" || name == "boot_out.txt")
    {
        return ProjectKind::CircuitPython;
    }
    if entries
        .iter()
        .any(|name| name == "main.py" || name == "boot.py")
    {
        return ProjectKind::MicroPython;
    }
    ProjectKind::Unknown
}

/// Directories that hold build output or tooling rather than source.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".pio",
    ".vscode",
    ".idea",
    "build",
    "node_modules",
    "__pycache__",
    "managed_components",
    "cmake-build-debug",
];

/// How many files a project may contribute before the explorer stops. A board
/// project is small; a cap keeps a mistakenly-opened home directory from
/// reading thousands of files on the UI's behalf.
const MAX_PROJECT_FILES: usize = 400;

/// Read the source and configuration files in `dir`, descending into
/// subdirectories (`src/`, `main/`, `lib/`) but skipping build output.
pub fn load_project(dir: &Path) -> std::io::Result<Vec<ProjectFile>> {
    let mut files = Vec::new();
    collect_files(dir, dir, 0, &mut files)?;
    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(files)
}

fn collect_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    files: &mut Vec<ProjectFile>,
) -> std::io::Result<()> {
    // Two levels covers src/, main/ and lib/<name>/ without turning the
    // explorer into a filesystem browser.
    const MAX_DEPTH: usize = 2;

    let mut subdirectories = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        if is_dir {
            if depth < MAX_DEPTH
                && !name.starts_with('.')
                && !SKIPPED_DIRS.contains(&name.to_lowercase().as_str())
            {
                subdirectories.push(path);
            }
            continue;
        }
        let Some(language) = language_for_file(name) else {
            continue;
        };
        if files.len() >= MAX_PROJECT_FILES {
            return Ok(());
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        files.push(ProjectFile {
            name: name.to_string(),
            relative,
            path,
            content,
            language,
        });
    }
    subdirectories.sort();
    for subdirectory in subdirectories {
        collect_files(root, &subdirectory, depth + 1, files)?;
    }
    Ok(())
}

/// Create a file (and any missing parent directories) with initial content.
pub fn create_file(dir: &Path, file_name: &str, content: &str) -> std::io::Result<PathBuf> {
    let path = dir.join(file_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)?;
    Ok(path)
}

pub fn write_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

/// A starting point for a new project. Several runtimes need more than one
/// file to build, so a template carries all of them; the first is the one the
/// editor opens.
#[derive(Debug, Clone)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub runtime: Runtime,
    pub files: &'static [(&'static str, &'static str)],
}

impl Template {
    /// The file the editor opens after creating the project.
    pub fn main_file(&self) -> &'static str {
        self.files
            .first()
            .map(|(name, _)| *name)
            .unwrap_or("main.py")
    }

    /// The folder name this template creates, under the templates root.
    pub fn directory_name(&self) -> String {
        self.id.replace('_', "-")
    }
}

const MICROPYTHON_BLINK: &str = "\
import time

from machine import Pin

# The built-in LED is GPIO 2 on most ESP32 boards and \"LED\" on a Pico.
led = Pin(2, Pin.OUT)

while True:
    led.value(1)
    time.sleep(0.5)
    led.value(0)
    time.sleep(0.5)
";

const MICROPYTHON_WIFI: &str = "\
import time

import network

SSID = \"your-network\"
PASSWORD = \"your-password\"


def connect(timeout=30):
    wlan = network.WLAN(network.STA_IF)
    wlan.active(True)
    if wlan.isconnected():
        return wlan.ifconfig()[0]

    print(\"Connecting to\", SSID)
    wlan.connect(SSID, PASSWORD)
    deadline = time.time() + timeout
    while not wlan.isconnected() and time.time() < deadline:
        time.sleep(1)

    if not wlan.isconnected():
        raise OSError(\"Could not join \" + SSID)
    return wlan.ifconfig()[0]


print(\"Address:\", connect())
";

const CIRCUITPYTHON_BLINK: &str = "\
import time

import board
import digitalio

led = digitalio.DigitalInOut(board.LED)
led.direction = digitalio.Direction.OUTPUT

while True:
    led.value = True
    time.sleep(0.5)
    led.value = False
    time.sleep(0.5)
";

const ARDUINO_BLINK: &str = "\
void setup() {
  pinMode(LED_BUILTIN, OUTPUT);
  Serial.begin(115200);
}

void loop() {
  digitalWrite(LED_BUILTIN, HIGH);
  delay(500);
  digitalWrite(LED_BUILTIN, LOW);
  delay(500);
  Serial.println(\"blink\");
}
";

const PLATFORMIO_INI: &str = "\
; Change `board` to match the board you are using. Anode fills in the id it
; knows for the selected board when you create the project from its panel.
[env:esp32dev]
platform = espressif32
board = esp32dev
framework = arduino
monitor_speed = 115200
";

const PLATFORMIO_MAIN: &str = "\
#include <Arduino.h>

void setup() {
  pinMode(LED_BUILTIN, OUTPUT);
  Serial.begin(115200);
}

void loop() {
  digitalWrite(LED_BUILTIN, HIGH);
  delay(500);
  digitalWrite(LED_BUILTIN, LOW);
  delay(500);
  Serial.println(\"blink\");
}
";

const IDF_MAIN: &str = "\
#include \"driver/gpio.h\"
#include \"freertos/FreeRTOS.h\"
#include \"freertos/task.h\"
#include \"esp_log.h\"

static const char *TAG = \"blink\";
#define BLINK_GPIO GPIO_NUM_2

void app_main(void) {
  gpio_reset_pin(BLINK_GPIO);
  gpio_set_direction(BLINK_GPIO, GPIO_MODE_OUTPUT);

  while (1) {
    gpio_set_level(BLINK_GPIO, 1);
    vTaskDelay(pdMS_TO_TICKS(500));
    gpio_set_level(BLINK_GPIO, 0);
    vTaskDelay(pdMS_TO_TICKS(500));
    ESP_LOGI(TAG, \"blink\");
  }
}
";

const IDF_MAIN_CMAKE: &str = "\
idf_component_register(SRCS \"main.c\" INCLUDE_DIRS \".\")
";

const IDF_ROOT_CMAKE: &str = "\
cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(blink)
";

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "micropython-blink",
        name: "MicroPython blink",
        description: "Toggle the built-in LED — the hello world of microcontrollers",
        runtime: Runtime::MicroPython,
        files: &[("main.py", MICROPYTHON_BLINK)],
    },
    Template {
        id: "micropython-wifi",
        name: "MicroPython Wi-Fi",
        description: "Join a network and print the board's address",
        runtime: Runtime::MicroPython,
        files: &[("main.py", MICROPYTHON_WIFI)],
    },
    Template {
        id: "circuitpython-blink",
        name: "CircuitPython blink",
        description: "code.py that the board re-runs every time you save it",
        runtime: Runtime::CircuitPython,
        files: &[("code.py", CIRCUITPYTHON_BLINK)],
    },
    Template {
        id: "arduino-blink",
        name: "Arduino sketch",
        description: "A sketch with setup and loop, ready for arduino-cli",
        runtime: Runtime::Arduino,
        files: &[("sketch.ino", ARDUINO_BLINK)],
    },
    Template {
        id: "platformio-arduino",
        name: "PlatformIO project",
        description: "platformio.ini and src/main.cpp for a native C++ build",
        runtime: Runtime::Native,
        files: &[
            ("src/main.cpp", PLATFORMIO_MAIN),
            ("platformio.ini", PLATFORMIO_INI),
        ],
    },
    Template {
        id: "esp-idf-blink",
        name: "ESP-IDF project",
        description: "A CMake project with main/main.c, built by idf.py",
        runtime: Runtime::Native,
        files: &[
            ("main/main.c", IDF_MAIN),
            ("main/CMakeLists.txt", IDF_MAIN_CMAKE),
            ("CMakeLists.txt", IDF_ROOT_CMAKE),
        ],
    },
];

/// Write every file of `template` into `dir`, returning the main file's path.
pub fn create_from_template(dir: &Path, template: &Template) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut main = None;
    for (name, content) in template.files {
        let path = create_file(dir, name, content)?;
        if main.is_none() {
            main = Some(path);
        }
    }
    main.ok_or_else(|| std::io::Error::other("the template declares no files"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_the_languages_it_opens() {
        assert_eq!(language_for_file("main.py"), Some(Language::Python));
        assert_eq!(language_for_file("Blink.ino"), Some(Language::Arduino));
        assert_eq!(language_for_file("main.cpp"), Some(Language::Cpp));
        assert_eq!(language_for_file("driver.c"), Some(Language::C));
        assert_eq!(language_for_file("pins.h"), Some(Language::Header));
        assert_eq!(language_for_file("CMakeLists.txt"), Some(Language::Cmake));
        assert_eq!(language_for_file("platformio.ini"), Some(Language::Ini));
        assert_eq!(language_for_file("sdkconfig"), Some(Language::Ini));
        assert_eq!(language_for_file("notes.txt"), None);
        assert_eq!(language_for_file("firmware.bin"), None);
    }

    #[test]
    fn arduino_and_cpp_share_the_cpp_highlighter() {
        assert_eq!(Language::Arduino.highlighter(), "cpp");
        assert_eq!(Language::Cpp.highlighter(), "cpp");
        assert_eq!(Language::Python.highlighter(), "python");
    }

    #[test]
    fn loads_nested_sources_and_skips_build_output() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("platformio.ini"), "[env:esp32]\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.cpp"), "int main() {}").unwrap();
        std::fs::create_dir_all(root.join(".pio/build")).unwrap();
        std::fs::write(root.join(".pio/build/firmware.cpp"), "noise").unwrap();
        std::fs::write(root.join("README.md"), "docs").unwrap();

        let files = load_project(root).unwrap();
        let names: Vec<&str> = files.iter().map(|file| file.relative.as_str()).collect();
        assert!(names.contains(&"platformio.ini"));
        assert!(names.contains(&"README.md"));
        assert!(names.iter().any(|name| name.ends_with("main.cpp")));
        assert!(
            !names.iter().any(|name| name.contains(".pio")),
            "build output must stay out of the explorer: {names:?}"
        );
    }

    #[test]
    fn detects_project_kinds_from_their_markers() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::Unknown);

        std::fs::write(dir.path().join("main.py"), "").unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::MicroPython);

        std::fs::write(dir.path().join("sketch.ino"), "").unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::ArduinoSketch);

        std::fs::write(dir.path().join("platformio.ini"), "").unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::PlatformIo);
    }

    #[test]
    fn an_idf_project_is_not_mistaken_for_a_plain_cmake_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CMakeLists.txt"),
            "include($ENV{IDF_PATH}/tools/cmake/project.cmake)\nproject(blink)\n",
        )
        .unwrap();
        assert_eq!(detect_project_kind(dir.path()), ProjectKind::EspIdf);

        let plain = tempfile::tempdir().unwrap();
        std::fs::write(plain.path().join("CMakeLists.txt"), "project(x)\n").unwrap();
        assert_eq!(detect_project_kind(plain.path()), ProjectKind::Unknown);
    }

    #[test]
    fn templates_create_every_file_they_declare() {
        for template in TEMPLATES {
            let dir = tempfile::tempdir().unwrap();
            let main = create_from_template(dir.path(), template).unwrap();
            assert!(main.ends_with(template.main_file()));
            for (name, _) in template.files {
                assert!(
                    dir.path().join(name).is_file(),
                    "{} did not create {name}",
                    template.id
                );
            }
        }
    }

    #[test]
    fn each_template_lands_as_the_project_kind_it_describes() {
        for template in TEMPLATES {
            let dir = tempfile::tempdir().unwrap();
            create_from_template(dir.path(), template).unwrap();
            let kind = detect_project_kind(dir.path());
            assert_eq!(
                kind.runtime(),
                Some(template.runtime),
                "{} was detected as {}",
                template.id,
                kind.label()
            );
        }
    }
}
