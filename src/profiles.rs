//! Execution profiles: which interpreter (MicroPython / CircuitPython /
//! Arduino) a port should run, and how the workspace lays out for it.
//!
//! Translated from `src/types/interpreter.ts` and the detection helpers in
//! `src/lib/store.ts`.

use crate::boards::{BoardType, PortIdentity};

/// USB signatures that identify a runtime. Order matters only within equal
/// specificity; `detect_profiles` scans every entry.
pub struct KnownSignature {
    pub vid: &'static str,
    pub pid_prefix: Option<&'static str>,
    pub runtime: super::boards::Runtime,
    pub variant: &'static str,
    pub board_name: &'static str,
    pub fqbn: Option<&'static str>,
}

pub const KNOWN_BOARD_SIGNATURES: &[KnownSignature] = &[
    KnownSignature {
        vid: "10c4",
        pid_prefix: Some("ea60"),
        runtime: super::boards::Runtime::MicroPython,
        variant: "ESP32",
        board_name: "M5Stack Atom Light",
        fqbn: None,
    },
    KnownSignature {
        vid: "303a",
        pid_prefix: Some("80"),
        runtime: super::boards::Runtime::MicroPython,
        variant: "ESP32-S3",
        board_name: "M5StampS3",
        fqbn: None,
    },
    KnownSignature {
        vid: "2e8a",
        pid_prefix: Some("0005"),
        runtime: super::boards::Runtime::MicroPython,
        variant: "RP2040",
        board_name: "Raspberry Pi Pico",
        fqbn: None,
    },
    // Adafruit's USB vendor id — these boards ship CircuitPython by default.
    KnownSignature {
        vid: "239a",
        pid_prefix: None,
        runtime: super::boards::Runtime::CircuitPython,
        variant: "Adafruit",
        board_name: "Adafruit CircuitPython",
        fqbn: None,
    },
    KnownSignature {
        vid: "2341",
        pid_prefix: None,
        runtime: super::boards::Runtime::Arduino,
        variant: "AVR",
        board_name: "Arduino Uno / Genuine",
        fqbn: Some("arduino:avr:uno"),
    },
    KnownSignature {
        vid: "2a03",
        pid_prefix: None,
        runtime: super::boards::Runtime::Arduino,
        variant: "AVR",
        board_name: "Arduino / Genuino",
        fqbn: Some("arduino:avr:uno"),
    },
    KnownSignature {
        vid: "1a86",
        pid_prefix: None,
        runtime: super::boards::Runtime::MicroPython,
        variant: "Generic ESP",
        board_name: "CH340 USB Bridge (Generic ESP)",
        fqbn: None,
    },
    KnownSignature {
        vid: "10c4",
        pid_prefix: None,
        runtime: super::boards::Runtime::MicroPython,
        variant: "Generic ESP",
        board_name: "CP210x USB Bridge (Generic ESP)",
        fqbn: None,
    },
    KnownSignature {
        vid: "303a",
        pid_prefix: None,
        runtime: super::boards::Runtime::MicroPython,
        variant: "ESP32-S3",
        board_name: "Espressif USB (Generic ESP32-S3)",
        fqbn: None,
    },
];

/// One detected interpreter choice for a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionProfile {
    pub id: String,
    pub runtime: super::boards::Runtime,
    pub variant: String,
    pub board_name: String,
    pub port_path: String,
    pub fqbn: Option<String>,
    pub is_generic: bool,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Low,
}

/// Result of the CIRCUITPY-drive probe. A mounted CIRCUITPY drive is the
/// authoritative CircuitPython signal and outranks USB-descriptor guesses —
/// a bare ESP32-S3 looks identical to MicroPython over USB, but a mounted
/// CIRCUITPY drive only ever means CircuitPython.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CircuitPythonDrive {
    pub present: bool,
    pub mount_path: Option<String>,
    pub board_name: Option<String>,
    pub version: Option<String>,
}

