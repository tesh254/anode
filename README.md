# Anode

A native hardware programming IDE built with Rust and GPUI Kit. Edit Python and
Arduino projects, detect USB boards, use a serial REPL, and flash or upload from
independent dockable device panels.

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

## Device tools

USB discovery and the serial REPL run in the application. Python 3 and external
tools are needed for device operations:

```sh
python3 -m pip install mpremote esptool
```

Install `arduino-cli` for Arduino compilation and upload. Firmware downloads use
`curl`. A Pico firmware operation writes to its mounted RPI-RP2 BOOTSEL volume.

## Workspace

- Files, Boards, Editor, Output and REPL can be docked, grouped and resized.
- Boards lists the last 12 connected devices and updates USB presence automatically.
- Each physical board can have its own panel, model selection, flash/upload job
  and output. Port locks prevent concurrent operations on the same device.
- Recent projects, device history, model overrides and panel layout persist in
  the user configuration directory.
- For an isolated development session, set `ANODE_CONFIG_DIR` to another folder.

See [architecture and current limitations](docs/architecture.md).

## Previous implementation

The Electron application was checkpointed in commit `99985a4` before replacement.
Its source, tests and build configuration remain available in Git history.
