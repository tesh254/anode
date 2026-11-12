# Anode — Rust / GPUI port

Native macOS/Windows/Linux port of the Electron Anode IDE for MicroPython
development, rebuilt on [GPUI Kit](https://gpui-kit.com) (`gpui-kit` 0.9).

The React/Electron app in the repo root is the reference implementation; this
crate is the translation. Everything that ran in two processes (Electron main
+ renderer, connected by 30+ IPC channels) now runs in one process: serial
I/O, flashing, mpremote, and the UI share the same address space.

## Run

```sh
cd rust
cargo run
```

For the macOS app bundle and persistent Dock icon:

```sh
./scripts/bundle-macos.sh
open target/release/Anode.app
```

The bundle is signed locally for development. It is not notarized for distribution.

## Test

```sh
cargo test
```

## Module map

| Rust module | Translated from | What it owns |
| --- | --- | --- |
| `main.rs` | `electron/main.ts` (window bootstrap), `src/main.tsx` | App init, theme from OS appearance, key bindings, window creation, dock icon |
| `build.rs` | `public/app-icon/macos/icon.icns` | Rasterizes `assets/app-icon.svg` (the standalone Anode mark) to the embedded 1024×1024 PNG at compile time |
| `assets.rs` | Vite asset pipeline | App `AssetSource`: default Lucide bundle plus explicit application icons, brand SVGs |
| `workspace_state.rs` | `src/lib/store.ts` + `src/App.tsx` state/effects | The workspace entity: project state, ports, profiles, REPL, log, flash flow |
| `views.rs` | `src/App.tsx` render tree, child components | Native title bar, recent boards, project explorer, editor and terminal content |
| `panels/` | Native workspace | Dockable tools, independent board panels, per-device jobs and logs, saved layouts |
| `hardware.rs` | USB connection management | Recent device history, serial identity, macOS alias deduplication, shared port leases |
| `boards.rs` | `src/lib/boards.ts` | Board catalog, USB-descriptor matching, per-port board overrides |
| `profiles.rs` | `src/types/interpreter.ts`, `store.ts` detection | Runtime profiles (MicroPython/CircuitPython/Arduino), port classification |
| `device.rs` | Electron IPC handlers (`upload-code`, `flash-firmware`, …) | mpremote/esptool/arduino-cli workflows, CIRCUITPY deploy, device fs |
| `repl.rs` | `repl-connect`/`repl-data` IPC + serialport.ts | Serial REPL connection on a background reader thread |
| `project.rs` | Electron file-system IPC handlers, `src/lib/templates.ts` | Local project files, templates |
| `settings.rs` | Zustand `persist` middleware | Settings JSON in the user config dir |

## What carried over

- Board detection from USB VID/PID (incl. the ambiguous ESP32-S3 native-USB
  case and the per-board override picker).
- The Anode a glyph: Dock icon (encoded PNG rendered by `build.rs`, plus bundled
  `.icns`) and the in-app mark in the header and empty state.
- Execution profiles: CircuitPython detection via the mounted CIRCUITPY drive
  outranking USB guesses; manual picks survive re-detection; board swaps
  replace them.
- REPL quirks: interrupt + banner nudge after connect (DTR/RTS reset on
  ESP32-S3), boot-loop spam filtering, ^C / soft-reset, input history.
- Flashing: auto-download of MicroPython firmware from micropython.org,
  esptool invocation, RPI-RP2 UF2 copy, tool availability checks and install.
- CircuitPython deploy-by-drive-copy (no serial port handoff).
- Recent projects, the last 12 connected boards, last port and board overrides, persisted to settings.json.

## What changed by design

- No IPC layer: `ipcMain.handle` pairs became plain function calls and
  background threads feeding events back into the UI.
- Monaco → GPUI Kit `Editor` (tree-sitter Python highlighting, via the
  component layer so the highlighter factory and syntax colors are installed);
  xterm REPL → native text + `Input`.
- The welcome screen folded into the editor empty state and the sidebar (open
  folder, recents, templates) so a fresh launch shows the way in without a
  separate screen.
- Zustand's store became one GPUI entity; the persisted subset lives in
  `settings.rs`.
- Toolbars and panels use GPUI Kit theme tokens instead of Tailwind classes;
  light/dark follows the OS appearance.

## Workspace panels

The shell restores the Electron neutral gray palette, with green commands and
teal device accents in both light and dark appearances.

Files, Boards, Editor, Output and REPL are dockable tools. Drag their tabs to
rearrange, split or group panels, and resize their borders. Layout and open board
panels are saved after changes. The activity bar reopens tools; Reset panel layout
restores the default tools while retaining board panels and running jobs.

Boards shows only recent USB connections, with Connected/Offline status; it does
not show stock images or an unconnected catalog. USB ports are scanned off the UI
thread every second. macOS `/dev/tty.*` and `/dev/cu.*` aliases collapse to one
physical device. Reconnecting a device with a USB serial number updates its entry
and follows its new port path. Auto-detection selects the best known model/family;
the board panel provides a remembered manual override for ambiguous descriptors.

Open panel creates an independent panel for that physical board, or selects its
existing panel. Different boards can flash/upload concurrently, each with its own
progress and result. Shared port leases prevent workspace commands, REPL and panel
jobs from competing for the same device. Running board panels cannot be closed.
Espressif output streams live; temporary firmware/upload files are unique per job
and removed after use. Firmware confirmation identifies the port and variant.
Use for Run selects the board for the shared editor commands; Connect REPL selects
it and opens the terminal. Active port and runtime are also available in the header.

For isolated development/smoke testing, set `ANODE_CONFIG_DIR` to a temporary
folder; Anode reads/writes `settings.json` there instead of the normal user config.
No fixture devices are shipped or inserted into ordinary history.

## Known gaps vs the Electron app

- Device file tree panel (`device-file-tree.tsx`) — the `mpremote fs`
  operations are translated (`device.rs`), but the tree UI is not yet built.
- Find & replace, command palette, and settings dialog are not yet built
  (the settings dialog in the Electron app was mostly placeholders).
- `install-tool` for arduino-cli (download/extract) is not yet ported;
  esptool install is.
- Windows CIRCUITPY drive-letter detection (the Electron app also deferred
  this).
- Project explorer currently lists top-level Python/Arduino files; nested folders
  and non-code files are not yet shown.
- USB descriptors cannot uniquely identify every model. Unknown devices and generic
  families need a manual model choice; devices without USB serial numbers use their
  port path as identity and may appear as new devices after USB renumbering.
- Pico firmware uses a BOOTSEL volume instead of the serial port. Start the action
  from the connected board, then enter BOOTSEL when prompted. Only the intended
  Pico should be in BOOTSEL mode; multiple mounted boot volumes are rejected.
- Physical USB attach/reconnect and firmware writes still need hardware validation.
