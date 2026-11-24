# Native application architecture

Anode is one Rust application. GPUI owns the window and retained editor
entities; device work runs on background threads and sends progress back to the
UI.

## Source map

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Application bootstrap, native window, keyboard commands, Dock icon |
| `src/lib.rs` | Application modules |
| `build.rs` | Rasterize `assets/app-icon.svg` into the embedded icon PNG |
| `src/assets.rs` | Embedded brand SVGs, default and application-specific icons |
| `src/boards.rs` | The board catalog: chips, runtimes, flash methods, USB detection |
| `src/toolchain.rs` | External tools: what each board needs, where they are, installing them |
| `src/profiles.rs` | Which runtime a port executes, and the workspace each runtime needs |
| `src/project.rs` | Project files, project-kind detection and templates |
| `src/device.rs` | mpremote, esptool, arduino-cli, PlatformIO, ESP-IDF, firmware downloads |
| `src/workspace_state.rs` | Project/editor state, USB polling, runtime dispatch and shared commands |
| `src/views/` | Workspace shell, the Board panel, and the Output and REPL consoles |
| `src/panels/` | Dockable tools, the board catalog browser, per-board jobs and persisted layouts |
| `src/hardware.rs` | Physical device identity, connection history and shared port leases |
| `src/repl.rs` | Interactive serial connection and reader/writer threads |
| `src/recovery.rs` | Connection-failure classification and runtime/board recovery guidance |
| `src/settings.rs` | Configuration JSON and recent projects/devices |
| `src/theme.rs` | Neutral light/dark surfaces, green commands, and the console palette |
| `scripts/bundle-macos.sh` | Release build, `.icns`, Info.plist and development signing |

## Boards, chips and runtimes

`BOARD_CATALOG` is the single source of truth. Each entry names its chip, the
runtimes it can execute, how firmware reaches it, and the identifiers each
toolchain knows it by (`mpy_variant`, `cpy_board`, `fqbn` plus its core,
`pio_board`). The ESP-IDF target comes from the chip rather than the board,
because that is where it is actually decided.

Detection separates two kinds of USB descriptor. A product id a board owns
identifies it. A shared one — a CP210x or CH340 bridge, a silicon vendor's
native USB, a bare vendor id — narrows it to a chip family and no further, so
the match resolves to that family's generic entry and the UI says "best guess"
rather than naming a board it cannot know. A saved override always wins, keyed
by USB serial number where there is one and by port path otherwise.

A profile pairs a port with a runtime. The board decides which runtimes exist;
generic fallbacks keep all four reachable on an unidentified board. A mounted
CIRCUITPY drive outranks every USB guess, because it is the board telling us
directly. A manual pick survives re-detection until the board itself changes.

## Firmware and builds

`FlashMethod` describes how a complete image reaches a board: esptool over
serial, a copy onto a bootloader volume, dfu-util, bossac, or the Teensy
loader. Bootloader volumes are found by their marker file (`INFO_UF2.TXT`, or
`DETAILS.TXT` for DAPLink), optionally narrowed by volume name, and the write
waits a few seconds for the volume to appear because the user is pressing a
button while it runs. Flash offsets come from the chip: 0x1000 on the ESP32 and
S2, 0x0 everywhere else.

MicroPython and CircuitPython images are resolved from their projects' own
download pages and cached in a temporary file that is removed after use. A
local image can be flashed instead, which is the path for a board whose
download slug Anode does not know.

Compiling is the runtime's job, not the flasher's: `arduino-cli` for sketches,
`pio` for PlatformIO projects, and `idf.py` for ESP-IDF projects, chosen from
the detected project kind.

## Panels and USB connections

Files, Boards, Catalog, Editor, Terminal, Output, REPL and Board are retained
dockable tools. Drag tabs to rearrange, split or group panels, and resize their
borders. Layout and open device panels are saved after changes; a layout saved
by an older version is dropped rather than restored without its newer panels.

Discovery runs off the UI thread every second. macOS `/dev/tty.*` and
`/dev/cu.*` aliases collapse to one device. USB serial numbers let history and
device panels follow a board across port renumbering.

Different boards can flash or upload concurrently with separate progress and
results. Shared port leases prevent workspace commands, the REPL and device
panels from competing for one board. A second lease covers the bootloader
volume, which has no serial identity of its own, so two boards cannot write to
one boot volume. Running device panels cannot be closed.

Toolchain probing spawns processes, so it runs on a background task and the
result is cached. It re-runs after any job that could change what is installed.
Tool lookup searches `PATH` plus the directories a GUI process started from the
Dock does not inherit — Homebrew, `~/.local/bin`, pip's user bin.

