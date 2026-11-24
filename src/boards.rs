//! The board catalog: every board Anode knows by name, the silicon it runs,
//! the runtimes it can execute and the tooling each of those needs.
//!
//! Detection from USB descriptors is best-effort. A few boards have a unique
//! VID/PID pair and can be named exactly; most do not. Every ESP32-S3 board
//! using the chip's *native* USB presents the identical descriptor
//! (`303a:1001`), and every board behind a CP210x bridge presents
//! `10c4:ea60`, so those can only be narrowed to a chip family. For those we
//! surface a picker and remember the choice per physical board.
//!
//! The toolchain identifiers below (`fqbn`, `pio_board`, `mpy_variant`,
//! `cpy_board`) are the published names for each board at the time of writing.
//! They are starting points, not guarantees: cores rename boards between
//! releases, so every one of them can be overridden per board in the UI.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Runtimes
// ---------------------------------------------------------------------------

/// The runtime a board executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Runtime {
    MicroPython,
    CircuitPython,
    Arduino,
    /// Native C / C++ through PlatformIO or ESP-IDF.
    Native,
}

impl Runtime {
    pub fn label(self) -> &'static str {
        match self {
            Runtime::MicroPython => "MicroPython",
            Runtime::CircuitPython => "CircuitPython",
            Runtime::Arduino => "Arduino",
            Runtime::Native => "C / C++",
        }
    }

    /// A stable key for settings and Select values.
    pub fn id(self) -> &'static str {
        match self {
            Runtime::MicroPython => "micropython",
            Runtime::CircuitPython => "circuitpython",
            Runtime::Arduino => "arduino",
            Runtime::Native => "native",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "micropython" => Runtime::MicroPython,
            "circuitpython" => Runtime::CircuitPython,
            "arduino" => Runtime::Arduino,
            "native" => Runtime::Native,
            _ => return None,
        })
    }

    /// True when code reaches the board as source, not as a compiled binary.
    pub fn is_interpreted(self) -> bool {
        matches!(self, Runtime::MicroPython | Runtime::CircuitPython)
    }
}

// ---------------------------------------------------------------------------
// Silicon
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    Xtensa,
    RiscV,
    Avr,
    ArmCortexM,
}

impl Architecture {
    pub fn label(self) -> &'static str {
        match self {
            Architecture::Xtensa => "Xtensa",
            Architecture::RiscV => "RISC-V",
            Architecture::Avr => "AVR",
            Architecture::ArmCortexM => "Arm Cortex-M",
        }
    }
}

/// The silicon a board is built on. Boards sharing a chip share a firmware
/// variant and a flashing procedure, which is what makes this the grouping
/// key for an ambiguous USB descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Chip {
    Esp8266,
    Esp32,
    Esp32S2,
    Esp32S3,
    Esp32C3,
    Esp32C6,
    Esp32H2,
    Rp2040,
    Rp2350,
    Atmega328p,
    Atmega2560,
    Atmega32u4,
    Atmega4809,
    Ra4m1,
    Samd21,
    Samd51,
    Sam3x8e,
    Nrf51822,
    Nrf52840,
    Stm32f1,
    Stm32f4,
    Stm32h7,
    Imxrt1062,
    Mk20dx256,
}

impl Chip {
    pub fn label(self) -> &'static str {
        match self {
            Chip::Esp8266 => "ESP8266",
            Chip::Esp32 => "ESP32",
            Chip::Esp32S2 => "ESP32-S2",
            Chip::Esp32S3 => "ESP32-S3",
            Chip::Esp32C3 => "ESP32-C3",
            Chip::Esp32C6 => "ESP32-C6",
            Chip::Esp32H2 => "ESP32-H2",
            Chip::Rp2040 => "RP2040",
            Chip::Rp2350 => "RP2350",
            Chip::Atmega328p => "ATmega328P",
            Chip::Atmega2560 => "ATmega2560",
            Chip::Atmega32u4 => "ATmega32U4",
            Chip::Atmega4809 => "ATmega4809",
            Chip::Ra4m1 => "Renesas RA4M1",
            Chip::Samd21 => "SAMD21",
            Chip::Samd51 => "SAMD51",
            Chip::Sam3x8e => "SAM3X8E",
            Chip::Nrf51822 => "nRF51822",
            Chip::Nrf52840 => "nRF52840",
            Chip::Stm32f1 => "STM32F1",
            Chip::Stm32f4 => "STM32F4",
            Chip::Stm32h7 => "STM32H7",
            Chip::Imxrt1062 => "i.MX RT1062",
            Chip::Mk20dx256 => "MK20DX256",
        }
    }

    pub fn architecture(self) -> Architecture {
        match self {
            Chip::Esp8266 | Chip::Esp32 | Chip::Esp32S2 | Chip::Esp32S3 => Architecture::Xtensa,
            Chip::Esp32C3 | Chip::Esp32C6 | Chip::Esp32H2 => Architecture::RiscV,
            Chip::Atmega328p | Chip::Atmega2560 | Chip::Atmega32u4 | Chip::Atmega4809 => {
                Architecture::Avr
            }
            _ => Architecture::ArmCortexM,
        }
    }

    pub fn silicon_vendor(self) -> &'static str {
        match self {
            Chip::Esp8266
            | Chip::Esp32
            | Chip::Esp32S2
            | Chip::Esp32S3
            | Chip::Esp32C3
            | Chip::Esp32C6
            | Chip::Esp32H2 => "Espressif",
            Chip::Rp2040 | Chip::Rp2350 => "Raspberry Pi",
            Chip::Atmega328p
            | Chip::Atmega2560
            | Chip::Atmega32u4
            | Chip::Atmega4809
            | Chip::Samd21
            | Chip::Samd51
            | Chip::Sam3x8e => "Microchip",
            Chip::Ra4m1 => "Renesas",
            Chip::Nrf51822 | Chip::Nrf52840 => "Nordic",
            Chip::Stm32f1 | Chip::Stm32f4 | Chip::Stm32h7 => "STMicroelectronics",
            Chip::Imxrt1062 | Chip::Mk20dx256 => "NXP",
        }
    }

    /// True for the Espressif parts `esptool` can talk to.
    pub fn is_espressif(self) -> bool {
        self.silicon_vendor() == "Espressif"
    }

    /// The offset `esptool write_flash` needs for a full MicroPython image.
    ///
    /// The second-stage bootloader sits at 0x1000 on the original ESP32 and
    /// the S2; every later part places it at 0x0, and the ESP8266 image is
    /// written from 0x0 as well.
    pub fn flash_offset(self) -> &'static str {
        match self {
            Chip::Esp32 | Chip::Esp32S2 => "0x1000",
            _ => "0x0",
        }
    }

    /// The name `esptool --chip` uses for this part. Naming it turns a
    /// mismatched board model into a refusal rather than a half-written flash.
    pub fn esptool_name(self) -> &'static str {
        match self {
            Chip::Esp8266 => "esp8266",
            _ => self.idf_target().unwrap_or("auto"),
        }
    }

    /// The ESP-IDF target name (`idf.py set-target <target>`).
    pub fn idf_target(self) -> Option<&'static str> {
        Some(match self {
            Chip::Esp32 => "esp32",
            Chip::Esp32S2 => "esp32s2",
            Chip::Esp32S3 => "esp32s3",
            Chip::Esp32C3 => "esp32c3",
            Chip::Esp32C6 => "esp32c6",
            Chip::Esp32H2 => "esp32h2",
            _ => return None,
        })
    }

    /// Every chip in the catalog, in catalog order.
    pub fn all() -> Vec<Chip> {
        let mut chips: Vec<Chip> = Vec::new();
        for board in BOARD_CATALOG {
            if !chips.contains(&board.chip) {
                chips.push(board.chip);
            }
        }
        chips
    }
}

// ---------------------------------------------------------------------------
// Firmware flashing
// ---------------------------------------------------------------------------

/// The file a mass-storage bootloader leaves on its volume to identify itself.
pub const UF2_MARKER: &str = "INFO_UF2.TXT";
/// Arm's DAPLink (BBC micro:bit, many Nucleo boards) uses this one instead.
pub const DAPLINK_MARKER: &str = "DETAILS.TXT";

/// How a complete firmware image reaches the board.
///
/// This describes *firmware* — a MicroPython or CircuitPython build, or a
/// user-supplied image. Compiling and uploading a sketch or a native project
/// is the runtime's job, not this one's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashMethod {
    /// `esptool write_flash` over the serial port.
    Esptool,
    /// Copy the image onto a bootloader volume the board mounts as a disk.
    MassStorage {
        /// The volume name prefix, when the board always uses one. `None`
        /// accepts any volume carrying `marker`.
        volume: Option<&'static str>,
        marker: &'static str,
    },
    /// USB DFU (`dfu-util`).
    DfuUtil,
    /// SAM-BA bootloader (`bossac`).
    Bossac,
    /// PJRC's HID bootloader (`teensy_loader_cli`).
    TeensyLoader,
    /// No firmware-flashing path: the board only runs compiled programs.
    Unsupported,
}

