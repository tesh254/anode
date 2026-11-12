# Anode Feature Set — Thonny Clone

## Current Features (Already Implemented)

| Feature | Status |
|---|---|
| Monaco code editor with syntax highlighting | Done |
| Serial port detection & auto-board-identification | Done |
| Run code on device (mpremote) | Done |
| Upload code to device (mpremote/ampy) | Done |
| Stop code execution (soft-reset) | Done |
| Firmware flashing (ESP32/Pico/Arduino) | Done |
| Auto-download MicroPython firmware | Done |
| Project file browser | Done |
| Shell log panel | Done |
| Command palette (Cmd+K) | Done |
| Welcome screen with templates | Done |
| Settings dialog | Done |
| Recent projects | Done |
| Dark/light theme | Done |
| Board panel (port selection, board detection) | Done |
| Tool install (esptool, arduino-cli) | Done |
| File/folder creation | Done |
| Keyboard shortcuts (Cmd+S, Cmd+K) | Done |
| Cross-platform builds (macOS/Windows/Linux) | Done |

## Missing Features (Required for Thonny Parity)

### P0 — Core IDE (must-have)

| Feature | Description |
|---|---|
| **Interactive REPL/Shell** | Live Python REPL connected to board via serial. User types commands, sees output immediately. This is Thonny's signature feature. |
| **Serial terminal/monitor** | Raw serial output stream from board (print statements, errors, tracebacks). Bi-directional: user can type into REPL. |
| **File management on device** | Browse, upload, download, delete files on the microcontroller's filesystem (not just local project). Thonny's Files panel shows both local and device files. |
| **Save to device** | Save current file directly to board filesystem (not just upload as a separate action). |
| **Auto-connect** | Auto-connect to last-used port/board on startup. Reconnect on disconnect. |
| **Multiple file tabs** | Open and switch between multiple files in the editor with tab bar. |
| **Undo/redo** | Full undo/redo support in editor (Monaco provides this, need UI buttons + shortcuts). |
| **Find & replace** | In-editor search/replace (Cmd+F, Cmd+H). Monaco provides this, need UI exposure. |
| **Go to line** | Jump to specific line number. |
| **Error highlighting in shell** | Traceback parsing — click on file:line in shell output to jump to that location in editor. |

### P1 — Important (Thonny experience)

| Feature | Description |
|---|---|
| **Package manager** | Install/uninstall MicroPython packages on the device (upip/mip). Thonny has "Tools → Manage packages". |
| **Device info panel** | Show board type, firmware version, available memory, disk space. |
| **Indentation guides** | Visual guides for Python indentation levels. |
| **Line numbers** | Toggle line numbers (already on by default in Monaco). |
| **Comment/uncomment** | Toggle line comments (Cmd+/). Monaco provides this. |
| **Auto-indent** | Smart indentation after colons in Python. Monaco + Python language service. |
| **Bracket matching & auto-closing** | Already in Monaco, ensure Python support. |
| **StatusBar details** | Show language mode, encoding, indentation, line ending type. |
| **New file without project** | Create unsaved scratch file (Thonny's "New" creates untitled buffer). |
| **Save As** | Save current file to a new path. |
| **Close file** | Close current file tab (prompt to save if dirty). |

### P2 — Nice to have (advanced Thonny features)

| Feature | Description |
|---|---|
| **Debugger / step-through** | Thonny's signature step debugger. Complex — requires custom backend protocol. |
| **Variable inspector** | View variables and their values during debugging or after shell commands. |
| **Syntax error highlighting** | Real-time syntax error markers in editor (Monaco can do basic, Python needs Pylance/LSP). |
| **Code completion** | Context-aware autocomplete for MicroPython APIs. |
| **Assistant / error explainer** | Thonny's Assistant panel that explains errors and suggests fixes. |
| **Outline view** | Functions/classes outline in sidebar. |
| **Plug-in system** | Thonny has a plug-in API for extensions. |
| **Multiple interpreter support** | Switch between local Python, MicroPython, CircuitPython backends. |
| **Configurable interpreter** | Point to custom Python/micropython binary. |

### P3 — Polish

| Feature | Description |
|---|---|
| **Keyboard shortcut customization** | Let users remap shortcuts. |
| **Editor theme selection** | Multiple editor themes beyond vs-dark. |
| **Font selection** | Custom editor font. |
| **Zoom in/out** | Cmd+/- for editor font size. |
| **Toggle minimap** | Show/hide code minimap. |
| **Word wrap toggle** | Toggle word wrap. |
| **Auto-save** | Periodic auto-save option. |
| **Drag & drop file open** | Open files by dragging into window. |
| **Recent files** | Track recently opened files separately from projects. |

## Implementation Priority

**Phase 1 (MVP+):** REPL, serial terminal, device file management, multiple tabs, find/replace, auto-connect
**Phase 2:** Package manager, device info, error traceback linking, save-as/close
**Phase 3:** Debugger, variable inspector, code completion
**Phase 4:** Plug-in system, polish items