pub fn detect_board_type(port: &PortIdentity) -> BoardType {
    let vid = port.vendor_id.as_deref().unwrap_or("").to_ascii_lowercase();
    let pid = port
        .product_id
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let mfr = port
        .manufacturer
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let path = port.path.to_ascii_lowercase();

    // M5Stack Atom Light: CP2104 bridge (VID 10c4, PID ea60) on ESP32 PICO-D4.
    if vid == "10c4" && pid == "ea60" {
        return BoardType::M5StackAtom;
    }
    // M5StampS3: ESP32-S3 native USB (VID 303a, PID 80??).
    if vid == "303a" && pid.starts_with("80") {
        return BoardType::M5StampS3;
    }
    if vid == "10c4"
        || vid == "1a86"
        || vid == "303a"
        || mfr.contains("silicon labs")
        || mfr.contains("espressif")
    {
        return BoardType::Espressif;
    }
    if vid == "2e8a" || mfr.contains("raspberry") || mfr.contains("rp2040") || pid == "0005" {
        return BoardType::RaspberryPi;
    }
    if vid == "2341" || vid == "2a03" || mfr.contains("arduino") || mfr.contains("genuino") {
        return BoardType::Arduino;
    }

    // Path-based fallback when VID/PID aren't populated (some macOS drivers
    // omit them).
    if regex::Regex::new(r"usbserial-[a-z0-9]+$")
        .unwrap()
        .is_match(&path)
    {
        return BoardType::Espressif;
    }
    // CH340 on macOS: usbmod* but NOT usbmodem (that's ambiguous).
    if regex::Regex::new(r"usbmod[a-z0-9]+$")
        .unwrap()
        .is_match(&path)
    {
        return BoardType::Espressif;
    }
    // usbmodem — ambiguous; ESP32-S3 native USB is the more common modern case.
    if path.contains("usbmodem") {
        return BoardType::Espressif;
    }
    if regex::Regex::new(r"ttyacm\d+$").unwrap().is_match(&path) {
        return BoardType::Arduino;
    }
    if regex::Regex::new(r"rp2040|pico|rpi-rp2")
        .unwrap()
        .is_match(&path)
    {
        return BoardType::RaspberryPi;
    }

    BoardType::Unknown
}

/// Ports that look like real hardware rather than macOS system/Bluetooth.
pub fn is_likely_hardware_port(port: &PortIdentity) -> bool {
    if detect_board_type(port) != BoardType::Unknown {
        return true;
    }
    let path = port.path.to_lowercase();
    let mfr = port.manufacturer.as_deref().unwrap_or("").to_lowercase();

    if path.contains("debug-console")
        || path.contains("bluetooth")
        || path.contains("incoming-port")
        || path.contains("airpods")
        || path.contains("redmibuds")
        || mfr.contains("bluetooth")
    {
        return false;
    }

    path.contains("usb")
        || path.contains("acm")
        || path.contains("serial")
        || path.contains("modem")
        || path.contains("uart")
}

pub fn preferred_port(ports: &[PortIdentity]) -> Option<&PortIdentity> {
    if ports.is_empty() {
        return None;
    }
    let likely: Vec<&PortIdentity> = ports
        .iter()
        .filter(|p| is_likely_hardware_port(p))
        .collect();
    if likely.is_empty() {
        return None;
    }
    if let Some(known) = likely
        .iter()
        .find(|p| detect_board_type(p) != BoardType::Unknown)
    {
        return Some(known);
    }
    let cu = likely.iter().find(|p| p.path.starts_with("/dev/cu."));
    cu.or_else(|| likely.first()).copied()
}

