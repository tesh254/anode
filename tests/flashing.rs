//! Flashing an Espressif board without asking the user anything.
//!
//! These are written around one real board — the ESP32-S3 devkit this feature
//! was built against — because the whole point is that its descriptor is
//! useless and its chip is not. The USB identity, the `esptool` output and the
//! firmware page layout below are all captured from that board, so a change
//! that breaks the chain from "plugged in" to "the right image, written with
//! the right settings" fails here with no hardware attached.

use anode::boards::{self, Chip, PortIdentity};
use anode::device::{FirmwareSource, FlashRequest};
use anode::esp;

/// The board on the bench: an ESP32-S3 devkit reached through a WCH CH343
/// USB-UART bridge. No part of this descriptor names the chip.
fn bench_board_port() -> PortIdentity {
    PortIdentity {
        path: "/dev/cu.usbmodem5B5E1061311".into(),
        vendor_id: Some("1a86".into()),
        product_id: Some("55d3".into()),
        manufacturer: None,
        serial_number: Some("5B5E106131".into()),
    }
}

/// What `esptool flash-id` actually printed for that board.
const BENCH_BOARD_ESPTOOL_OUTPUT: &str = "\
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
Running stub flasher...
Stub flasher running.
Changing baud rate to 460800...
Changed.

Flash Memory Information:
=========================
Manufacturer: 68
Device: 4018
Detected flash size: 16MB
Flash type set in eFuse: quad (4 data lines)
Flash voltage set by eFuse: 3.3V

Hard resetting via RTS pin...
";

/// The descriptor alone is a dead end, and Anode has to know that rather than
/// guess a family from the bridge's vendor.
#[test]
fn the_bench_board_cannot_be_identified_from_usb_alone() {
    let port = bench_board_port();
    let matched = boards::identify_board(Some(&port), &Default::default());
    assert!(
        matched.board.is_none(),
        "a CH343 fronts boards from every vendor, so it must not resolve to one"
    );
    assert_eq!(matched.confidence(), "Unidentified");
    assert!(
        boards::is_usb_uart_bridge(&port),
        "but it is recognisable as a bridge, which is what makes it worth asking the chip"
    );
    assert!(
        !esp::is_native_usb(&port),
        "and it is not the chip's own USB, so its baud rate is real"
    );
}

/// Asking the chip is what turns that dead end into a usable board.
#[test]
fn asking_the_chip_resolves_the_bench_board() {
    let port = bench_board_port();
    let report = esp::parse_report(BENCH_BOARD_ESPTOOL_OUTPUT);
    assert_eq!(report.chip, Some(Chip::Esp32S3));

    let matched = boards::identify_board(Some(&port), &Default::default())
        .refined_by_chip(report.chip.unwrap());
    let board = matched.board.expect("the chip's family has a board");
    assert_eq!(board.chip, Chip::Esp32S3);
    assert_eq!(board.id, "esp32-s3-generic");
    assert_eq!(
        matched.confidence(),
        "Chip detected",
        "the chip is known; which S3 board it is still is not"
    );
    assert!(
        matched.candidates.contains(&"esp32-s3-generic"),
        "the S3 boards stay offered so the user can narrow it"
    );
}

/// A board the user pinned is their decision, and a probe does not overrule
/// it — not even when the chip disagrees.
#[test]
fn a_pinned_board_outranks_the_probe() {
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("5B5E106131".to_string(), "xiao-esp32s3".to_string());
    let matched =
        boards::identify_board(Some(&bench_board_port()), &overrides).refined_by_chip(Chip::Esp32);
    assert_eq!(matched.board.unwrap().id, "xiao-esp32s3");
    assert_eq!(matched.confidence(), "Pinned");
}

/// A bridge that resolved to the wrong family is the case that silently
/// writes the wrong image, so the probe has to win over the descriptor.
#[test]
fn a_probe_corrects_a_family_the_descriptor_got_wrong() {
    let cp210x = PortIdentity {
        path: "/dev/cu.SLAB_USBtoUART".into(),
        vendor_id: Some("10c4".into()),
        product_id: Some("ea60".into()),
        ..Default::default()
    };
    let guessed = boards::identify_board(Some(&cp210x), &Default::default());
    assert_eq!(
        guessed.board.unwrap().chip,
        Chip::Esp32,
        "the bridge's best guess is the plain ESP32"
    );

    let corrected = guessed.refined_by_chip(Chip::Esp32C6);
    assert_eq!(corrected.board.unwrap().chip, Chip::Esp32C6);
    assert!(!corrected.exact);
}

/// The settings a flash needs, end to end, from the descriptor and the chip's
/// own report — none of them asked for.
#[test]
fn the_bench_board_produces_a_complete_flash_plan() {
    let port = bench_board_port();
    let report = esp::parse_report(BENCH_BOARD_ESPTOOL_OUTPUT);
    let board = boards::identify_board(Some(&port), &Default::default())
        .refined_by_chip(report.chip.unwrap())
        .board
        .unwrap();

    let request = FlashRequest {
        port_path: port.path.clone(),
        board,
        source: FirmwareSource::MicroPython,
        report: Some(report),
        native_usb: esp::is_native_usb(&port),
    };

    assert_eq!(
        request.image_label(),
        "ESP32_GENERIC_S3-SPIRAM_OCT",
        "8 MB of octal PSRAM wants the PSRAM build, not the plain one"
    );

    let plan = request.plan().expect("an esptool board has a plan");
    assert_eq!(plan.chip, Chip::Esp32S3);
    assert_eq!(plan.offset, "0x0", "the S3 bootloader sits at 0x0");
    assert_eq!(
        plan.baud,
        esp::BRIDGE_BAUD,
        "a real UART bridge gets the conservative rate"
    );
    assert!(
        plan.erase_first,
        "a stale filesystem is the usual reason a reflashed board will not boot"
    );
}

