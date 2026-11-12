//! Execution profiles: which runtime a port should run, and how the
//! workspace lays out for it.
//!
//! A profile is the pairing of a port with a runtime. The board catalog says
//! which runtimes a board can execute; this module decides which of them is
//! active, keeps a manual choice from being overwritten by re-detection, and
//! describes the workspace each runtime needs.

use crate::boards::{self, BoardDef, BoardMatch, PortIdentity, Runtime};

/// How sure we are that a profile is the right one for the port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The board was identified, or the board itself told us (a mounted
    /// CIRCUITPY drive, a REPL that answered).
    High,
    /// A fallback offered so every runtime stays reachable.
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

/// One runtime choice for a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionProfile {
    pub id: String,
    pub runtime: Runtime,
    /// The catalog board this profile targets, when one was identified.
    pub board_id: Option<&'static str>,
    /// What the picker shows, e.g. "MicroPython · XIAO ESP32S3".
    pub label: String,
    pub port_path: String,
    /// The `arduino-cli` FQBN for this board, when the runtime needs one.
    pub fqbn: Option<String>,
    /// True for a fallback that is not tied to an identified board.
    pub is_generic: bool,
    pub confidence: Confidence,
}

impl ExecutionProfile {
    pub fn board(&self) -> Option<&'static BoardDef> {
        self.board_id.and_then(boards::board_by_id)
    }
}

/// Ports that look like real hardware rather than a system or Bluetooth port.
pub fn is_likely_hardware_port(port: &PortIdentity) -> bool {
    if boards::is_known_vendor(port) {
        return true;
    }
    let path = port.path.to_lowercase();
    let manufacturer = port.manufacturer.as_deref().unwrap_or("").to_lowercase();

    // macOS exposes a handful of virtual ports that are never a board.
    const NEVER_HARDWARE: &[&str] = &[
        "debug-console",
        "bluetooth",
        "incoming-port",
        "airpods",
        "redmibuds",
        "wireless",
    ];
    if NEVER_HARDWARE
        .iter()
        .any(|marker| path.contains(marker) || manufacturer.contains(marker))
    {
        return false;
    }

    path.contains("usb")
        || path.contains("acm")
        || path.contains("serial")
        || path.contains("modem")
        || path.contains("uart")
}

/// The port most likely to be the board the user means.
pub fn preferred_port(ports: &[PortIdentity]) -> Option<&PortIdentity> {
    let likely: Vec<&PortIdentity> = ports
        .iter()
        .filter(|port| is_likely_hardware_port(port))
        .collect();
    // A port whose vendor id is in the catalog beats one that only looks like
    // a serial device by its path.
    likely
        .iter()
        .find(|port| boards::is_known_vendor(port))
        .or_else(|| likely.iter().find(|port| port.path.starts_with("/dev/cu.")))
        .or_else(|| likely.first())
        .copied()
}

/// Build the stable id for a profile. Port path and runtime together are what
/// make a profile unique, so a board swap on the same port produces a new id
/// only when the runtime set changes with it.
fn profile_id(runtime: Runtime, port_path: &str) -> String {
    format!("{}@{port_path}", runtime.id())
}

/// Detect the runtime choices for a port.
///
/// The order is the priority order: a mounted CIRCUITPY drive first, then the
/// identified board's own runtimes in catalog order, then a generic fallback
/// for every runtime so nothing is unreachable on an unknown board.
pub fn detect_profiles(
    port: &PortIdentity,
    board: &BoardMatch,
    circuit_python: Option<&CircuitPythonDrive>,
) -> Vec<ExecutionProfile> {
    let port_path = port.path.clone();
    let mut profiles: Vec<ExecutionProfile> = Vec::new();

    let push = |runtime: Runtime,
                board_id: Option<&'static str>,
                label: String,
                fqbn: Option<String>,
                is_generic: bool,
                confidence: Confidence,
                profiles: &mut Vec<ExecutionProfile>| {
        let id = profile_id(runtime, &port_path);
        if profiles.iter().any(|existing| existing.id == id) {
            return;
        }
        profiles.push(ExecutionProfile {
            id,
            runtime,
            board_id,
            label,
            port_path: port_path.clone(),
            fqbn,
            is_generic,
            confidence,
        });
    };

    if let Some(drive) = circuit_python.filter(|drive| drive.present) {
        let name = drive.board_name.as_deref().unwrap_or("CIRCUITPY drive");
        push(
            Runtime::CircuitPython,
            board.board.map(|board| board.id),
            format!("CircuitPython · {name}"),
            None,
            false,
            Confidence::High,
            &mut profiles,
        );
    }

    if let Some(resolved) = board.board {
        for runtime in resolved.runtimes {
            push(
                *runtime,
                Some(resolved.id),
                format!("{} · {}", runtime.label(), resolved.name),
                resolved.fqbn.map(str::to_string),
                resolved.generic,
                // An identified board is a real signal; a family guess still
                // tells us which runtimes are possible, just not which board.
                if board.exact || board.from_override {
                    Confidence::High
                } else {
                    Confidence::Low
                },
                &mut profiles,
            );
        }
    }

    // Fallbacks, so an unknown board can still be driven.
    for runtime in [
        Runtime::MicroPython,
        Runtime::CircuitPython,
        Runtime::Arduino,
        Runtime::Native,
    ] {
        push(
            runtime,
            None,
            format!("{} · unidentified board", runtime.label()),
            None,
            true,
            Confidence::Low,
            &mut profiles,
        );
    }

    profiles
}