/// The UF2 bootloader every RP2040 board exposes.
const UF2_RP2040: FlashMethod = FlashMethod::MassStorage {
    volume: Some("RPI-RP2"),
    marker: UF2_MARKER,
};
/// RP2350 boards (Pico 2) name their boot volume after the new part.
const UF2_RP2350: FlashMethod = FlashMethod::MassStorage {
    volume: Some("RP2350"),
    marker: UF2_MARKER,
};
/// SAMD, nRF52 and friends each name their volume after the board, so match
/// any mounted UF2 bootloader instead of guessing the label.
const UF2_ANY: FlashMethod = FlashMethod::MassStorage {
    volume: None,
    marker: UF2_MARKER,
};
const DAPLINK_MICROBIT: FlashMethod = FlashMethod::MassStorage {
    volume: Some("MICROBIT"),
    marker: DAPLINK_MARKER,
};

impl FlashMethod {
    /// The firmware file extension this method writes.
    pub fn firmware_extension(self) -> &'static str {
        match self {
            FlashMethod::Esptool => ".bin",
            FlashMethod::MassStorage { marker, .. } if marker == DAPLINK_MARKER => ".hex",
            FlashMethod::MassStorage { .. } => ".uf2",
            FlashMethod::DfuUtil | FlashMethod::Bossac => ".bin",
            FlashMethod::TeensyLoader => ".hex",
            FlashMethod::Unsupported => "",
        }
    }

    /// What the user has to do before the flash can start.
    pub fn preparation(self) -> Option<&'static str> {
        match self {
            FlashMethod::Esptool => {
                Some("Most boards reset into the bootloader automatically. If the connection fails, hold BOOT while reconnecting.")
            }
            FlashMethod::MassStorage { marker, .. } if marker == DAPLINK_MARKER => {
                Some("Connect the board over USB; its DAPLink drive mounts on its own.")
            }
            FlashMethod::MassStorage { volume, .. } => Some(match volume {
                Some("RPI-RP2") | Some("RP2350") => {
                    "Hold BOOTSEL while connecting the board, so its boot volume mounts."
                }
                _ => "Double-tap RESET to enter the bootloader, so the board's boot volume mounts.",
            }),
            FlashMethod::DfuUtil => Some("Put the board in DFU mode (BOOT0 high, then reset)."),
            FlashMethod::Bossac => Some("Double-tap RESET to start the SAM-BA bootloader."),
            FlashMethod::TeensyLoader => Some("Press the button on the Teensy when prompted."),
            FlashMethod::Unsupported => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Board definitions
// ---------------------------------------------------------------------------

/// A USB descriptor that points at a board. `pid` is a prefix, so a family
/// sharing a product-id range needs only one entry.
///
/// `shared` separates the two kinds of descriptor that matter for detection:
/// one a single product owns, and one that many boards present because they
/// sit behind the same USB-UART bridge or the same silicon vendor's native
/// USB. Only the first can identify a board; the second narrows it to a chip
/// family and nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbId {
    pub vid: &'static str,
    pub pid: Option<&'static str>,
    pub shared: bool,
}

/// A vendor id with no product id: always shared across that vendor's range.
const fn usb(vid: &'static str) -> UsbId {
    UsbId {
        vid,
        pid: None,
        shared: true,
    }
}

/// A product id this board owns — the only kind that identifies a board.
const fn usb_pid(vid: &'static str, pid: &'static str) -> UsbId {
    UsbId {
        vid,
        pid: Some(pid),
        shared: false,
    }
}

/// A USB-UART bridge or native-USB descriptor many boards present.
const fn bridge(vid: &'static str, pid: &'static str) -> UsbId {
    UsbId {
        vid,
        pid: Some(pid),
        shared: true,
    }
}

/// An `arduino-cli` platform, and the package index it comes from when it is
/// not in the built-in one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArduinoCore {
    pub id: &'static str,
    pub index_url: Option<&'static str>,
}

const CORE_AVR: ArduinoCore = ArduinoCore {
    id: "arduino:avr",
    index_url: None,
};
const CORE_MEGAAVR: ArduinoCore = ArduinoCore {
    id: "arduino:megaavr",
    index_url: None,
};
const CORE_SAM: ArduinoCore = ArduinoCore {
    id: "arduino:sam",
    index_url: None,
};
const CORE_SAMD: ArduinoCore = ArduinoCore {
    id: "arduino:samd",
    index_url: None,
};
const CORE_MBED_NANO: ArduinoCore = ArduinoCore {
    id: "arduino:mbed_nano",
    index_url: None,
};
const CORE_MBED_GIGA: ArduinoCore = ArduinoCore {
    id: "arduino:mbed_giga",
    index_url: None,
};
const CORE_MBED_PORTENTA: ArduinoCore = ArduinoCore {
    id: "arduino:mbed_portenta",
    index_url: None,
};
const CORE_RENESAS: ArduinoCore = ArduinoCore {
    id: "arduino:renesas_uno",
    index_url: None,
};
const CORE_ARDUINO_ESP32: ArduinoCore = ArduinoCore {
    id: "arduino:esp32",
    index_url: None,
};
const ESP32_INDEX: &str =
    "https://raw.githubusercontent.com/espressif/arduino-esp32/gh-pages/package_esp32_index.json";
const CORE_ESP32: ArduinoCore = ArduinoCore {
    id: "esp32:esp32",
    index_url: Some(ESP32_INDEX),
};
const CORE_ESP8266: ArduinoCore = ArduinoCore {
    id: "esp8266:esp8266",
    index_url: Some("https://arduino.esp8266.com/stable/package_esp8266com_index.json"),
};
const CORE_RP2040: ArduinoCore = ArduinoCore {
    id: "rp2040:rp2040",
    index_url: Some(
        "https://github.com/earlephilhower/arduino-pico/releases/download/global/package_rp2040_index.json",
    ),
};
const CORE_ADAFRUIT_SAMD: ArduinoCore = ArduinoCore {
    id: "adafruit:samd",
    index_url: Some("https://adafruit.github.io/arduino-board-index/package_adafruit_index.json"),
};
const CORE_ADAFRUIT_NRF52: ArduinoCore = ArduinoCore {
    id: "adafruit:nrf52",
    index_url: Some("https://adafruit.github.io/arduino-board-index/package_adafruit_index.json"),
};
const CORE_SEEED_SAMD: ArduinoCore = ArduinoCore {
    id: "Seeeduino:samd",
    index_url: Some("https://files.seeedstudio.com/arduino/package_seeeduino_boards_index.json"),
};
const CORE_SEEED_NRF52: ArduinoCore = ArduinoCore {
    id: "Seeeduino:nrf52",
    index_url: Some("https://files.seeedstudio.com/arduino/package_seeeduino_boards_index.json"),
};
const CORE_STM32: ArduinoCore = ArduinoCore {
    id: "STMicroelectronics:stm32",
    index_url: Some(
        "https://github.com/stm32duino/BoardManagerFiles/raw/main/package_stmicroelectronics_index.json",
    ),
};
const CORE_TEENSY: ArduinoCore = ArduinoCore {
    id: "teensy:avr",
    index_url: Some("https://www.pjrc.com/teensy/package_teensy_index.json"),
};

/// One board Anode knows by name.
#[derive(Debug, Clone, Copy)]
pub struct BoardDef {
    pub id: &'static str,
    /// Friendly name shown in the UI.
    pub name: &'static str,
    /// The board's maker, e.g. "M5Stack", "Seeed Studio". Also the group the
    /// board picker files it under.
    pub vendor: &'static str,
    /// Silicon family — the key an ambiguous USB descriptor narrows to.
    pub chip: Chip,
    /// Runtimes this board can execute. The first is the default.
    pub runtimes: &'static [Runtime],
    /// How a firmware image reaches this board.
    pub flash: FlashMethod,
    /// MicroPython firmware variant (the micropython.org download slug).
    pub mpy_variant: Option<&'static str>,
    /// CircuitPython board slug (the circuitpython.org board page).
    pub cpy_board: Option<&'static str>,
    /// Fully-qualified board name for `arduino-cli`.
    pub fqbn: Option<&'static str>,
    /// The `arduino-cli` core that provides `fqbn`.
    pub core: Option<ArduinoCore>,
    /// PlatformIO board id (`board = …` in `platformio.ini`).
    pub pio_board: Option<&'static str>,
    /// True for chip-family fallbacks that are not a specific product.
    pub generic: bool,
    /// USB descriptors that point at this board.
    pub usb: &'static [UsbId],
    /// Anything the user needs to know before flashing this specific board.
    pub note: Option<&'static str>,
}

impl BoardDef {
    pub fn default_runtime(&self) -> Runtime {
        self.runtimes.first().copied().unwrap_or(Runtime::Arduino)
    }

    pub fn supports(&self, runtime: Runtime) -> bool {
        self.runtimes.contains(&runtime)
    }

