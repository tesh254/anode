//! The board catalog: every board Anode knows by name.
//!
//! Translated from `src/lib/boards.ts`. Detection from USB descriptors is
//! best-effort: some boards have a unique VID/PID and can be named exactly;
//! every ESP32-S3 board using the chip's *native* USB presents the identical
//! descriptor (303a:1001), so they can only be narrowed to a chip family. For
//! those we surface a picker; the choice is remembered per physical board.

/// The runtime a board executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    MicroPython,
    CircuitPython,
    Arduino,
}

impl Runtime {
    pub fn label(self) -> &'static str {
        match self {
            Runtime::MicroPython => "MicroPython",
            Runtime::CircuitPython => "CircuitPython",
            Runtime::Arduino => "Arduino",
        }
    }
}

/// The legacy board kind the flash/upload paths key off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardType {
    Espressif,
    M5StackAtom,
    M5StampS3,
    RaspberryPi,
    Arduino,
    #[default]
    Unknown,
}

impl BoardType {
    pub fn label(self) -> &'static str {
        match self {
            BoardType::Espressif => "Espressif (ESP32 / ESP8266)",
            BoardType::M5StackAtom => "M5Stack Atom Light (ESP32 PICO)",
            BoardType::M5StampS3 => "M5StampS3 (ESP32-S3)",
            BoardType::RaspberryPi => "Raspberry Pi Pico (RP2040)",
            BoardType::Arduino => "Arduino",
            BoardType::Unknown => "Unknown",
        }
    }

    /// The flashing tool this board family needs.
    pub fn tool(self) -> Option<&'static str> {
        match self {
            BoardType::Espressif | BoardType::M5StackAtom | BoardType::M5StampS3 => {
                Some("esptool.py")
            }
            BoardType::Arduino => Some("arduino-cli"),
            BoardType::RaspberryPi | BoardType::Unknown => None,
        }
    }

    /// The Espressif chip family that flashes this board kind.
    pub fn micro_python_chip(self) -> Option<&'static str> {
        match self {
            BoardType::M5StackAtom => Some("ESP32_GENERIC"),
            BoardType::M5StampS3 => Some("ESP32_GENERIC_S3"),
            BoardType::Espressif => Some("ESP32_GENERIC"),
            BoardType::RaspberryPi => Some("RPI_PICO"),
            _ => None,
        }
    }

    /// The firmware file extension the flasher downloads.
    pub fn firmware_extension(self) -> Option<&'static str> {
        match self {
            BoardType::RaspberryPi => Some(".uf2"),
            BoardType::Espressif | BoardType::M5StackAtom | BoardType::M5StampS3 => Some(".bin"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BoardDef {
    pub id: &'static str,
    /// Friendly name shown in the UI.
    pub name: &'static str,
    /// Short vendor/series label, e.g. "M5Stack", "Seeed Studio".
    pub vendor: &'static str,
    /// Silicon family — also groups boards into the same picker list.
    pub chip: Chip,
    pub runtime: Runtime,
    /// Legacy board kind the flash/upload handlers key off.
    pub flash_board_type: BoardType,
    /// MicroPython firmware variant (micropython.org slug), where applicable.
    pub mpy_variant: Option<&'static str>,
    /// USB descriptor hints used to auto-detect this board.
    pub vid: Option<&'static str>,
    pub pid_prefix: Option<&'static str>,
    /// True for chip-family fallbacks that aren't a specific product.
    pub generic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    Esp32,
    Esp32S3,
    Esp32C3,
    Rp2040,
    Avr,
}

impl Chip {
    pub fn label(self) -> &'static str {
        match self {
            Self::Esp32 => "ESP32",
            Self::Esp32S3 => "ESP32-S3",
            Self::Esp32C3 => "ESP32-C3",
            Self::Rp2040 => "RP2040",
            Self::Avr => "AVR",
        }
    }
}

/// The single source of truth for the boards Anode knows by name.
pub const BOARD_CATALOG: &[BoardDef] = &[
    BoardDef {
        id: "m5stack-atom-lite",
        name: "M5Stack ATOM Lite",
        vendor: "M5Stack",
        chip: Chip::Esp32,
        runtime: Runtime::MicroPython,
        flash_board_type: BoardType::M5StackAtom,
        mpy_variant: Some("ESP32_GENERIC"),
        // CP2104 USB-UART bridge — a unique, board-specific descriptor.
        vid: Some("10c4"),
        pid_prefix: Some("ea60"),
        generic: false,
    },
    BoardDef {
        id: "m5stamp-s3",
        name: "M5StampS3",
        vendor: "M5Stack",
        chip: Chip::Esp32S3,
        runtime: Runtime::MicroPython,
        flash_board_type: BoardType::M5StampS3,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        // Enumerates via ESP32-S3 native USB with an 0x80xx product id.
        vid: Some("303a"),
        pid_prefix: Some("80"),
        generic: false,
    },
    BoardDef {
        id: "esp32-s3-generic",
        name: "Generic ESP32-S3",
        vendor: "Espressif",
        chip: Chip::Esp32S3,
        runtime: Runtime::MicroPython,
        flash_board_type: BoardType::Espressif,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        vid: Some("303a"),
        pid_prefix: None,
        generic: true,
    },
    BoardDef {
        id: "esp32-generic",
        name: "Generic ESP32",
        vendor: "Espressif",
        chip: Chip::Esp32,
        runtime: Runtime::MicroPython,
        flash_board_type: BoardType::Espressif,
        mpy_variant: Some("ESP32_GENERIC"),
        vid: Some("10c4"), // CP210x bridge, non-Atom
        pid_prefix: None,
        generic: true,
    },
    BoardDef {
        id: "esp32-ch340",
        name: "Generic ESP32 (CH340)",
        vendor: "Espressif",
        chip: Chip::Esp32,
        runtime: Runtime::MicroPython,
        flash_board_type: BoardType::Espressif,
        mpy_variant: Some("ESP32_GENERIC"),
        vid: Some("1a86"), // CH340 bridge
        pid_prefix: None,
        generic: true,
    },
    BoardDef {
        id: "rpi-pico",
        name: "Raspberry Pi Pico",
        vendor: "Raspberry Pi",
        chip: Chip::Rp2040,
        runtime: Runtime::MicroPython,
        flash_board_type: BoardType::RaspberryPi,
        mpy_variant: Some("RPI_PICO"),
        vid: Some("2e8a"),
        pid_prefix: None,
        generic: false,
    },
    BoardDef {
        id: "arduino-uno",
        name: "Arduino Uno",
        vendor: "Arduino",
        chip: Chip::Avr,
        runtime: Runtime::Arduino,
        flash_board_type: BoardType::Arduino,
        mpy_variant: None,
        vid: Some("2341"),
        pid_prefix: None,
        generic: false,
    },
];

pub fn board_by_id(id: &str) -> Option<&'static BoardDef> {
    BOARD_CATALOG.iter().find(|b| b.id == id)
}

