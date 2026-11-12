# Anode IDE — Project Summary

**What it is:** Electron + React + Vite desktop IDE for MicroPython development on microcontrollers, targeting feature parity with Thonny.

## Done so far

- **Core IDE features:** Monaco editor, serial port detection, run/upload code via mpremote, firmware flashing (ESP32/Pico/Arduino), project file browser, shell log, command palette (Cmd+K), welcome screen with templates, settings, recent projects, dark/light theme, cross-platform builds.
- **Board support:** esp32, esp32-s3, esp8266, m5stack-atom, m5stamp-s3 — with VID/PID detection logic (`detectBoardType()` in `src/lib/store.ts`), dedicated flash paths, and board-specific templates.
- **Testing infrastructure:** Vitest (unit/integration) + Playwright (E2E) set up; 562 tests passing, **82.1% statement coverage / 86.65% lines** — exceeds the 80% goal.
- **Phase 1 REPL/Serial Terminal:** Implemented end-to-end — IPC handlers in `electron/main.ts`, preload API, `ReplPanel` component (connect/disconnect/^C/soft-reset, scrollable output, input with history), store state, command palette integration, 30 tests.
- **Roadmap doc:** `FEATURESET.md` — full Thonny gap analysis across 4 priority phases.

## Assumed next step

Per the Phase 1 plan in `FEATURESET.md:90`, the remaining **P0 items** to implement next are:

1. **Device file management** — browse/upload/download/delete files on the board's filesystem (Thonny's dual-pane Files panel).
2. **Multiple file tabs** — tab bar to switch between open files in the editor.
3. **Find & replace** — expose Monaco's Cmd+F / Cmd+H.
4. **Auto-connect** — reconnect to last-used port/board on startup; reconnect on disconnect.

A secondary backlog item is wiring `detectBoardType()` to the flash dialog so the board auto-selects instead of requiring manual selection.