    /// The ESP-IDF target, when this board's chip has one.
    pub fn idf_target(&self) -> Option<&'static str> {
        self.chip.idf_target()
    }

    /// One line describing the board under its name: chip, architecture and
    /// the runtimes it can run.
    pub fn summary(&self) -> String {
        let runtimes: Vec<&str> = self.runtimes.iter().map(|r| r.label()).collect();
        format!(
            "{} · {} · {}",
            self.chip.label(),
            self.chip.architecture().label(),
            runtimes.join(", ")
        )
    }

    /// Whether `query` matches this board by name, vendor or chip.
    pub fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_lowercase();
        self.name.to_lowercase().contains(&query)
            || self.vendor.to_lowercase().contains(&query)
            || self.chip.label().to_lowercase().contains(&query)
            || self.id.contains(&query)
    }
}

/// Shorthand for the common Espressif board shape, so the catalog entries
/// below stay readable.
const ESP_RUNTIMES: &[Runtime] = &[
    Runtime::MicroPython,
    Runtime::Arduino,
    Runtime::Native,
    Runtime::CircuitPython,
];
const RP2_RUNTIMES: &[Runtime] = &[
    Runtime::MicroPython,
    Runtime::CircuitPython,
    Runtime::Arduino,
    Runtime::Native,
];
const CPY_RUNTIMES: &[Runtime] = &[
    Runtime::CircuitPython,
    Runtime::Arduino,
    Runtime::Native,
    Runtime::MicroPython,
];
const ARDUINO_RUNTIMES: &[Runtime] = &[Runtime::Arduino, Runtime::Native];
const ARDUINO_MPY_RUNTIMES: &[Runtime] = &[Runtime::Arduino, Runtime::Native, Runtime::MicroPython];

/// USB bridges, shared by the many boards that use them. A board matched only
/// by one of these is a chip-family guess, never an exact identification.
const CP210X: UsbId = bridge("10c4", "ea60");
const CH340: UsbId = bridge("1a86", "7523");
const CH9102: UsbId = bridge("1a86", "55d4");
const FTDI: UsbId = bridge("0403", "6001");
/// The ESP32-S3 / C3 / C6 native USB-serial-JTAG descriptor.
const ESP_NATIVE: UsbId = bridge("303a", "1001");