/// The fields of a serial port that identify a board.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortIdentity {
    pub path: String,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
}

/// A stable key for remembering a board override across sessions.
pub fn port_key(port: &PortIdentity) -> String {
    port.serial_number
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| port.path.clone())
}

#[derive(Debug, Clone, Default)]
pub struct BoardMatch {
    /// Best resolved board (override, then auto-detect, then none).
    pub board: Option<&'static BoardDef>,
    /// True when a unique, board-specific descriptor identified it.
    pub exact: bool,
    /// True when the resolution came from a saved user override.
    pub from_override: bool,
    /// Board ids the user can pick from to refine an ambiguous match.
    pub candidates: Vec<&'static str>,
}

/// Score how specifically a catalog entry matches a port. Higher means a more
/// distinguishing match (pid > vid). Returns -1 for no match.
fn score_match(board: &BoardDef, port: &PortIdentity) -> i32 {
    let Some(vid) = board.vid else { return -1 };
    let Some(port_vid) = &port.vendor_id else {
        return -1;
    };
    if !vid.eq_ignore_ascii_case(port_vid) {
        return -1;
    }
    let mut score = 1;
    if let Some(prefix) = board.pid_prefix {
        let Some(pid) = &port.product_id else {
            return -1;
        };
        if !pid
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase())
        {
            return -1;
        }
        score += 2;
    }
    score
}