/// Detect the execution profiles for a port. A mounted CIRCUITPY drive is
/// added first so it wins over any USB-descriptor match.
pub fn detect_profiles(
    port: &PortIdentity,
    circuit_python: Option<&CircuitPythonDrive>,
) -> Vec<ExecutionProfile> {
    let vid = port.vendor_id.as_deref().unwrap_or("").to_lowercase();
    let pid = port.product_id.as_deref().unwrap_or("").to_lowercase();
    let mfr = port.manufacturer.as_deref().unwrap_or("").to_lowercase();
    let port_path = port.path.clone();

    let mut profiles: Vec<ExecutionProfile> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let add = |runtime: super::boards::Runtime,
               variant: &str,
               board_name: &str,
               fqbn: Option<&str>,
               confidence: Confidence,
               is_generic: bool,
               profiles: &mut Vec<ExecutionProfile>,
               seen: &mut std::collections::HashSet<String>| {
        let slug: String = variant
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let id = format!("{}-{}-{}", runtime.label().to_lowercase(), slug, port_path);
        if seen.insert(id.clone()) {
            profiles.push(ExecutionProfile {
                id,
                runtime,
                variant: variant.to_string(),
                board_name: board_name.to_string(),
                port_path: port_path.clone(),
                fqbn: fqbn.map(|s| s.to_string()),
                is_generic,
                confidence,
            });
        }
    };

    if let Some(cp) = circuit_python.filter(|d| d.present) {
        let name = cp.board_name.clone();
        add(
            super::boards::Runtime::CircuitPython,
            name.as_deref().unwrap_or("CircuitPython"),
            &match &name {
                Some(n) => format!("{n} (CircuitPython)"),
                None => "CircuitPython board".to_string(),
            },
            None,
            Confidence::High,
            false,
            &mut profiles,
            &mut seen,
        );
    }

    for sig in KNOWN_BOARD_SIGNATURES {
        if !sig.vid.eq_ignore_ascii_case(&vid) {
            continue;
        }
        if let Some(prefix) = sig.pid_prefix {
            if !pid.starts_with(&prefix.to_lowercase()) {
                continue;
            }
        }
        add(
            sig.runtime,
            sig.variant,
            sig.board_name,
            sig.fqbn,
            Confidence::High,
            false,
            &mut profiles,
            &mut seen,
        );
    }

    if mfr.contains("arduino") || mfr.contains("genuino") {
        add(
            super::boards::Runtime::Arduino,
            "AVR",
            "Arduino (by manufacturer)",
            Some("arduino:avr:uno"),
            Confidence::High,
            false,
            &mut profiles,
            &mut seen,
        );
    }
    if mfr.contains("raspberry")
        || mfr.contains("rp2040")
        || regex::Regex::new(r"pico|rp2040|rpi-rp2")
            .unwrap()
            .is_match(&port_path)
    {
        add(
            super::boards::Runtime::MicroPython,
            "RP2040",
            "Raspberry Pi Pico (by path)",
            None,
            Confidence::High,
            false,
            &mut profiles,
            &mut seen,
        );
    }

    // Generic fallbacks, always available.
    add(
        super::boards::Runtime::MicroPython,
        "Generic",
        "MicroPython (generic)",
        None,
        Confidence::Low,
        true,
        &mut profiles,
        &mut seen,
    );
    add(
        super::boards::Runtime::CircuitPython,
        "Generic",
        "CircuitPython (generic)",
        None,
        Confidence::Low,
        true,
        &mut profiles,
        &mut seen,
    );
    add(
        super::boards::Runtime::Arduino,
        "Generic",
        "Arduino (generic)",
        Some("arduino:avr:uno"),
        Confidence::Low,
        true,
        &mut profiles,
        &mut seen,
    );

    profiles
}

pub fn select_best_profile(profiles: &[ExecutionProfile]) -> Option<ExecutionProfile> {
    profiles
        .iter()
        .find(|p| p.confidence == Confidence::High && !p.is_generic)
        .or_else(|| profiles.iter().find(|p| !p.is_generic))
        .or_else(|| profiles.first())
        .cloned()
}

/// Decide which profile should be active after a port/board (re)detection.
///
/// - no current profile (fresh launch) → restore the saved one, else the best;
/// - a MANUAL pick is preserved while still valid for this port; only a genuine
///   board swap replaces it;
/// - an AUTO-selected profile tracks the current best detection.
pub fn resolve_profile_on_detect(
    current: Option<&ExecutionProfile>,
    profiles: &[ExecutionProfile],
    saved_id: Option<&str>,
    manual: bool,
) -> Option<ExecutionProfile> {
    let best = select_best_profile(profiles);
    match current {
        None => {
            let saved = saved_id.and_then(|id| profiles.iter().find(|p| p.id == id));
            saved.cloned().or(best)
        }
        Some(current) => {
            if manual {
                let still_valid = profiles.iter().any(|p| p.id == current.id);
                if still_valid {
                    Some(current.clone())
                } else {
                    best.or_else(|| Some(current.clone()))
                }
            } else {
                best.or_else(|| Some(current.clone()))
            }
        }
    }
}

/// How the workspace lays out for a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceLayout {
    pub show_device_file_tree: bool,
    pub show_run_button: bool,
    pub show_compile_button: bool,
    pub show_upload_button: bool,
    pub is_arduino: bool,
}

