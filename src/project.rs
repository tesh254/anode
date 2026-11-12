//! Project files and local filesystem operations.
//!
//! Replaces the Electron `read-directory` / `read-file` / `write-file` /
//! `create-file` / `create-directory` IPC handlers: in a native process these
//! are plain function calls.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    pub name: String,
    pub path: PathBuf,
    pub content: String,
    pub language: Language,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Python,
    Arduino,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::Arduino => "arduino",
        }
    }
}

pub fn language_for_file(name: &str) -> Language {
    if name.to_lowercase().ends_with(".ino") {
        Language::Arduino
    } else {
        Language::Python
    }
}

/// Read every `.py` / `.ino` file directly inside `dir` (one level, matching
/// the Electron app's project loader).
pub fn load_project(dir: &Path) -> std::io::Result<Vec<ProjectFile>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
        if !is_file {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let is_py = name.to_lowercase().ends_with(".py");
        let is_ino = name.to_lowercase().ends_with(".ino");
        if !(is_py || is_ino) {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        files.push(ProjectFile {
            name: name.to_string(),
            path: path.clone(),
            content,
            language: language_for_file(name),
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

/// Create a file (and any missing parent directories) with initial content.
pub fn create_file(dir: &Path, file_name: &str, content: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file_name);
    std::fs::write(&path, content)?;
    Ok(path)
}

pub fn create_directory(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

pub fn write_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)
}

/// A lightweight directory entry for tree views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_directory: bool,
}

pub fn read_directory(dir: &Path) -> Vec<DirEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<DirEntry> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let is_directory = e.file_type().ok()?.is_dir();
            let path = e.path();
            let name = path.file_name()?.to_str()?.to_string();
            Some(DirEntry {
                name,
                path,
                is_directory,
            })
        })
        .collect();
    out.sort_by(|a, b| (b.is_directory, &a.name).cmp(&(a.is_directory, &b.name)));
    out
}

/// Templates for the welcome screen. Translated from `src/lib/templates.ts`
/// (the subset shown on the welcome screen).
#[derive(Debug, Clone)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub language: Language,
    pub code: &'static str,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "blank",
        name: "Blank Project",
        description: "Start from scratch with an empty file",
        language: Language::Python,
        code: "# main.py\n",
    },
    Template {
        id: "blink",
        name: "Blink",
        description: "LED blink example - the hello world of microcontrollers",
        language: Language::Python,
        code: concat!(
            "import machine\n",
            "import time\n",
            "\n",
            "# Configure the built-in LED (GPIO 2 on most ESP32 boards)\n",
            "led = machine.Pin(2, machine.Pin.OUT)\n",
            "\n",
            "print(\"Starting blink example...\")\n",
            "print(\"LED will blink every 500ms\")\n",
            "\n",
            "while True:\n",
            "    led.value(1)  # LED on\n",
            "    time.sleep(0.5)\n",
            "    led.value(0)  # LED off\n",
            "    time.sleep(0.5)\n",
        ),
    },
    Template {
        id: "wifi-connect",
        name: "WiFi Connect",
        description: "Connect to WiFi and display IP address",
        language: Language::Python,
        code: concat!(
            "import network\n",
            "import time\n",
            "\n",
            "# WiFi credentials\n",
            "SSID = \"your_wifi_ssid\"\n",
            "PASSWORD = \"your_wifi_password\"\n",
            "\n",
            "def connect_wifi():\n",
            "    wlan = network.WLAN(network.STA_IF)\n",
            "    wlan.active(True)\n",
            "\n",
            "    if not wlan.isconnected():\n",
            "        print(f\"Connecting to {SSID}...\")\n",
            "        wlan.connect(SSID, PASSWORD)\n",
            "\n",
            "        # Wait for connection with timeout\n",
            "        timeout = 30\n",
            "        while not wlan.isconnected() and timeout > 0:\n",
            "            time.sleep(1)\n",
            "            timeout -= 1\n",
            "            print(f\"Waiting... {timeout}s remaining\")\n",
            "\n",
            "    if wlan.isconnected():\n",
            "        print(f\"Connected! IP: {wlan.ifconfig()[0]}\")\n",
            "        return True\n",
            "    else:\n",
            "        print(\"Failed to connect\")\n",
            "        return False\n",
            "\n",
            "# Run\n",
            "connect_wifi()\n",
        ),
    },
];

/// The default file name a template creates.
pub fn template_main_file(template: &Template) -> &'static str {
    match template.language {
        Language::Arduino => "sketch.ino",
        Language::Python => "main.py",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_only_code_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.py"), "print('hi')").unwrap();
        std::fs::write(dir.path().join("Blink.ino"), "void setup() {}").unwrap();
        std::fs::write(dir.path().join("README.md"), "docs").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let files = load_project(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "Blink.ino");
        assert_eq!(files[0].language, Language::Arduino);
        assert_eq!(files[1].name, "main.py");
        assert_eq!(files[1].language, Language::Python);
    }

    #[test]
    fn create_file_writes_and_returns_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_file(dir.path(), "new.py", "# New file\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# New file\n");
    }

    #[test]
    fn templates_create_the_right_main_file() {
        assert_eq!(template_main_file(&TEMPLATES[0]), "main.py");
    }
}