/// The profile to use when nothing has been chosen.
pub fn select_best_profile(profiles: &[ExecutionProfile]) -> Option<ExecutionProfile> {
    profiles
        .iter()
        .find(|profile| profile.confidence == Confidence::High && !profile.is_generic)
        .or_else(|| profiles.iter().find(|profile| !profile.is_generic))
        .or_else(|| profiles.first())
        .cloned()
}

/// Decide which profile should be active after a port/board (re)detection.
///
/// - no current profile (fresh launch) → restore the saved one, else the best;
/// - a MANUAL pick is preserved while still valid for this port; only a
///   genuine board swap replaces it;
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
            let saved = saved_id.and_then(|id| profiles.iter().find(|profile| profile.id == id));
            saved.cloned().or(best)
        }
        Some(current) => {
            if manual {
                let still_valid = profiles.iter().any(|profile| profile.id == current.id);
                if still_valid {
                    // Keep the choice, but refresh the board it points at: the
                    // user picked a runtime, not a stale board identification.
                    return profiles
                        .iter()
                        .find(|profile| profile.id == current.id)
                        .cloned();
                }
                best.or_else(|| Some(current.clone()))
            } else {
                best.or_else(|| Some(current.clone()))
            }
        }
    }
}

/// How the workspace lays out for a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceLayout {
    /// MicroPython and CircuitPython hold a filesystem worth browsing.
    pub show_device_files: bool,
    /// Source runs directly on the board, with no build step.
    pub show_run: bool,
    /// The program is compiled before it reaches the board.
    pub show_build: bool,
    pub show_upload: bool,
    /// Firmware is a thing the user replaces on this runtime.
    pub show_firmware: bool,
}

