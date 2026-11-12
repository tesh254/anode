# Anode

A native hardware programming IDE built with Rust and GPUI Kit. Edit a project,
detect the board on the USB port, pick the runtime it should execute, and flash
or upload from independent dockable device panels.

Rust is the main application. Run all commands from this repository root.

## Development

Install Rust and the native platform build tools. On macOS, install Xcode Command
Line Tools. The application is validated with Rust 1.99.1 on macOS.

```sh
cargo run --locked
```

Build a release binary:

```sh
cargo build --release --locked
```

## macOS application

```sh
./scripts/bundle-macos.sh
open target/release/Anode.app
```

The script builds the native application, generates its standalone Anode icon,
and creates a locally signed `.app` bundle. Distribution signing and notarization
are separate release work.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

## Boards

The catalog covers the Espressif range (ESP8266 and ESP32 through S2, S3, C3,
C6 and H2), RP2040 and RP2350, AVR, Renesas RA4M1, SAMD21/51, SAM3X, nRF52840,
STM32 F1/F4/H7 and NXP i.MX RT, across Espressif, Raspberry Pi, Arduino,
M5Stack, Seeed Studio, Adafruit, Unexpected Maker, LilyGO, Heltec, SparkFun,
Waveshare, Pimoroni, DFRobot, LOLIN, BBC, PJRC and ST boards. The **Catalog**
panel lists them with the tools each one needs.

Most boards cannot be identified from USB alone: every board behind a CP210x
bridge presents `10c4:ea60`, and every ESP32-S3 using native USB presents
`303a:1001`. Anode narrows those to a chip family, says so, and asks you to pick
the exact board once — the choice is then remembered for that physical board.
Any board not listed by name works through its family's generic entry.

## Runtimes

Each board declares the runtimes it can execute, and the selected runtime
decides what the editor's commands do:

| Runtime | Run | Build | Upload | Firmware |
| --- | --- | --- | --- | --- |
| MicroPython | `mpremote run` | — | `mpremote cp` | micropython.org build |
| CircuitPython | save `code.py` | — | copy to CIRCUITPY | circuitpython.org build |
| Arduino | — | `arduino-cli compile` | `arduino-cli upload` | the sketch is the firmware |
| C / C++ | — | `pio run` or `idf.py build` | `pio run -t upload` or `idf.py flash` | the program is the firmware |

Opening a folder also sets the runtime: `platformio.ini`, an ESP-IDF
`CMakeLists.txt`, a `.ino`, `code.py` or `main.py` each identify the project.
**New project** creates a starting point for any of them.

## Device tools

USB discovery and the serial REPL run in the application; everything else is
the ecosystem's own tool. The Board panel lists exactly what the selected board
and runtime need, whether each is installed, and installs the ones it can.

```sh
python3 -m pip install mpremote esptool platformio
brew install arduino-cli dfu-util
```

ESP-IDF is installed separately; export its environment in the shell that
launches Anode so `idf.py` is on `PATH`. Firmware downloads use `curl`. A
bootloader-volume board (Pico, most SAMD and nRF52 boards, micro:bit) is flashed
by copying the image onto the volume it mounts, so it needs no flashing tool at
all — only the right button held while it connects.

The `arduino-cli` board name Anode uses for each board is a default you can
replace in the Board panel; cores rename boards between releases.

## Workspace

- Files, Boards, Catalog, Editor, Terminal, Output, REPL and Board can be
  docked, grouped and resized.
- Terminal runs your default login shell in a real PTY. It starts in the open
  project folder, or your home directory when no project is open. Each project
  retains its own session until the app closes; switching projects preserves
  running commands and shell history. Use Ctrl+C to interrupt, Tab to complete,
  and the arrow keys for history. Select text to copy, and use the Paste button
  or the platform paste shortcut.
- Output carries everything the external tools say, with timestamps, a
  Problems filter and a follow toggle. REPL is the live serial session; Up and
  Down walk its history.
- Boards lists the last 12 connected devices and updates USB presence
  automatically.
- Each physical board can have its own panel, model selection, flash/upload job
  and output. Port locks prevent concurrent operations on the same device, and
  a bootloader-volume lease prevents two boards writing to one boot volume.
- Recent projects, device history, model overrides, board-name overrides and
  panel layout persist in the user configuration directory.
- For an isolated development session, set `ANODE_CONFIG_DIR` to another folder.

See [architecture and current limitations](docs/architecture.md).

## Previous implementation

The Electron application was checkpointed in commit `99985a4` before replacement.
Its source, tests and build configuration remain available in Git history.