/// The single source of truth for the boards Anode knows by name.
///
/// Ordered by vendor so the picker's groups read in a stable order. The
/// generic per-chip entries come first: they are the fallback a bare USB
/// bridge resolves to.
pub const BOARD_CATALOG: &[BoardDef] = &[
    // -- Generic chip-family fallbacks ------------------------------------
    BoardDef {
        id: "esp32-generic",
        name: "Generic ESP32",
        vendor: "Espressif",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32dev"),
        generic: true,
        usb: &[CP210X, CH340, CH9102, FTDI],
        note: None,
    },
    BoardDef {
        id: "esp32-s2-generic",
        name: "Generic ESP32-S2",
        vendor: "Espressif",
        chip: Chip::Esp32S2,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S2"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32s2"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-s2-saola-1"),
        generic: true,
        usb: &[usb_pid("303a", "0002")],
        note: None,
    },
    BoardDef {
        id: "esp32-s3-generic",
        name: "Generic ESP32-S3",
        vendor: "Espressif",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32s3"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-s3-devkitc-1"),
        generic: true,
        usb: &[ESP_NATIVE, usb("303a")],
        note: None,
    },
    BoardDef {
        id: "esp32-c3-generic",
        name: "Generic ESP32-C3",
        vendor: "Espressif",
        chip: Chip::Esp32C3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32c3"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-c3-devkitm-1"),
        generic: true,
        usb: &[],
        note: None,
    },
    BoardDef {
        id: "esp32-c6-generic",
        name: "Generic ESP32-C6",
        vendor: "Espressif",
        chip: Chip::Esp32C6,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C6"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32c6"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-c6-devkitc-1"),
        generic: true,
        usb: &[],
        note: None,
    },
    BoardDef {
        id: "esp8266-generic",
        name: "Generic ESP8266",
        vendor: "Espressif",
        chip: Chip::Esp8266,
        runtimes: &[Runtime::MicroPython, Runtime::Arduino, Runtime::Native],
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP8266_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp8266:esp8266:generic"),
        core: Some(CORE_ESP8266),
        pio_board: Some("esp01_1m"),
        generic: true,
        usb: &[],
        note: None,
    },
    BoardDef {
        id: "rp2040-generic",
        name: "Generic RP2040",
        vendor: "Raspberry Pi",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("RPI_PICO"),
        cpy_board: None,
        fqbn: Some("rp2040:rp2040:generic"),
        core: Some(CORE_RP2040),
        pio_board: Some("pico"),
        generic: true,
        usb: &[usb("2e8a")],
        note: None,
    },

    // -- Espressif development kits ---------------------------------------
    BoardDef {
        id: "esp32-devkitc",
        name: "ESP32 DevKitC",
        vendor: "Espressif",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32dev"),
        generic: false,
        usb: &[CP210X],
        note: None,
    },
    BoardDef {
        id: "esp32-s2-saola",
        name: "ESP32-S2-Saola-1",
        vendor: "Espressif",
        chip: Chip::Esp32S2,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S2"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32s2"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-s2-saola-1"),
        generic: false,
        usb: &[CP210X],
        note: None,
    },
    BoardDef {
        id: "esp32-s3-devkitc-1",
        name: "ESP32-S3-DevKitC-1",
        vendor: "Espressif",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32s3"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-s3-devkitc-1"),
        generic: false,
        usb: &[ESP_NATIVE, CP210X],
        note: Some("The board has two USB ports. Flash over the UART port if the native port drops during a write."),
    },
    BoardDef {
        id: "esp32-c3-devkitm-1",
        name: "ESP32-C3-DevKitM-1",
        vendor: "Espressif",
        chip: Chip::Esp32C3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32c3"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-c3-devkitm-1"),
        generic: false,
        usb: &[ESP_NATIVE, CP210X],
        note: None,
    },
    BoardDef {
        id: "esp32-c6-devkitc-1",
        name: "ESP32-C6-DevKitC-1",
        vendor: "Espressif",
        chip: Chip::Esp32C6,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C6"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32c6"),
        core: Some(CORE_ESP32),
        pio_board: Some("esp32-c6-devkitc-1"),
        generic: false,
        usb: &[ESP_NATIVE],
        note: None,
    },
    BoardDef {
        id: "esp32-h2-devkitm-1",
        name: "ESP32-H2-DevKitM-1",
        vendor: "Espressif",
        chip: Chip::Esp32H2,
        runtimes: &[Runtime::Native, Runtime::Arduino],
        flash: FlashMethod::Esptool,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32h2"),
        core: Some(CORE_ESP32),
        pio_board: None,
        generic: false,
        usb: &[ESP_NATIVE],
        note: Some("MicroPython does not publish an ESP32-H2 build. Use ESP-IDF or the Arduino core."),
    },

    // -- ESP8266 boards ----------------------------------------------------
    BoardDef {
        id: "nodemcu-v3",
        name: "NodeMCU v3",
        vendor: "Espressif",
        chip: Chip::Esp8266,
        runtimes: &[Runtime::MicroPython, Runtime::Arduino, Runtime::Native],
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP8266_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp8266:esp8266:nodemcuv2"),
        core: Some(CORE_ESP8266),
        pio_board: Some("nodemcuv2"),
        generic: false,
        usb: &[CH340, CP210X],
        note: None,
    },
    BoardDef {
        id: "wemos-d1-mini",
        name: "WEMOS D1 mini",
        vendor: "LOLIN (WEMOS)",
        chip: Chip::Esp8266,
        runtimes: &[Runtime::MicroPython, Runtime::Arduino, Runtime::Native],
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP8266_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp8266:esp8266:d1_mini"),
        core: Some(CORE_ESP8266),
        pio_board: Some("d1_mini"),
        generic: false,
        usb: &[CH340],
        note: None,
    },
    BoardDef {
        id: "lolin-d1-mini-esp32",
        name: "LOLIN D1 mini ESP32",
        vendor: "LOLIN (WEMOS)",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:d1_mini32"),
        core: Some(CORE_ESP32),
        pio_board: Some("wemos_d1_mini32"),
        generic: false,
        usb: &[CP210X, CH340],
        note: None,
    },
    BoardDef {
        id: "lolin-s2-mini",
        name: "LOLIN S2 mini",
        vendor: "LOLIN (WEMOS)",
        chip: Chip::Esp32S2,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S2"),
        cpy_board: Some("lolin_s2_mini"),
        fqbn: Some("esp32:esp32:lolin_s2_mini"),
        core: Some(CORE_ESP32),
        pio_board: Some("lolin_s2_mini"),
        generic: false,
        usb: &[usb("303a")],
        note: None,
    },
    BoardDef {
        id: "lolin-c3-mini",
        name: "LOLIN C3 mini",
        vendor: "LOLIN (WEMOS)",
        chip: Chip::Esp32C3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:lolin_c3_mini"),
        core: Some(CORE_ESP32),
        pio_board: Some("lolin_c3_mini"),
        generic: false,
        usb: &[ESP_NATIVE],
        note: None,
    },

    // -- Raspberry Pi ------------------------------------------------------
    BoardDef {
        id: "rpi-pico",
        name: "Raspberry Pi Pico",
        vendor: "Raspberry Pi",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("RPI_PICO"),
        cpy_board: Some("raspberry_pi_pico"),
        fqbn: Some("rp2040:rp2040:rpipico"),
        core: Some(CORE_RP2040),
        pio_board: Some("pico"),
        generic: false,
        usb: &[usb_pid("2e8a", "0005"), usb_pid("2e8a", "000a"), usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "rpi-pico-w",
        name: "Raspberry Pi Pico W",
        vendor: "Raspberry Pi",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("RPI_PICO_W"),
        cpy_board: Some("raspberry_pi_pico_w"),
        fqbn: Some("rp2040:rp2040:rpipicow"),
        core: Some(CORE_RP2040),
        pio_board: Some("rpipicow"),
        generic: false,
        usb: &[usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "rpi-pico-2",
        name: "Raspberry Pi Pico 2",
        vendor: "Raspberry Pi",
        chip: Chip::Rp2350,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2350,
        mpy_variant: Some("RPI_PICO2"),
        cpy_board: Some("raspberry_pi_pico2"),
        fqbn: Some("rp2040:rp2040:rpipico2"),
        core: Some(CORE_RP2040),
        pio_board: Some("rpipico2"),
        generic: false,
        usb: &[usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "rpi-pico-2w",
        name: "Raspberry Pi Pico 2 W",
        vendor: "Raspberry Pi",
        chip: Chip::Rp2350,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2350,
        mpy_variant: Some("RPI_PICO2_W"),
        cpy_board: Some("raspberry_pi_pico2_w"),
        fqbn: Some("rp2040:rp2040:rpipico2w"),
        core: Some(CORE_RP2040),
        pio_board: Some("rpipico2w"),
        generic: false,
        usb: &[usb("2e8a")],
        note: None,
    },

    // -- Arduino -----------------------------------------------------------
    BoardDef {
        id: "arduino-uno",
        name: "Arduino Uno R3",
        vendor: "Arduino",
        chip: Chip::Atmega328p,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::Unsupported,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:avr:uno"),
        core: Some(CORE_AVR),
        pio_board: Some("uno"),
        generic: false,
        usb: &[usb_pid("2341", "0043"), usb_pid("2341", "0001"), usb("2a03"), CH340],
        note: Some("The ATmega328P has 2 KB of RAM — too little for MicroPython. Compile a sketch or a native project instead."),
    },
    BoardDef {
        id: "arduino-uno-r4-minima",
        name: "Arduino Uno R4 Minima",
        vendor: "Arduino",
        chip: Chip::Ra4m1,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::DfuUtil,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:renesas_uno:minima"),
        core: Some(CORE_RENESAS),
        pio_board: Some("uno_r4_minima"),
        generic: false,
        usb: &[usb_pid("2341", "0069"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-uno-r4-wifi",
        name: "Arduino Uno R4 WiFi",
        vendor: "Arduino",
        chip: Chip::Ra4m1,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::DfuUtil,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:renesas_uno:unor4wifi"),
        core: Some(CORE_RENESAS),
        pio_board: Some("uno_r4_wifi"),
        generic: false,
        usb: &[usb_pid("2341", "1002"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-nano",
        name: "Arduino Nano",
        vendor: "Arduino",
        chip: Chip::Atmega328p,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::Unsupported,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:avr:nano"),
        core: Some(CORE_AVR),
        pio_board: Some("nanoatmega328"),
        generic: false,
        usb: &[CH340, FTDI, usb("2341")],
        note: Some("Clones with the old bootloader need the fqbn arduino:avr:nano:cpu=atmega328old."),
    },
    BoardDef {
        id: "arduino-nano-every",
        name: "Arduino Nano Every",
        vendor: "Arduino",
        chip: Chip::Atmega4809,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::Unsupported,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:megaavr:nona4809"),
        core: Some(CORE_MEGAAVR),
        pio_board: Some("nano_every"),
        generic: false,
        usb: &[usb_pid("2341", "0058"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-nano-33-iot",
        name: "Arduino Nano 33 IoT",
        vendor: "Arduino",
        chip: Chip::Samd21,
        runtimes: &[Runtime::Arduino, Runtime::Native, Runtime::CircuitPython],
        // The Arduino SAMD bootloader is SAM-BA, not UF2.
        flash: FlashMethod::Bossac,
        mpy_variant: None,
        cpy_board: Some("arduino_nano_33_iot"),
        fqbn: Some("arduino:samd:nano_33_iot"),
        core: Some(CORE_SAMD),
        pio_board: Some("nano_33_iot"),
        generic: false,
        usb: &[usb_pid("2341", "8057"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-nano-33-ble",
        name: "Arduino Nano 33 BLE",
        vendor: "Arduino",
        chip: Chip::Nrf52840,
        runtimes: &[Runtime::Arduino, Runtime::Native, Runtime::CircuitPython],
        flash: UF2_ANY,
        mpy_variant: None,
        cpy_board: Some("arduino_nano_33_ble"),
        fqbn: Some("arduino:mbed_nano:nano33ble"),
        core: Some(CORE_MBED_NANO),
        pio_board: Some("nano33ble"),
        generic: false,
        usb: &[usb_pid("2341", "805a"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-nano-rp2040",
        name: "Arduino Nano RP2040 Connect",
        vendor: "Arduino",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("ARDUINO_NANO_RP2040_CONNECT"),
        cpy_board: Some("arduino_nano_rp2040_connect"),
        fqbn: Some("arduino:mbed_nano:nanorp2040connect"),
        core: Some(CORE_MBED_NANO),
        pio_board: Some("nanorp2040connect"),
        generic: false,
        usb: &[usb("2341"), usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "arduino-nano-esp32",
        name: "Arduino Nano ESP32",
        vendor: "Arduino",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ARDUINO_NANO_ESP32"),
        cpy_board: Some("arduino_nano_esp32s3"),
        fqbn: Some("arduino:esp32:nano_nora"),
        core: Some(CORE_ARDUINO_ESP32),
        pio_board: Some("arduino_nano_esp32"),
        generic: false,
        usb: &[usb_pid("2341", "0070"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-mega-2560",
        name: "Arduino Mega 2560",
        vendor: "Arduino",
        chip: Chip::Atmega2560,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::Unsupported,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:avr:mega"),
        core: Some(CORE_AVR),
        pio_board: Some("megaatmega2560"),
        generic: false,
        usb: &[usb_pid("2341", "0042"), usb_pid("2341", "0010"), usb("2341"), CH340],
        note: None,
    },
    BoardDef {
        id: "arduino-leonardo",
        name: "Arduino Leonardo",
        vendor: "Arduino",
        chip: Chip::Atmega32u4,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::Unsupported,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:avr:leonardo"),
        core: Some(CORE_AVR),
        pio_board: Some("leonardo"),
        generic: false,
        usb: &[usb_pid("2341", "8036"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-micro",
        name: "Arduino Micro",
        vendor: "Arduino",
        chip: Chip::Atmega32u4,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::Unsupported,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:avr:micro"),
        core: Some(CORE_AVR),
        pio_board: Some("micro"),
        generic: false,
        usb: &[usb_pid("2341", "8037"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-due",
        name: "Arduino Due",
        vendor: "Arduino",
        chip: Chip::Sam3x8e,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::Bossac,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:sam:arduino_due_x_dbg"),
        core: Some(CORE_SAM),
        pio_board: Some("due"),
        generic: false,
        usb: &[usb_pid("2341", "003d"), usb("2341")],
        note: Some("Upload through the Programming port unless the sketch needs the Native port."),
    },
    BoardDef {
        id: "arduino-mkr-wifi-1010",
        name: "Arduino MKR WiFi 1010",
        vendor: "Arduino",
        chip: Chip::Samd21,
        runtimes: &[Runtime::Arduino, Runtime::Native, Runtime::CircuitPython],
        flash: FlashMethod::Bossac,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("arduino:samd:mkrwifi1010"),
        core: Some(CORE_SAMD),
        pio_board: Some("mkrwifi1010"),
        generic: false,
        usb: &[usb_pid("2341", "8054"), usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-giga-r1",
        name: "Arduino GIGA R1 WiFi",
        vendor: "Arduino",
        chip: Chip::Stm32h7,
        runtimes: ARDUINO_MPY_RUNTIMES,
        flash: FlashMethod::DfuUtil,
        mpy_variant: Some("ARDUINO_GIGA"),
        cpy_board: None,
        fqbn: Some("arduino:mbed_giga:giga"),
        core: Some(CORE_MBED_GIGA),
        pio_board: None,
        generic: false,
        usb: &[usb("2341")],
        note: None,
    },
    BoardDef {
        id: "arduino-portenta-h7",
        name: "Arduino Portenta H7",
        vendor: "Arduino",
        chip: Chip::Stm32h7,
        runtimes: ARDUINO_MPY_RUNTIMES,
        flash: FlashMethod::DfuUtil,
        mpy_variant: Some("ARDUINO_PORTENTA_H7"),
        cpy_board: None,
        fqbn: Some("arduino:mbed_portenta:envie_m7"),
        core: Some(CORE_MBED_PORTENTA),
        pio_board: Some("portenta_h7_m7"),
        generic: false,
        usb: &[usb("2341")],
        note: None,
    },

    // -- M5Stack -----------------------------------------------------------
    BoardDef {
        id: "m5stack-atom-lite",
        name: "M5Stack ATOM Lite",
        vendor: "M5Stack",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:m5stack_atom"),
        core: Some(CORE_ESP32),
        pio_board: Some("m5stack-atom"),
        generic: false,
        usb: &[CP210X],
        note: None,
    },
    BoardDef {
        id: "m5stack-atom-matrix",
        name: "M5Stack ATOM Matrix",
        vendor: "M5Stack",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:m5stack_atom"),
        core: Some(CORE_ESP32),
        pio_board: Some("m5stack-atom"),
        generic: false,
        usb: &[CP210X],
        note: None,
    },
    BoardDef {
        id: "m5stack-atoms3",
        name: "M5Stack AtomS3",
        vendor: "M5Stack",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:m5stack_atoms3"),
        core: Some(CORE_ESP32),
        pio_board: Some("m5stack-atoms3"),
        generic: false,
        usb: &[ESP_NATIVE, usb("303a")],
        note: None,
    },
    BoardDef {
        id: "m5stamp-s3",
        name: "M5Stamp S3",
        vendor: "M5Stack",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:m5stack_stamps3"),
        core: Some(CORE_ESP32),
        pio_board: Some("m5stack-stamps3"),
        generic: false,
        // Enumerates via ESP32-S3 native USB with an 0x80xx product id.
        usb: &[usb_pid("303a", "80")],
        note: None,
    },
    BoardDef {
        id: "m5stack-core2",
        name: "M5Stack Core2",
        vendor: "M5Stack",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:m5stack_core2"),
        core: Some(CORE_ESP32),
        pio_board: Some("m5stack-core2"),
        generic: false,
        usb: &[CP210X, CH9102],
        note: None,
    },
    BoardDef {
        id: "m5stickc-plus",
        name: "M5StickC PLUS",
        vendor: "M5Stack",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:m5stick_c"),
        core: Some(CORE_ESP32),
        pio_board: Some("m5stick-c"),
        generic: false,
        usb: &[CP210X],
        note: None,
    },
    BoardDef {
        id: "m5stack-nanoc6",
        name: "M5Stack NanoC6",
        vendor: "M5Stack",
        chip: Chip::Esp32C6,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C6"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:esp32c6"),
        core: Some(CORE_ESP32),
        pio_board: None,
        generic: false,
        usb: &[ESP_NATIVE],
        note: None,
    },

    // -- Seeed Studio ------------------------------------------------------
    BoardDef {
        id: "xiao-esp32c3",
        name: "XIAO ESP32C3",
        vendor: "Seeed Studio",
        chip: Chip::Esp32C3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("SEEED_XIAO_ESP32C3"),
        cpy_board: Some("seeed_xiao_esp32c3"),
        fqbn: Some("esp32:esp32:XIAO_ESP32C3"),
        core: Some(CORE_ESP32),
        pio_board: Some("seeed_xiao_esp32c3"),
        generic: false,
        usb: &[ESP_NATIVE],
        note: None,
    },
    BoardDef {
        id: "xiao-esp32s3",
        name: "XIAO ESP32S3",
        vendor: "Seeed Studio",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("SEEED_XIAO_ESP32S3"),
        cpy_board: Some("seeed_xiao_esp32s3"),
        fqbn: Some("esp32:esp32:XIAO_ESP32S3"),
        core: Some(CORE_ESP32),
        pio_board: Some("seeed_xiao_esp32s3"),
        generic: false,
        usb: &[ESP_NATIVE, usb("303a")],
        note: None,
    },
    BoardDef {
        id: "xiao-esp32c6",
        name: "XIAO ESP32C6",
        vendor: "Seeed Studio",
        chip: Chip::Esp32C6,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C6"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:XIAO_ESP32C6"),
        core: Some(CORE_ESP32),
        pio_board: None,
        generic: false,
        usb: &[ESP_NATIVE],
        note: None,
    },
    BoardDef {
        id: "xiao-rp2040",
        name: "XIAO RP2040",
        vendor: "Seeed Studio",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("SEEED_XIAO_RP2040"),
        cpy_board: Some("seeeduino_xiao_rp2040"),
        fqbn: Some("rp2040:rp2040:seeed_xiao_rp2040"),
        core: Some(CORE_RP2040),
        pio_board: Some("seeed_xiao_rp2040"),
        generic: false,
        usb: &[usb("2e8a"), usb("2886")],
        note: None,
    },
    BoardDef {
        id: "xiao-samd21",
        name: "XIAO SAMD21",
        vendor: "Seeed Studio",
        chip: Chip::Samd21,
        runtimes: CPY_RUNTIMES,
        flash: UF2_ANY,
        mpy_variant: Some("SEEED_XIAO_SAMD21"),
        cpy_board: Some("seeeduino_xiao"),
        fqbn: Some("Seeeduino:samd:seeed_XIAO_m0"),
        core: Some(CORE_SEEED_SAMD),
        pio_board: Some("seeed_xiao"),
        generic: false,
        usb: &[usb("2886")],
        note: None,
    },
    BoardDef {
        id: "xiao-nrf52840",
        name: "XIAO nRF52840",
        vendor: "Seeed Studio",
        chip: Chip::Nrf52840,
        runtimes: CPY_RUNTIMES,
        flash: UF2_ANY,
        mpy_variant: Some("SEEED_XIAO_NRF52840_SENSE"),
        cpy_board: Some("seeed_xiao_nrf52840_sense"),
        fqbn: Some("Seeeduino:nrf52:xiaonRF52840"),
        core: Some(CORE_SEEED_NRF52),
        pio_board: Some("seeed_xiao_nrf52840_sense"),
        generic: false,
        usb: &[usb("2886")],
        note: None,
    },

    // -- Adafruit ----------------------------------------------------------
    BoardDef {
        id: "adafruit-feather-esp32s3",
        name: "Feather ESP32-S3",
        vendor: "Adafruit",
        chip: Chip::Esp32S3,
        runtimes: CPY_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ADAFRUIT_FEATHER_ESP32S3"),
        cpy_board: Some("adafruit_feather_esp32s3"),
        fqbn: Some("esp32:esp32:adafruit_feather_esp32s3"),
        core: Some(CORE_ESP32),
        pio_board: Some("adafruit_feather_esp32s3"),
        generic: false,
        usb: &[usb("239a"), usb("303a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-feather-esp32s2",
        name: "Feather ESP32-S2",
        vendor: "Adafruit",
        chip: Chip::Esp32S2,
        runtimes: CPY_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S2"),
        cpy_board: Some("adafruit_feather_esp32s2"),
        fqbn: Some("esp32:esp32:adafruit_feather_esp32s2"),
        core: Some(CORE_ESP32),
        pio_board: Some("adafruit_feather_esp32s2"),
        generic: false,
        usb: &[usb("239a"), usb("303a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-qtpy-esp32s3",
        name: "QT Py ESP32-S3",
        vendor: "Adafruit",
        chip: Chip::Esp32S3,
        runtimes: CPY_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: Some("adafruit_qtpy_esp32s3_nopsram"),
        fqbn: Some("esp32:esp32:adafruit_qtpy_esp32s3_nopsram"),
        core: Some(CORE_ESP32),
        pio_board: Some("adafruit_qtpy_esp32s3_nopsram"),
        generic: false,
        usb: &[usb("239a"), usb("303a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-metro-esp32s3",
        name: "Metro ESP32-S3",
        vendor: "Adafruit",
        chip: Chip::Esp32S3,
        runtimes: CPY_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: Some("adafruit_metro_esp32s3"),
        fqbn: Some("esp32:esp32:adafruit_metro_esp32s3"),
        core: Some(CORE_ESP32),
        pio_board: Some("adafruit_metro_esp32s3"),
        generic: false,
        usb: &[usb("239a"), usb("303a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-feather-rp2040",
        name: "Feather RP2040",
        vendor: "Adafruit",
        chip: Chip::Rp2040,
        runtimes: CPY_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("ADAFRUIT_FEATHER_RP2040"),
        cpy_board: Some("adafruit_feather_rp2040"),
        fqbn: Some("rp2040:rp2040:adafruit_feather"),
        core: Some(CORE_RP2040),
        pio_board: Some("adafruit_feather"),
        generic: false,
        usb: &[usb("239a"), usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-qtpy-rp2040",
        name: "QT Py RP2040",
        vendor: "Adafruit",
        chip: Chip::Rp2040,
        runtimes: CPY_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("ADAFRUIT_QTPY_RP2040"),
        cpy_board: Some("adafruit_qtpy_rp2040"),
        fqbn: Some("rp2040:rp2040:adafruit_qtpy"),
        core: Some(CORE_RP2040),
        pio_board: Some("adafruit_qtpy"),
        generic: false,
        usb: &[usb("239a"), usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-itsybitsy-rp2040",
        name: "ItsyBitsy RP2040",
        vendor: "Adafruit",
        chip: Chip::Rp2040,
        runtimes: CPY_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("ADAFRUIT_ITSYBITSY_RP2040"),
        cpy_board: Some("adafruit_itsybitsy_rp2040"),
        fqbn: Some("rp2040:rp2040:adafruit_itsybitsy"),
        core: Some(CORE_RP2040),
        pio_board: Some("adafruit_itsybitsy"),
        generic: false,
        usb: &[usb("239a"), usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-feather-m4",
        name: "Feather M4 Express",
        vendor: "Adafruit",
        chip: Chip::Samd51,
        runtimes: CPY_RUNTIMES,
        flash: UF2_ANY,
        mpy_variant: Some("ADAFRUIT_FEATHER_M4_EXPRESS"),
        cpy_board: Some("feather_m4_express"),
        fqbn: Some("adafruit:samd:adafruit_feather_m4"),
        core: Some(CORE_ADAFRUIT_SAMD),
        pio_board: Some("adafruit_feather_m4"),
        generic: false,
        usb: &[usb("239a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-trinket-m0",
        name: "Trinket M0",
        vendor: "Adafruit",
        chip: Chip::Samd21,
        runtimes: CPY_RUNTIMES,
        flash: UF2_ANY,
        mpy_variant: Some("ADAFRUIT_TRINKET_M0"),
        cpy_board: Some("trinket_m0"),
        fqbn: Some("adafruit:samd:adafruit_trinket_m0"),
        core: Some(CORE_ADAFRUIT_SAMD),
        pio_board: Some("adafruit_trinket_m0"),
        generic: false,
        usb: &[usb("239a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-cpx",
        name: "Circuit Playground Express",
        vendor: "Adafruit",
        chip: Chip::Samd21,
        runtimes: &[Runtime::CircuitPython, Runtime::Arduino, Runtime::Native],
        flash: UF2_ANY,
        mpy_variant: None,
        cpy_board: Some("circuitplayground_express"),
        fqbn: Some("adafruit:samd:adafruit_circuitplayground_m0"),
        core: Some(CORE_ADAFRUIT_SAMD),
        pio_board: Some("adafruit_circuitplayground_m0"),
        generic: false,
        usb: &[usb("239a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-cp-bluefruit",
        name: "Circuit Playground Bluefruit",
        vendor: "Adafruit",
        chip: Chip::Nrf52840,
        runtimes: &[Runtime::CircuitPython, Runtime::Arduino, Runtime::Native],
        flash: UF2_ANY,
        mpy_variant: None,
        cpy_board: Some("circuitplayground_bluefruit"),
        fqbn: Some("adafruit:nrf52:cplaynrf52840"),
        core: Some(CORE_ADAFRUIT_NRF52),
        pio_board: Some("adafruit_cplaynrf52840"),
        generic: false,
        usb: &[usb("239a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-feather-nrf52840",
        name: "Feather nRF52840 Express",
        vendor: "Adafruit",
        chip: Chip::Nrf52840,
        // MicroPython's nRF port does not publish a build for this board.
        runtimes: &[Runtime::CircuitPython, Runtime::Arduino, Runtime::Native],
        flash: UF2_ANY,
        mpy_variant: None,
        cpy_board: Some("feather_nrf52840_express"),
        fqbn: Some("adafruit:nrf52:feather52840"),
        core: Some(CORE_ADAFRUIT_NRF52),
        pio_board: Some("adafruit_feather_nrf52840"),
        generic: false,
        usb: &[usb("239a")],
        note: None,
    },
    BoardDef {
        id: "adafruit-macropad-rp2040",
        name: "MacroPad RP2040",
        vendor: "Adafruit",
        chip: Chip::Rp2040,
        runtimes: CPY_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("RPI_PICO"),
        cpy_board: Some("adafruit_macropad_rp2040"),
        fqbn: Some("rp2040:rp2040:adafruit_macropad2040"),
        core: Some(CORE_RP2040),
        pio_board: Some("adafruit_macropad2040"),
        generic: false,
        usb: &[usb("239a"), usb("2e8a")],
        note: None,
    },

    // -- Unexpected Maker --------------------------------------------------
    BoardDef {
        id: "um-feathers3",
        name: "FeatherS3",
        vendor: "Unexpected Maker",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("UM_FEATHERS3"),
        cpy_board: Some("unexpectedmaker_feathers3"),
        fqbn: Some("esp32:esp32:um_feathers3"),
        core: Some(CORE_ESP32),
        pio_board: Some("um_feathers3"),
        generic: false,
        usb: &[usb("303a")],
        note: None,
    },
    BoardDef {
        id: "um-pros3",
        name: "ProS3",
        vendor: "Unexpected Maker",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("UM_PROS3"),
        cpy_board: Some("unexpectedmaker_pros3"),
        fqbn: Some("esp32:esp32:um_pros3"),
        core: Some(CORE_ESP32),
        pio_board: Some("um_pros3"),
        generic: false,
        usb: &[usb("303a")],
        note: None,
    },
    BoardDef {
        id: "um-tinypico",
        name: "TinyPICO",
        vendor: "Unexpected Maker",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("UM_TINYPICO"),
        cpy_board: Some("unexpectedmaker_tinypico"),
        fqbn: Some("esp32:esp32:tinypico"),
        core: Some(CORE_ESP32),
        pio_board: Some("tinypico"),
        generic: false,
        usb: &[CP210X],
        note: None,
    },

    // -- LilyGO ------------------------------------------------------------
    BoardDef {
        id: "lilygo-t-display-s3",
        name: "T-Display S3",
        vendor: "LilyGO",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: Some("lilygo_tdisplay_s3"),
        fqbn: Some("esp32:esp32:lilygo_t_display_s3"),
        core: Some(CORE_ESP32),
        pio_board: Some("lilygo-t-display-s3"),
        generic: false,
        usb: &[ESP_NATIVE, usb("303a")],
        note: None,
    },
    BoardDef {
        id: "lilygo-ttgo-lora32",
        name: "TTGO LoRa32",
        vendor: "LilyGO",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:ttgo-lora32"),
        core: Some(CORE_ESP32),
        pio_board: Some("ttgo-lora32-v21"),
        generic: false,
        usb: &[CP210X, CH9102],
        note: None,
    },
    BoardDef {
        id: "lilygo-t-beam",
        name: "T-Beam",
        vendor: "LilyGO",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:t-beam"),
        core: Some(CORE_ESP32),
        pio_board: Some("ttgo-t-beam"),
        generic: false,
        usb: &[CP210X, CH9102],
        note: None,
    },

    // -- Heltec ------------------------------------------------------------
    BoardDef {
        id: "heltec-wifi-lora32-v3",
        name: "WiFi LoRa 32 V3",
        vendor: "Heltec",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:heltec_wifi_lora_32_V3"),
        core: Some(CORE_ESP32),
        pio_board: Some("heltec_wifi_lora_32_V3"),
        generic: false,
        usb: &[CP210X, ESP_NATIVE],
        note: None,
    },

    // -- SparkFun ----------------------------------------------------------
    BoardDef {
        id: "sparkfun-thing-plus-esp32s2",
        name: "Thing Plus ESP32-S2 WROOM",
        vendor: "SparkFun",
        chip: Chip::Esp32S2,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S2"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:sparkfun_esp32s2_thing_plus"),
        core: Some(CORE_ESP32),
        pio_board: Some("sparkfun_esp32s2_thing_plus"),
        generic: false,
        usb: &[usb("1b4f"), usb("303a")],
        note: None,
    },
    BoardDef {
        id: "sparkfun-promicro-rp2040",
        name: "Pro Micro RP2040",
        vendor: "SparkFun",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("RPI_PICO"),
        cpy_board: Some("sparkfun_pro_micro_rp2040"),
        fqbn: Some("rp2040:rp2040:sparkfun_promicrorp2040"),
        core: Some(CORE_RP2040),
        pio_board: Some("sparkfun_promicrorp2040"),
        generic: false,
        usb: &[usb("1b4f"), usb("2e8a")],
        note: None,
    },

    // -- Waveshare / Pimoroni ---------------------------------------------
    BoardDef {
        id: "waveshare-rp2040-zero",
        name: "RP2040-Zero",
        vendor: "Waveshare",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("RPI_PICO"),
        cpy_board: Some("waveshare_rp2040_zero"),
        fqbn: Some("rp2040:rp2040:waveshare_rp2040_zero"),
        core: Some(CORE_RP2040),
        pio_board: Some("waveshare_rp2040_zero"),
        generic: false,
        usb: &[usb("2e8a")],
        note: None,
    },
    BoardDef {
        id: "waveshare-esp32s3-zero",
        name: "ESP32-S3-Zero",
        vendor: "Waveshare",
        chip: Chip::Esp32S3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_S3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:waveshare_esp32_s3_zero"),
        core: Some(CORE_ESP32),
        pio_board: None,
        generic: false,
        usb: &[ESP_NATIVE, usb("303a")],
        note: None,
    },
    BoardDef {
        id: "pimoroni-tiny2040",
        name: "Tiny 2040",
        vendor: "Pimoroni",
        chip: Chip::Rp2040,
        runtimes: RP2_RUNTIMES,
        flash: UF2_RP2040,
        mpy_variant: Some("RPI_PICO"),
        cpy_board: Some("pimoroni_tiny2040"),
        fqbn: Some("rp2040:rp2040:pimoroni_tiny2040"),
        core: Some(CORE_RP2040),
        pio_board: Some("pimoroni_tiny2040"),
        generic: false,
        usb: &[usb("2e8a")],
        note: None,
    },

    // -- DFRobot -----------------------------------------------------------
    BoardDef {
        id: "dfrobot-firebeetle-esp32",
        name: "FireBeetle ESP32",
        vendor: "DFRobot",
        chip: Chip::Esp32,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:firebeetle32"),
        core: Some(CORE_ESP32),
        pio_board: Some("firebeetle32"),
        generic: false,
        usb: &[CP210X, CH340],
        note: None,
    },
    BoardDef {
        id: "dfrobot-beetle-esp32c3",
        name: "Beetle ESP32-C3",
        vendor: "DFRobot",
        chip: Chip::Esp32C3,
        runtimes: ESP_RUNTIMES,
        flash: FlashMethod::Esptool,
        mpy_variant: Some("ESP32_GENERIC_C3"),
        cpy_board: None,
        fqbn: Some("esp32:esp32:dfrobot_beetle_esp32c3"),
        core: Some(CORE_ESP32),
        pio_board: Some("dfrobot_beetle_esp32c3"),
        generic: false,
        usb: &[ESP_NATIVE],
        note: None,
    },

    // -- BBC / education ---------------------------------------------------
    BoardDef {
        id: "microbit-v2",
        name: "BBC micro:bit v2",
        vendor: "BBC",
        chip: Chip::Nrf52840,
        runtimes: &[Runtime::CircuitPython, Runtime::Arduino, Runtime::Native],
        flash: DAPLINK_MICROBIT,
        mpy_variant: None,
        cpy_board: Some("microbit_v2"),
        fqbn: None,
        core: None,
        pio_board: Some("bbcmicrobit_v2"),
        generic: false,
        usb: &[usb_pid("0d28", "0204")],
        note: Some("MicroPython for micro:bit is a separate build from microbit.org, not micropython.org."),
    },
    BoardDef {
        id: "microbit-v1",
        name: "BBC micro:bit v1",
        vendor: "BBC",
        chip: Chip::Nrf51822,
        runtimes: &[Runtime::Native],
        flash: DAPLINK_MICROBIT,
        mpy_variant: None,
        cpy_board: None,
        fqbn: None,
        core: None,
        pio_board: Some("bbcmicrobit"),
        generic: false,
        usb: &[usb_pid("0d28", "0204")],
        note: Some("MicroPython for micro:bit is a separate build from microbit.org, not micropython.org."),
    },

    // -- PJRC Teensy -------------------------------------------------------
    BoardDef {
        id: "teensy-40",
        name: "Teensy 4.0",
        vendor: "PJRC",
        chip: Chip::Imxrt1062,
        runtimes: ARDUINO_MPY_RUNTIMES,
        flash: FlashMethod::TeensyLoader,
        mpy_variant: Some("TEENSY40"),
        cpy_board: None,
        fqbn: Some("teensy:avr:teensy40"),
        core: Some(CORE_TEENSY),
        pio_board: Some("teensy40"),
        generic: false,
        usb: &[usb("16c0")],
        note: Some("Uploading needs Teensyduino or teensy_loader_cli; the board has no serial bootloader."),
    },
    BoardDef {
        id: "teensy-41",
        name: "Teensy 4.1",
        vendor: "PJRC",
        chip: Chip::Imxrt1062,
        runtimes: ARDUINO_MPY_RUNTIMES,
        flash: FlashMethod::TeensyLoader,
        mpy_variant: Some("TEENSY41"),
        cpy_board: None,
        fqbn: Some("teensy:avr:teensy41"),
        core: Some(CORE_TEENSY),
        pio_board: Some("teensy41"),
        generic: false,
        usb: &[usb("16c0")],
        note: Some("Uploading needs Teensyduino or teensy_loader_cli; the board has no serial bootloader."),
    },
    BoardDef {
        id: "teensy-32",
        name: "Teensy 3.2",
        vendor: "PJRC",
        chip: Chip::Mk20dx256,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::TeensyLoader,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("teensy:avr:teensy31"),
        core: Some(CORE_TEENSY),
        pio_board: Some("teensy31"),
        generic: false,
        usb: &[usb("16c0")],
        note: None,
    },

    // -- STMicroelectronics ------------------------------------------------
    BoardDef {
        id: "stm32-blue-pill",
        name: "Blue Pill (STM32F103C8)",
        vendor: "STMicroelectronics",
        chip: Chip::Stm32f1,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::DfuUtil,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("STMicroelectronics:stm32:GenF1:pnum=BLUEPILL_F103C8"),
        core: Some(CORE_STM32),
        pio_board: Some("bluepill_f103c8"),
        generic: false,
        usb: &[usb_pid("0483", "df11"), usb("0483")],
        note: Some("Without a USB bootloader the board needs an ST-Link or a USB-UART adapter on BOOT0."),
    },
    BoardDef {
        id: "stm32-black-pill-f411",
        name: "Black Pill (STM32F411CE)",
        vendor: "STMicroelectronics",
        chip: Chip::Stm32f4,
        runtimes: ARDUINO_RUNTIMES,
        flash: FlashMethod::DfuUtil,
        mpy_variant: None,
        cpy_board: None,
        fqbn: Some("STMicroelectronics:stm32:GenF4:pnum=BLACKPILL_F411CE"),
        core: Some(CORE_STM32),
        pio_board: Some("blackpill_f411ce"),
        generic: false,
        usb: &[usb_pid("0483", "df11"), usb("0483")],
        note: None,
    },
    BoardDef {
        id: "stm32-nucleo-f401re",
        name: "NUCLEO-F401RE",
        vendor: "STMicroelectronics",
        chip: Chip::Stm32f4,
        runtimes: ARDUINO_MPY_RUNTIMES,
        flash: FlashMethod::MassStorage {
            volume: Some("NODE_F401RE"),
            marker: DAPLINK_MARKER,
        },
        mpy_variant: Some("NUCLEO_F401RE"),
        cpy_board: None,
        fqbn: Some("STMicroelectronics:stm32:Nucleo_64:pnum=NUCLEO_F401RE"),
        core: Some(CORE_STM32),
        pio_board: Some("nucleo_f401re"),
        generic: false,
        usb: &[usb_pid("0483", "374b"), usb("0483")],
        note: None,
    },
];

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

pub fn board_by_id(id: &str) -> Option<&'static BoardDef> {
    BOARD_CATALOG.iter().find(|board| board.id == id)
}

/// Every vendor in the catalog, in first-appearance order.
pub fn vendors() -> Vec<&'static str> {
    let mut vendors: Vec<&'static str> = Vec::new();
    for board in BOARD_CATALOG {
        if !vendors.contains(&board.vendor) {
            vendors.push(board.vendor);
        }
    }
    vendors
}

/// The named (non-generic) boards built on `chip`, followed by the chip's
/// generic fallback. This is the candidate list for an ambiguous descriptor.
pub fn boards_for_chip(chip: Chip) -> Vec<&'static BoardDef> {
    let mut named: Vec<&'static BoardDef> = BOARD_CATALOG
        .iter()
        .filter(|board| board.chip == chip && !board.generic)
        .collect();
    named.extend(
        BOARD_CATALOG
            .iter()
            .filter(|board| board.chip == chip && board.generic),
    );
    named
}

/// Boards that can run `runtime`.
pub fn boards_for_runtime(runtime: Runtime) -> Vec<&'static BoardDef> {
    BOARD_CATALOG
        .iter()
        .filter(|board| board.supports(runtime))
        .collect()
}

// ---------------------------------------------------------------------------
// USB identity and detection
// ---------------------------------------------------------------------------

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
        .map(|serial| serial.trim().to_string())
        .filter(|serial| !serial.is_empty())
        .unwrap_or_else(|| port.path.clone())
}

/// Vendors whose USB-serial bridges front boards from every silicon vendor.
///
/// A port behind one of these says nothing about the chip on the other side:
/// the same CH343 descriptor fronts an ESP32, an ESP32-S3 and plenty of boards
/// that are not Espressif at all. Only the chip itself can settle it, so this
/// list marks the ports worth asking rather than guessing about.
const BRIDGE_VENDORS: &[&str] = &[
    // Silicon Labs CP210x
    "10c4", // WCH CH340, CH9102, CH343
    "1a86", // FTDI
    "0403", // Prolific PL2303
    "067b",
];

/// True when this port is a USB-UART bridge rather than a chip's own USB.
pub fn is_usb_uart_bridge(port: &PortIdentity) -> bool {
    port.vendor_id.as_deref().is_some_and(|vid| {
        BRIDGE_VENDORS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(vid))
    })
}

#[derive(Debug, Clone, Default)]
pub struct BoardMatch {
    /// Best resolved board (override, then auto-detect, then none).
    pub board: Option<&'static BoardDef>,
    /// True when a unique, board-specific descriptor identified it.
    pub exact: bool,
    /// True when the resolution came from a saved user override.
    pub from_override: bool,
    /// True when the chip answered for itself rather than being inferred from
    /// a USB descriptor.
    pub probed: bool,
    /// Board ids the user can pick from to refine an ambiguous match.
    pub candidates: Vec<&'static str>,
}

impl BoardMatch {
    /// How confident the resolution is, as one short phrase for the UI.
    pub fn confidence(&self) -> &'static str {
        if self.board.is_none() {
            "Unidentified"
        } else if self.from_override {
            "Pinned"
        } else if self.exact {
            "Identified"
        } else if self.probed {
            "Chip detected"
        } else {
            "Best guess"
        }
    }

    /// Replace a descriptor-based guess with what the silicon itself
    /// reported.
    ///
    /// A pinned board is the user's decision and is never overruled. Anything
    /// else gives way: a USB bridge can only ever name a family, and it names
    /// the wrong one as often as the right one, so a chip that answered for
    /// itself wins — keeping the current entry when it is already a board
    /// built on that chip, and falling back to the chip's generic entry when
    /// it is not.
    pub fn refined_by_chip(mut self, chip: Chip) -> Self {
        if self.from_override {
            return self;
        }
        self.probed = true;
        if self.board.is_some_and(|board| board.chip == chip) {
            return self;
        }
        self.board = BOARD_CATALOG
            .iter()
            .find(|board| board.chip == chip && board.generic)
            .or_else(|| BOARD_CATALOG.iter().find(|board| board.chip == chip));
        self.exact = false;
        self.candidates = boards_for_chip(chip)
            .into_iter()
            .map(|board| board.id)
            .collect();
        self
    }
}

/// The score below which a match only narrows the chip family.
const FAMILY_ONLY: i32 = 1;

/// Score how specifically a catalog entry matches a port. Returns -1 for no
/// match, [`FAMILY_ONLY`] for a shared bridge or vendor id, and a higher
/// score for a product id this board owns — a longer product id being the
/// more distinguishing one.
fn score_match(board: &BoardDef, port: &PortIdentity) -> i32 {
    let Some(port_vid) = &port.vendor_id else {
        return -1;
    };
    let port_pid = port
        .product_id
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut best = -1;
    for id in board.usb {
        if !id.vid.eq_ignore_ascii_case(port_vid) {
            continue;
        }
        if let Some(prefix) = id.pid {
            if !port_pid.starts_with(&prefix.to_ascii_lowercase()) {
                continue;
            }
        }
        let score = if id.shared {
            FAMILY_ONLY
        } else {
            // Unreachable for a vid-only entry: `usb` always sets `shared`.
            2 + id.pid.map_or(0, str::len) as i32
        };
        best = best.max(score);
    }
    best
}

/// True when the port's vendor id belongs to a board or USB bridge the
/// catalog knows — the cheap "is this plausibly hardware" test.
pub fn is_known_vendor(port: &PortIdentity) -> bool {
    let Some(vid) = &port.vendor_id else {
        return false;
    };
    BOARD_CATALOG
        .iter()
        .any(|board| board.usb.iter().any(|id| id.vid.eq_ignore_ascii_case(vid)))
}

/// Resolve which board is on a port. A saved override always wins. Otherwise
/// the highest-scoring catalog match is used; when that match is only a
/// chip-family fallback (generic), `exact` is false and `candidates` lists the
/// boards in that family so the UI can offer a picker.
pub fn identify_board(
    port: Option<&PortIdentity>,
    overrides: &HashMap<String, String>,
) -> BoardMatch {
    let Some(port) = port else {
        return BoardMatch::default();
    };

    let mut best: Option<&'static BoardDef> = None;
    let mut best_score = -1;
    for board in BOARD_CATALOG {
        let score = score_match(board, port);
        if score < 0 {
            continue;
        }
        // On a tie the answer depends on what tied. A shared bridge says only
        // which family this is, so the family's own entry is the honest
        // answer; anything stronger has already named a product.
        let wins = score > best_score
            || (score == best_score
                && best.is_some_and(|current| current.generic != board.generic)
                && (board.generic == (score <= FAMILY_ONLY)));
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
            boards_for_chip(resolved.chip)
                .into_iter()
                .map(|board| board.id)
                .collect()
        })
        .unwrap_or_default();

    BoardMatch {
        // Only a product id this board owns identifies it. A shared bridge or
        // vendor id narrows the family and stops there.
        exact: from_override.is_none()
            && best_score > FAMILY_ONLY
            && best.is_some_and(|board| !board.generic),
        from_override: from_override.is_some(),
        probed: false,
        board: resolved,
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
    fn catalog_ids_are_unique() {
        let mut ids: Vec<&str> = BOARD_CATALOG.iter().map(|board| board.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate board id in the catalog");
    }

    #[test]
    fn every_chip_has_exactly_one_generic_fallback() {
        for chip in Chip::all() {
            let generics = BOARD_CATALOG
                .iter()
                .filter(|board| board.chip == chip && board.generic)
                .count();
            assert!(
                generics <= 1,
                "{} has {generics} generic entries",
                chip.label()
            );
        }
    }

    #[test]
    fn micropython_boards_name_a_firmware_variant() {
        for board in BOARD_CATALOG {
            if board.supports(Runtime::MicroPython) {
                assert!(
                    board.mpy_variant.is_some(),
                    "{} runs MicroPython but names no firmware variant",
                    board.id
                );
            }
        }
    }

    #[test]
    fn arduino_boards_name_a_core_for_their_fqbn() {
        for board in BOARD_CATALOG {
            if board.fqbn.is_some() {
                assert!(
                    board.core.is_some(),
                    "{} has an fqbn but no core to install",
                    board.id
                );
            }
        }
    }

    #[test]
    fn espressif_chips_flash_with_esptool() {
        for board in BOARD_CATALOG {
            if board.chip.is_espressif() {
                assert_eq!(
                    board.flash,
                    FlashMethod::Esptool,
                    "{} is Espressif but does not flash with esptool",
                    board.id
                );
            }
        }
    }

    #[test]
    fn flash_offsets_follow_the_bootloader_layout() {
        assert_eq!(Chip::Esp32.flash_offset(), "0x1000");
        assert_eq!(Chip::Esp32S2.flash_offset(), "0x1000");
        assert_eq!(Chip::Esp32S3.flash_offset(), "0x0");
        assert_eq!(Chip::Esp32C3.flash_offset(), "0x0");
        assert_eq!(Chip::Esp32C6.flash_offset(), "0x0");
        assert_eq!(Chip::Esp8266.flash_offset(), "0x0");
    }

    #[test]
    fn detects_a_board_by_its_product_id() {
        let m = identify_board(Some(&port("2341", "0043")), &Default::default());
        assert_eq!(m.board.unwrap().id, "arduino-uno");
        assert!(m.exact);
    }

    #[test]
    fn esp32s3_native_usb_is_a_family_guess_with_candidates() {
        let m = identify_board(Some(&port("303a", "1001")), &Default::default());
        let board = m.board.unwrap();
        assert_eq!(board.chip, Chip::Esp32S3);
        assert!(!m.exact, "a shared native-USB descriptor is never exact");
        assert!(
            m.candidates.len() > 3,
            "the S3 family should offer a choice"
        );
        assert!(m.candidates.contains(&"esp32-s3-generic"));
    }

    #[test]
    fn a_bare_cp210x_bridge_resolves_to_the_generic_esp32() {
        let m = identify_board(Some(&port("10c4", "ea60")), &Default::default());
        // Several boards use this bridge, so the family fallback has to win
        // rather than whichever product happens to come first in the catalog.
        assert_eq!(m.board.unwrap().chip, Chip::Esp32);
        assert!(!m.exact);
    }

    #[test]
    fn m5stamp_s3_wins_on_its_product_id_prefix() {
        let m = identify_board(Some(&port("303a", "8001")), &Default::default());
        assert_eq!(m.board.unwrap().id, "m5stamp-s3");
    }

    #[test]
    fn override_wins_and_pins_the_board() {
        let mut overrides = HashMap::new();
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
        assert_eq!(m.confidence(), "Pinned");
    }

    #[test]
    fn unknown_vid_matches_nothing() {
        let m = identify_board(Some(&port("dead", "beef")), &Default::default());
        assert!(m.board.is_none());
        assert!(m.candidates.is_empty());
        assert_eq!(m.confidence(), "Unidentified");
    }

    #[test]
    fn no_port_has_no_board() {
        assert!(identify_board(None, &Default::default()).board.is_none());
    }

    #[test]
    fn query_matches_name_vendor_and_chip() {
        let xiao = board_by_id("xiao-esp32s3").unwrap();
        assert!(xiao.matches_query("seeed"));
        assert!(xiao.matches_query("esp32-s3"));
        assert!(xiao.matches_query("XIAO"));
        assert!(!xiao.matches_query("teensy"));
    }
}