/// Resolve which board is on a port. A saved override always wins. Otherwise
/// the highest-scoring catalog match is used; if that match is only a
/// chip-family fallback (generic), `exact` is false and `candidates` lists the
/// boards in that family so the UI can offer a picker.
pub fn identify_board(
    port: Option<&PortIdentity>,
    overrides: &std::collections::HashMap<String, String>,
) -> BoardMatch {
    let Some(port) = port else {
        return BoardMatch::default();
    };

    let mut best: Option<&'static BoardDef> = None;
    let mut best_score = -1;
    for board in BOARD_CATALOG {
        let score = score_match(board, port);
        // Prefer specific boards over generics on score ties.
        let wins = score > best_score
            || (score == best_score && best.is_some_and(|b| b.generic) && !board.generic);
        if wins {
            best = Some(board);
            best_score = score;
        }
    }

    let from_override = overrides
        .get(&port_key(port))
        .and_then(|id| board_by_id(id));
    let resolved = from_override.or(best);

    let candidates = resolved
        .map(|resolved| {
            let mut named: Vec<&'static str> = BOARD_CATALOG
                .iter()
                .filter(|b| b.chip == resolved.chip && !b.generic)
                .map(|b| b.id)
                .collect();
            let generic = BOARD_CATALOG
                .iter()
                .filter(|b| b.chip == resolved.chip && b.generic)
                .map(|b| b.id)
                .next();
            named.extend(generic);
            named
        })
        .unwrap_or_default();

    BoardMatch {
        board: resolved,
        exact: from_override.is_none() && best.is_some_and(|b| !b.generic),
        from_override: from_override.is_some(),
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(vid: &str, pid: &str) -> PortIdentity {
        PortIdentity {
            path: "/dev/cu.test".into(),
            vendor_id: Some(vid.into()),
            product_id: Some(pid.into()),
            ..Default::default()
        }
    }

    #[test]
    fn detects_atom_by_unique_vid_pid() {
        let m = identify_board(Some(&port("10c4", "ea60")), &Default::default());
        assert_eq!(m.board.unwrap().id, "m5stack-atom-lite");
        assert!(m.exact);
    }

    #[test]
    fn esp32s3_native_usb_is_generic_and_ambiguous() {
        let m = identify_board(Some(&port("303a", "1001")), &Default::default());
        assert_eq!(m.board.unwrap().id, "esp32-s3-generic");
        assert!(!m.exact);
        // Both S3 products plus one generic fallback are offered.
        assert_eq!(m.candidates, vec!["m5stamp-s3", "esp32-s3-generic"]);
    }

    #[test]
    fn override_wins_and_pins_the_board() {
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("key".to_string(), "m5stamp-s3".to_string());
        let p = PortIdentity {
            path: "key".into(),
            vendor_id: Some("303a".into()),
            product_id: Some("1001".into()),
            ..Default::default()
        };
        let m = identify_board(Some(&p), &overrides);
        assert_eq!(m.board.unwrap().id, "m5stamp-s3");
        assert!(m.from_override);
    }

    #[test]
    fn unknown_vid_matches_nothing() {
        let m = identify_board(Some(&port("dead", "beef")), &Default::default());
        assert!(m.board.is_none());
        assert!(m.candidates.is_empty());
    }

    #[test]
    fn no_port_has_no_board() {
        assert!(identify_board(None, &Default::default()).board.is_none());
    }
}