Recognized serial, raw-REPL, bootloader and USB-volume failures expose **How to
fix** in a workspace notice and in per-board task panels. **Connection help**
remains available in the Board panel even before a failure. The shared dialog
in `src/views/recovery.rs` explains the failure, lists checks for the selected
runtime, and offers rescan, REPL, Board setup and diagnostic-copy actions.
Background operations capture their original port, runtime and board for this
guidance. Unknown or generic board models get an identification step rather
than a guessed bootloader sequence. The guide does not retry writes, install
firmware or assert that a busy port is the confirmed cause of a handshake
timeout.

## Editing and persistence

The component-layer GPUI editor installs the highlighter for the open file's
language: Python, C, C++ (also used for `.ino`), CMake and TOML. The project
explorer reads source and configuration files two directories deep, skipping
build output, and opens the entry point for the detected project kind.

The native REPL uses a text view and input, with interrupt, soft reset and a
bounded buffer. Up and Down are claimed before the input's own bindings so they
walk session history rather than the cursor.

Settings are stored in `anode/settings.json` under the platform configuration
directory. `ANODE_CONFIG_DIR` overrides that folder for development and
isolated smoke checks. No fixture devices are inserted into ordinary user
history.

## Current limitations

- Device filesystem operations exist, but their tree UI is not built.
- Find/replace, command palette and settings dialog are not built.
- The toolchain identifiers in the catalog (`fqbn`, `pio_board`, `mpy_variant`,
  `cpy_board`) are the published names at the time of writing. Cores rename
  boards between releases; the Arduino board name is editable per board in the
  UI, and the others fail with the download page's own message.
- ESP-IDF must already be exported in the environment that launches Anode;
  Anode cannot install it.
- `arduino-cli`, dfu-util, bossac, picotool, avrdude and the Teensy loader are
  installed through Homebrew on macOS only. Elsewhere Anode shows the command.
- MicroPython for the BBC micro:bit comes from microbit.org, not
  micropython.org, and is not downloaded automatically.
- Windows bootloader volumes are not enumerated: that needs platform APIs this
  build does not link. CircuitPython is still detected there by USB descriptor.
- Boards without a serial number use the port path as identity and may create
  another history entry after renumbering.
- Bootloader-volume firmware requires the board to be in its bootloader. Start
  from the connected board and enter it when prompted. Only the intended board
  should be in that mode; multiple mounted volumes are rejected.
- Physical USB attach/reconnect and firmware writes still need hardware
  validation. The UI is verified by launching the application and rendering
  every panel, including with a simulated attached board; it is not covered by
  automated UI tests, because `gpui-kit`'s `test-support` feature cannot
  currently resolve (`proptest` requires `rand_xorshift ^0.5`, which has no
  stable release).
- macOS is the validated development platform; other platforms need build and
  packaging validation.

## Agent side chat

The right dock switches between **Board** and **Agent** from the activity bar,
showing one panel at a time. Agent is a retained
panel (`src/panels/agent.rs`): a compact header with chat history and the
connection menu, the transcript, and a composer that holds the agent menu, a
searchable model palette grouped by the upstream provider each model comes
from, the remaining ACP session options, and the draft. **OpenCode Go** is
the first model group, followed by **OpenCode Zen**, then the current upstream
provider and the others alphabetically. The selected-model button also names
Go or Zen so identical model names remain distinguishable. This ordering
does not change the selected model. Drafts and selections survive panel
switches. Existing version-3 layouts gain the Agent panel without resetting
pane arrangements.

Chat never chooses an agent. Until the user picks one the panel shows an empty
state that opens Settings, and no composer, because there is nothing to send
to. A choice is saved in `Settings::agent_provider` and restored on the next
launch only while that provider is still configured and still resolves to an
executable.

## Settings

`src/panels/settings.rs` is a full-window destination with its own sidebar,
opened by the cog at the bottom of the activity bar. Its **Providers** page is a master–detail over the
provider registry (`src/agent/profiles.rs`): Codex and OpenCode ship as
built-ins, and any other stdio ACP agent can be added as a custom provider with
its own command, arguments and environment — which is also where a base URL and
API key for an OpenAI- or Anthropic-compatible endpoint go. The registry lives
in `<config>/anode/agents/providers.json`, written `0600` because it can hold
those keys.

Install and sign-in state always come from the provider's own commands, never
from the presence of a binary. `src/agent/install.rs` runs `npm install -g` and
the provider's sign-in as managed child processes and streams their output into
the page, so a failure is visible instead of disappearing into a terminal.
OpenCode's sign-in is an interactive picker and is opened in the Terminal panel
instead. The **Agent** page covers automatic connection and transcript history;
**Diagnostics** shows the current session's transport log.

The connected conversation and engineering tools are specified in
[ACP integration](acp-integration.md) and
[engineering tools](agent-engineering-tools.md).