pub fn workspace_layout(is_arduino: bool) -> WorkspaceLayout {
    if is_arduino {
        WorkspaceLayout {
            show_device_file_tree: false,
            show_run_button: false,
            show_compile_button: true,
            show_upload_button: true,
            is_arduino: true,
        }
    } else {
        WorkspaceLayout {
            show_device_file_tree: true,
            show_run_button: true,
            show_compile_button: false,
            show_upload_button: true,
            is_arduino: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(vid: &str, pid: &str, path: &str) -> PortIdentity {
        PortIdentity {
            path: path.into(),
            vendor_id: Some(vid.into()),
            product_id: Some(pid.into()),
            ..Default::default()
        }
    }

    #[test]
    fn detect_board_type_families() {
        assert_eq!(
            detect_board_type(&port("10c4", "ea60", "/dev/cu.usbserial-1")),
            BoardType::M5StackAtom
        );
        assert_eq!(
            detect_board_type(&port("303a", "8001", "/dev/cu.usbmodem1")),
            BoardType::M5StampS3
        );
        assert_eq!(
            detect_board_type(&port("303a", "1001", "/dev/cu.usbmodem1")),
            BoardType::Espressif
        );
        assert_eq!(
            detect_board_type(&port("2e8a", "0005", "/dev/cu.usbmodem1")),
            BoardType::RaspberryPi
        );
        assert_eq!(
            detect_board_type(&port("2341", "0001", "/dev/cu.usbmodem1")),
            BoardType::Arduino
        );
        assert_eq!(
            detect_board_type(&port("dead", "beef", "/dev/ttyS0")),
            BoardType::Unknown
        );
    }

    #[test]
    fn bluetooth_ports_are_not_hardware() {
        let p = port("00", "00", "/dev/cu.AirPods-Pro");
        assert!(!is_likely_hardware_port(&p));
    }

    #[test]
    fn circuit_python_drive_beats_usb_guess() {
        let profiles = detect_profiles(
            &port("303a", "1001", "/dev/cu.usbmodem1"),
            Some(&CircuitPythonDrive {
                present: true,
                mount_path: Some("/Volumes/CIRCUITPY".into()),
                board_name: Some("Adafruit Feather ESP32-S3".into()),
                version: Some("9.0.0".into()),
            }),
        );
        let best = select_best_profile(&profiles).unwrap();
        assert_eq!(best.runtime, crate::boards::Runtime::CircuitPython);
        assert_eq!(best.variant, "Adafruit Feather ESP32-S3");
    }

    #[test]
    fn generic_fallbacks_always_present() {
        let profiles = detect_profiles(&port("dead", "beef", "/dev/cu.usbmodem1"), None);
        assert!(profiles.iter().any(|p| p.is_generic));
        let best = select_best_profile(&profiles).unwrap();
        // A bare unknown port has no high-confidence signal; a generic wins.
        assert!(best.is_generic);
    }

    #[test]
    fn manual_pick_survives_redetect() {
        let profiles = detect_profiles(&port("303a", "1001", "/dev/cu.usbmodem1"), None);
        let chosen = profiles.first().unwrap().clone();
        let next = resolve_profile_on_detect(Some(&chosen), &profiles, None, true);
        assert_eq!(next, Some(chosen));
    }

    #[test]
    fn board_swap_replaces_manual_pick() {
        let old_profiles = detect_profiles(&port("10c4", "ea60", "/dev/cu.usbserial-A"), None);
        let chosen = old_profiles.first().unwrap().clone();
        let new_profiles = detect_profiles(&port("2e8a", "0005", "/dev/cu.usbmodem-B"), None);
        // The manual pick's id is gone from the new port's profile set.
        assert_ne!(chosen.port_path, new_profiles[0].port_path);
        let next = resolve_profile_on_detect(Some(&chosen), &new_profiles, None, true);
        assert_eq!(next, select_best_profile(&new_profiles));
    }

    #[test]
    fn auto_pick_tracks_best() {
        let profiles = detect_profiles(&port("303a", "1001", "/dev/cu.usbmodem1"), None);
        let weak = profiles.last().unwrap().clone();
        let next = resolve_profile_on_detect(Some(&weak), &profiles, None, false);
        assert_eq!(next, select_best_profile(&profiles));
    }

    #[test]
    fn fresh_launch_restores_saved_profile() {
        let profiles = detect_profiles(&port("303a", "1001", "/dev/cu.usbmodem1"), None);
        let saved = profiles.last().unwrap();
        let next = resolve_profile_on_detect(None, &profiles, Some(&saved.id), false);
        assert_eq!(next.as_ref(), Some(saved));
    }
}
