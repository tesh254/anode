# Native application architecture

Anode is one Rust application. GPUI owns the window and retained editor entities;
device work runs on background threads and sends progress back to the UI.

## Source map

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Application bootstrap, native window, keyboard commands, Dock icon |
| `src/lib.rs` | Application modules |
| `build.rs` | Rasterize `assets/app-icon.svg` into the embedded icon PNG |
| `src/assets.rs` | Embedded brand SVGs, default and application-specific icons |
| `src/workspace_state.rs` | Project/editor state, USB polling, runtime profiles and shared commands |
| `src/views.rs` | Title bar, recent boards, project explorer, editor and terminal views |
| `src/panels/` | Dockable tools, independent device jobs/logs and persisted layouts |
| `src/hardware.rs` | Physical device identity, connection history and shared port leases |
| `src/boards.rs` | Board catalog, USB matching and remembered model overrides |
| `src/profiles.rs` | MicroPython, CircuitPython and Arduino execution profiles |
| `src/device.rs` | mpremote, esptool, arduino-cli, firmware downloads and device filesystem operations |
| `src/repl.rs` | Interactive serial connection and reader/writer threads |
| `src/project.rs` | Local files and project templates |
| `src/settings.rs` | Configuration JSON and recent projects/devices |
| `src/theme.rs` | Neutral light/dark surfaces with green commands and teal device accents |
| `scripts/bundle-macos.sh` | Release build, `.icns`, Info.plist and development signing |

## Panels and USB connections

Files, Boards, Editor, Output and REPL are retained dockable tools. Drag tabs to
rearrange, split or group panels, and resize their borders. Layout and open device
panels are saved after changes. The activity bar reopens tools. Reset panel layout
restores the default tools while retaining device panels and running jobs.

Boards contains recent USB connections with Connected/Offline status. Discovery
runs off the UI thread every second. macOS `/dev/tty.*` and `/dev/cu.*` aliases
collapse to one device. USB serial numbers let history and device panels follow
a board across port renumbering. Model selections are remembered for ambiguous
USB descriptors.

Open panel creates a device panel or selects its existing panel wherever it has
been docked. Different boards can flash/upload concurrently with separate progress
and results. Shared port leases prevent workspace commands, REPL and device panels
from competing for one board. Running device panels cannot be closed. Pico UF2
jobs also share a boot-volume lease because BOOTSEL lacks the serial identity.

Firmware confirmation identifies the target and firmware variant. Espressif output
streams live. Firmware and upload temporary files are unique per operation and
are removed after use. Use for Run selects a board for shared editor commands;
Connect REPL selects it and opens the terminal. The header also selects the active
port and execution profile.

## Editing and persistence

The component-layer GPUI editor installs Python syntax highlighting. Local files
and dirty editor tabs live in retained workspace state. The native REPL uses a
text view and input, with interrupt, soft reset and ESP32 boot-loop filtering.

Settings are stored in `anode/settings.json` under the platform configuration
directory. `ANODE_CONFIG_DIR` overrides that folder for development and isolated
smoke checks. No fixture devices are inserted into ordinary user history.

## Current limitations

- Device filesystem operations exist, but their tree UI is not built.
- Find/replace, command palette and settings dialog are not built.
- Arduino CLI automatic installation and Windows CIRCUITPY drive-letter detection
  are not implemented.
- The project explorer lists top-level Python/Arduino files; nested directories
  and other file types are not shown.
- Arduino syntax highlighting is not enabled.
- USB descriptors cannot uniquely identify every model. Unknown devices and
  generic families need a manual choice. Devices without serial numbers use the
  port path as identity and may create another history entry after renumbering.
- Pico firmware requires BOOTSEL. Start from the connected board and enter BOOTSEL
  when prompted. Only the intended Pico should be in BOOTSEL mode; multiple
  mounted boot volumes are rejected.
- Physical USB attach/reconnect and firmware writes still need hardware validation.
- macOS is the validated development platform; other platforms need build and
  packaging validation.