/// The chip the silicon reported beats the catalog entry's assumption, so a
/// stale board model cannot send the wrong `--chip` to esptool.
#[test]
fn the_reported_chip_beats_the_board_models_assumption() {
    let request = FlashRequest {
        port_path: "/dev/cu.usbmodem1".into(),
        board: boards::board_by_id("esp32-generic").unwrap(),
        source: FirmwareSource::MicroPython,
        report: Some(esp::parse_report(BENCH_BOARD_ESPTOOL_OUTPUT)),
        native_usb: false,
    };
    let plan = request.plan().unwrap();
    assert_eq!(plan.chip, Chip::Esp32S3);
    assert_eq!(
        plan.offset, "0x0",
        "and the offset follows the real chip, not the ESP32's 0x1000"
    );
}

/// A board on its own USB peripheral is a different connection with different
/// settings, and nothing about that is asked either.
#[test]
fn a_native_usb_board_gets_the_faster_link() {
    let port = PortIdentity {
        path: "/dev/cu.usbmodem101".into(),
        vendor_id: Some("303a".into()),
        product_id: Some("1001".into()),
        ..Default::default()
    };
    assert!(esp::is_native_usb(&port));
    let plan = esp::FlashPlan::full_image(Chip::Esp32S3, true);
    assert_eq!(plan.baud, esp::NATIVE_USB_BAUD);
    assert_eq!(plan.slowed().baud, esp::SLOW_BAUD);
}

/// Every failure a user actually hits has to say what happened and what to do,
/// rather than repeating a Python traceback.
#[test]
fn each_common_failure_names_its_cause_and_a_next_step() {
    let cases: &[(&str, esp::FailureKind, &str)] = &[
        (
            "serial.serialutil.SerialException: could not open port /dev/cu.usbmodem1: \
             [Errno 16] Resource busy: '/dev/cu.usbmodem1'",
            esp::FailureKind::PortBusy,
            "serial monitor",
        ),
        (
            "A fatal error occurred: Failed to connect to ESP32-S3: No serial data received.",
            esp::FailureKind::NoAnswer,
            "BOOT",
        ),
        (
            "A fatal error occurred: This chip is ESP32-C3, not ESP32-S3. Wrong --chip argument?",
            esp::FailureKind::WrongChip,
            "board model",
        ),
        (
            "A fatal error occurred: Timed out waiting for packet header",
            esp::FailureKind::LinkUnstable,
            "cable",
        ),
        (
            "SerialException: [Errno 13] Permission denied: '/dev/cu.usbmodem1'",
            esp::FailureKind::Denied,
            "serial devices",
        ),
    ];
    for (tail, expected, expected_advice) in cases {
        let diagnosis = esp::diagnose(tail, Some(Chip::Esp32S3));
        assert_eq!(diagnosis.kind, *expected, "misread: {tail}");
        assert!(
            diagnosis.next.is_some(),
            "a known failure must say what to do next: {tail}"
        );
        assert!(
            diagnosis.message().contains(expected_advice),
            "the advice for {expected:?} should mention {expected_advice}: {}",
            diagnosis.message()
        );
        assert!(
            !diagnosis.message().contains("Traceback"),
            "a traceback is not an explanation"
        );
    }
}

/// Every Espressif board in the catalog has to survive the derivation, so a
/// new entry cannot quietly produce a flash with no settings.
#[test]
fn every_espressif_board_derives_a_usable_plan() {
    for board in boards::BOARD_CATALOG
        .iter()
        .filter(|board| board.chip.is_espressif())
    {
        let request = FlashRequest {
            port_path: "/dev/cu.test".into(),
            board,
            source: FirmwareSource::MicroPython,
            report: None,
            native_usb: false,
        };
        let plan = request
            .plan()
            .unwrap_or_else(|| panic!("{} has no flash plan", board.id));
        assert_eq!(plan.chip, board.chip);
        assert!(
            plan.offset.starts_with("0x"),
            "{} has no write offset",
            board.id
        );
        assert!(plan.baud >= esp::SLOW_BAUD, "{} has no speed", board.id);
        assert!(
            !plan.chip.esptool_name().is_empty() && plan.chip.esptool_name() != "auto",
            "{} would ask esptool to guess its chip",
            board.id
        );
        if board.mpy_variant.is_some() {
            assert!(
                !request.image_label().is_empty(),
                "{} names no firmware image",
                board.id
            );
        }
    }
}

/// The variant decides which file to take off a page that lists several, so
/// the two builds must not be interchangeable.
#[test]
fn the_psram_build_and_the_plain_build_are_told_apart() {
    let generic = boards::board_by_id("esp32-s3-generic").unwrap();
    let plain = esp::micropython_variant(generic, None).unwrap();
    let psram = esp::micropython_variant(
        generic,
        Some(&esp::ChipReport {
            psram: Some(esp::Psram {
                megabytes: 8,
                octal: true,
            }),
            ..Default::default()
        }),
    )
    .unwrap();

    assert_ne!(plain, psram);
    assert_eq!(
        esp::micropython_download_page(&psram),
        esp::micropython_download_page(&plain),
        "both builds are listed on the one page"
    );
    // Which is exactly why the file name, not the page, has to be matched.
    assert!(psram.starts_with(&plain));
}