pub fn workspace_layout(runtime: Runtime) -> WorkspaceLayout {
    match runtime {
        Runtime::MicroPython => WorkspaceLayout {
            show_device_files: true,
            show_run: true,
            show_build: false,
            show_upload: true,
            show_firmware: true,
        },
        Runtime::CircuitPython => WorkspaceLayout {
            show_device_files: true,
            // A CircuitPython board re-runs code.py whenever the file lands,
            // so deploying is running.
            show_run: true,
            show_build: false,
            show_upload: true,
            show_firmware: true,
        },
        Runtime::Arduino | Runtime::Native => WorkspaceLayout {
            show_device_files: false,
            show_run: false,
            show_build: true,
            show_upload: true,
            show_firmware: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::identify_board;
    use std::collections::HashMap;

    fn port(vid: &str, pid: &str, path: &str) -> PortIdentity {
        PortIdentity {
            path: path.into(),
            vendor_id: Some(vid.into()),
            product_id: Some(pid.into()),
            ..Default::default()
        }
    }

    fn profiles_for(port: &PortIdentity) -> Vec<ExecutionProfile> {
        let board = identify_board(Some(port), &HashMap::new());
        detect_profiles(port, &board, None)
    }

    #[test]
    fn bluetooth_ports_are_not_hardware() {
        let p = PortIdentity {
            path: "/dev/cu.AirPods-Pro".into(),
            ..Default::default()
        };
        assert!(!is_likely_hardware_port(&p));
    }

    #[test]
    fn a_catalog_vendor_beats_a_path_that_merely_looks_serial() {
        let ports = vec![
            PortIdentity {
                path: "/dev/cu.usbserial-0001".into(),
                ..Default::default()
            },
            port("2e8a", "0005", "/dev/cu.usbmodem1101"),
        ];
        assert_eq!(preferred_port(&ports).unwrap().path, "/dev/cu.usbmodem1101");
    }

    #[test]
    fn an_identified_board_offers_exactly_its_runtimes_with_high_confidence() {
        let p = port("2341", "0043", "/dev/cu.usbmodem1");
        let profiles = profiles_for(&p);
        let uno = crate::boards::board_by_id("arduino-uno").unwrap();
        for runtime in uno.runtimes {
            let found = profiles
                .iter()
                .find(|profile| profile.runtime == *runtime)
                .expect("every runtime the board supports is offered");
            assert_eq!(found.confidence, Confidence::High);
            assert_eq!(found.board_id, Some("arduino-uno"));
        }
        let best = select_best_profile(&profiles).unwrap();
        assert_eq!(best.runtime, Runtime::Arduino);
        assert_eq!(best.fqbn.as_deref(), Some("arduino:avr:uno"));
    }

    #[test]
    fn a_family_guess_stays_low_confidence() {
        let profiles = profiles_for(&port("303a", "1001", "/dev/cu.usbmodem1"));
        assert!(
            profiles.iter().all(|p| p.confidence == Confidence::Low),
            "a shared native-USB descriptor cannot identify a board"
        );
    }

    #[test]
    fn circuit_python_drive_beats_a_usb_guess() {
        let p = port("303a", "1001", "/dev/cu.usbmodem1");
        let board = identify_board(Some(&p), &HashMap::new());
        let profiles = detect_profiles(
            &p,
            &board,
            Some(&CircuitPythonDrive {
                present: true,
                mount_path: Some("/Volumes/CIRCUITPY".into()),
                board_name: Some("Adafruit Feather ESP32-S3".into()),
                version: Some("9.0.0".into()),
            }),
        );
        let best = select_best_profile(&profiles).unwrap();
        assert_eq!(best.runtime, Runtime::CircuitPython);
        assert!(best.label.contains("Feather ESP32-S3"));
    }

    #[test]
    fn every_runtime_stays_reachable_on_an_unknown_board() {
        let profiles = profiles_for(&port("dead", "beef", "/dev/cu.usbmodem1"));
        for runtime in [
            Runtime::MicroPython,
            Runtime::CircuitPython,
            Runtime::Arduino,
            Runtime::Native,
        ] {
            assert!(profiles.iter().any(|profile| profile.runtime == runtime));
        }
        assert!(select_best_profile(&profiles).unwrap().is_generic);
    }

    #[test]
    fn manual_pick_survives_redetect() {
        let profiles = profiles_for(&port("303a", "1001", "/dev/cu.usbmodem1"));
        let chosen = profiles.last().unwrap().clone();
        let next = resolve_profile_on_detect(Some(&chosen), &profiles, None, true);
        assert_eq!(next, Some(chosen));
    }

    #[test]
    fn board_swap_replaces_manual_pick() {
        let old = profiles_for(&port("10c4", "ea60", "/dev/cu.usbserial-A"));
        let chosen = old.first().unwrap().clone();
        let new = profiles_for(&port("2e8a", "0005", "/dev/cu.usbmodem-B"));
        // The manual pick's id names the old port, so it is gone from the new
        // port's profile set.
        assert!(!new.iter().any(|profile| profile.id == chosen.id));
        let next = resolve_profile_on_detect(Some(&chosen), &new, None, true);
        assert_eq!(next, select_best_profile(&new));
    }

    #[test]
    fn auto_pick_tracks_best() {
        let profiles = profiles_for(&port("2341", "0043", "/dev/cu.usbmodem1"));
        let weak = profiles.last().unwrap().clone();
        let next = resolve_profile_on_detect(Some(&weak), &profiles, None, false);
        assert_eq!(next, select_best_profile(&profiles));
    }

    #[test]
    fn fresh_launch_restores_saved_profile() {
        let profiles = profiles_for(&port("303a", "1001", "/dev/cu.usbmodem1"));
        let saved = profiles.last().unwrap();
        let next = resolve_profile_on_detect(None, &profiles, Some(&saved.id), false);
        assert_eq!(next.as_ref(), Some(saved));
    }

    #[test]
    fn compiled_runtimes_build_instead_of_running() {
        let arduino = workspace_layout(Runtime::Arduino);
        assert!(arduino.show_build && !arduino.show_run);
        assert!(!arduino.show_firmware, "a sketch is the firmware");
        let micropython = workspace_layout(Runtime::MicroPython);
        assert!(micropython.show_run && !micropython.show_build);
        assert!(micropython.show_device_files);
    }
}
