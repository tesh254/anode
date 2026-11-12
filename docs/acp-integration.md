# Integrating Coding Agents into Anode via ACP

First implementation: [Codex and current OpenCode release specification](acp-codex-opencode-spec.md).

**Status: the Agent side-panel UI is implemented; ACP connections and hardware
MCP tools are not implemented in Anode yet.** The expanded [hardware engineering tool specification](agent-engineering-tools.md)
defines project discovery, component knowledge, circuit and breadboard design,
connection diagrams, project creation, and validation. Treat its milestones as
the implementation scope beyond the initial chat integration.

Anode can embed external coding agents—Claude, Codex, OpenCode, Gemini CLI, and any other agent that speaks the **Agent Client Protocol (ACP)**—directly inside the IDE so developers get hardware-aware assistance for writing, building, flashing, and debugging firmware on their boards.

---

## 1. Why ACP

Without a shared protocol, every IDE must build a custom integration for every coding agent. **ACP** (introduced by Zed and JetBrains under the Apache-2.0 license, at [agentclientprotocol.com](https://agentclientprotocol.com)) solves this the same way LSP solved language tooling:

- **Clients** (editors and IDEs like Anode) own the workspace UI, file buffers, terminal/console surfaces, and user permission prompts.
- **Agents** (Claude, Codex, OpenCode, etc.) own their LLM calls, reasoning loop, authentication, billing, and native tool execution.
- **Initial transport target** is JSON-RPC 2.0 over `stdio` for local agent subprocesses. Treat remote transports as future work requiring explicit protocol and adapter support.

For Anode, ACP means a developer can pick their preferred agent from a dropdown—using their existing Claude, OpenAI/Codex, or OpenCode credentials—and get live diffs, tool-call streaming, and board-aware hardware actions without Anode having to manage API keys or model providers itself.

---

## 2. Supported Agents & How They Connect

ACP separates **the protocol client** (Anode) from **the agent binary or adapter** (spawned as a child process or connected over HTTP/WS):

| Agent | How it speaks ACP | Auth & Config Ownership | Notes for Anode |
| --- | --- | --- | --- |
| **Claude Code / Claude Agent** | Official/community ACP adapter (e.g. `claude-code-acp` from the ACP Registry) | Agent owns Anthropic API key or `/login` subscription; reads `CLAUDE.md` | Claude Desktop itself is an MCP host app rather than an ACP server; use the Claude ACP adapter for in-IDE threads, or expose Anode's Hardware MCP server to Claude Desktop separately. |
| **Codex CLI / Codex** | Codex ACP adapter (`codex-acp` from the ACP Registry) | Agent owns OpenAI / ChatGPT login and sandbox policy | Supports streaming tool output, diffs, and project-scoped edits. |
| **OpenCode** | Built-in ACP mode (`opencode acp`) or ACP Registry entry | Agent owns provider & model selection (`opencode.json`) | Verify ACP support and launch arguments against the installed release; product version numbers are separate from ACP protocol versions. |
| **Any other ACP agent** | An executable speaking the negotiated ACP version over `stdio` | Agent-owned | Configured via Anode's custom agent settings (`command`, `args`, `env`, `cwd`). |

### Supporting Both Local Subprocesses and Desktop/Remote Agents

1. **Local CLI / Adapter (`stdio`)**: Anode spawns `<command> <args>` with `cwd` set to `Workspace::project_path` and communicates over `stdin`/`stdout` JSON-RPC 2.0. This covers Claude Code ACP, Codex ACP, OpenCode, and Gemini CLI.
2. **ACP Registry**: Anode can optionally fetch the shared ACP Registry manifest to offer one-click install/launch for known agents instead of requiring manual command paths.
3. **Standalone MCP Bridge for Desktop Apps**: For desktop apps that act as MCP hosts rather than ACP servers (such as Claude Desktop or Codex Desktop running outside Anode), Anode can expose the **same Hardware MCP server** over a local Unix domain socket or `stdio` bridge so those desktop apps can still inspect connected boards and trigger builds/uploads.

---

## 3. Rust SDK & Crate Architecture

Use a pinned ACP Rust SDK release and implement its client connection interface.
Check the selected release's APIs and negotiated capabilities during implementation;
the examples here are architecture proposals, not compile-ready SDK calls. Expose
hardware tools through a separate MCP server using stable session configuration.
Do not depend on draft ACP v2 or an experimental MCP-over-ACP bridge for the
initial integration.

Because Anode is a single-process GPUI desktop app (`src/main.rs`) that already runs hardware discovery and flashing on background threads (`src/device.rs`, `src/hardware.rs`), ACP's async JSON-RPC loop should run on a dedicated background Tokio runtime and communicate with the GPUI `Workspace` over bounded channels—matching the existing `drain_background_events` pattern in `src/workspace_state.rs`.

### Proposed Module Layout

```text
src/
  agent/
    mod.rs           # AgentManager, configured/registry agents, session lifecycle, UI event queue
    client.rs        # ACP Client implementation (session/update, fs/*, terminal/*, permissions)
    context.rs       # Serializes active BoardDef, Runtime, ProjectKind, Toolchain into session context
    mcp_hardware.rs  # Hardware MCP tools (board status, build/run/upload, flash, REPL/serial tail)
```

---

## 4. Integrating with Anode's Existing Systems

Anode already has strong domain primitives that make a coding agent dramatically more useful for embedded work than a generic editor chat:

### A. Hardware & Project Context Injection (`context.rs`)

When an ACP session starts (or when the active board/port changes), Anode injects a concise hardware context block into the session:

- **Board**: `BoardDef` (`id`, `name`, `chip`, `architecture`, `flash` method, `mpy_variant`, `cpy_board`, `fqbn`, `pio_board`) and detection confidence (`exact`, `from_override`, or `best guess`).
- **Runtime & Project**: active `Runtime` (`MicroPython`, `CircuitPython`, `Arduino`, `Native`) and detected `ProjectKind` (`src/project.rs`).
- **Connection & Toolchain**: selected serial port (`self.selected_port`), connection state, and installed/missing tools from `src/toolchain.rs`.

This gives the agent a starting point for selecting compatible tools and APIs.
It does not establish a verified pinout or electrical limits: the current
catalog describes boards and toolchains, not a complete electrical model.
Revision-specific pin and component data must come from the engineering
catalog described in the expanded specification.

### B. File & Editor Synchronization (`client.rs`)

ACP delegates file reads and writes back to the client (`fs/read_text_file`, `fs/write_text_file`):

- **Reads** return the live in-memory buffer if the file is open in `Workspace::project_files` (including unsaved edits), falling back to disk.
- **Writes** update `Workspace` buffers and `EditorState` directly and mark the tab dirty (or save on user approval), so the user immediately sees agent edits in the editor without reload conflicts.

### C. Hardware MCP Tools (`mcp_hardware.rs`)

Expose Anode's domain tools through a Hardware MCP server supplied in
`session/new.mcpServers`, using a transport the agent supports. ACP carries
editor/agent interaction and tool-call presentation; MCP exposes the hardware
tool schemas and invocations. Do not assume ACP compatibility guarantees every
optional capability. See the official [session setup](https://agentclientprotocol.com/protocol/v1/session-setup)
and [initialization](https://agentclientprotocol.com/protocol/v1/initialization)
specifications.

Enforce hardware authorization inside Anode's command service. An agent's ACP
permission request can supply the user interface, but the MCP handler must
also enforce policy when called from an external desktop host. The expanded
specification replaces the initial small tool list with a shared engineering
tool service.

| Tool | Purpose | Safety / Lease Behavior |
| --- | --- | --- |
| `anode_board_status` | Returns active port, resolved `BoardDef`, confidence, runtime, and toolchain readiness. | Read-only. |
| `anode_build` | Runs `arduino-cli`, `pio`, or `idf.py` compile and streams structured compiler errors back to the agent. | No serial port required; safe to run anytime. |
| `anode_run` / `anode_upload` | Runs the open script or uploads the project using `src/device.rs`. | Acquires `PortLeases` (`src/hardware.rs`); fails fast with a clear message if the REPL or a `FlashPanel` owns the port. |
| `anode_repl_eval` | Sends a short expression to the board REPL and returns stdout/traceback. | Acquires `PortLeases`; gated by user permission. |
| `anode_output_tail` | Reads the last N lines of Anode's `Output` log (`Workspace`) so the agent can diagnose upload or runtime failures. | Read-only. |
| `anode_flash_firmware` | Requests a firmware flash (`MicroPython`, `CircuitPython`, or local image). | **Always** requires explicit ACP `request_permission` + Anode's destructive-action confirmation dialog. |

### D. Terminal & Command Execution

When an agent requests `terminal/create`, Anode can either run it in a scoped subprocess or surface it through `src/terminal/` (`portable-pty` + `vt100`) so the developer can watch long builds (`pio`, `idf.py`, `cargo`) in the same Terminal tab.

---

## 5. UI & UX in Anode (`src/panels/mod.rs`, `src/views/`)

Following Anode's `DockArea` architecture and `gpui-kit` conventions:

1. **New `Tool::Agent` panel**:
   - A side chat in the same right-hand dock tab group as **Board**, with a new tab named **Agent**. Users toggle **Board | Agent** without leaving the editor. The Agent tab hosts conversations with the selected ACP-connected coding agent; it is not a separate editor mode or a replacement for the Board inspector.
   - Top bar: agent selector (`Select` listing configured ACP agents: Claude, Codex, OpenCode, Custom…), thread/new-session button, and connection status dot.
   - Message stream: user prompts, streaming markdown responses, collapsible tool-call cards (showing file edits, `anode_build` results, `anode_upload` progress), and inline diff previews.
   - Permission bar: when the agent sends `session/request_permission` (e.g., to write a file, run a shell command, or flash a board), render a focused inline approval card with **Allow once**, **Allow for session**, and **Deny** buttons (`Button` variants: `primary` / `outline` / `danger`).
   - Composer: multi-line input with `Enter` to send, `Shift+Enter` for newline, and a quick-attach chip for current board/output context.
2. **Settings persistence (`src/settings.rs`)**:
   - Store configured ACP agents (`id`, `name`, `command`, `args`, `env`, `enabled`) in `settings.json` under `ANODE_CONFIG_DIR` so custom adapters and registry installs survive restarts.

---

## 6. Security & Safety Guardrails

- **No silent board writes**: `anode_flash_firmware` and `anode_upload` must pass through ACP permission checks (`session/request_permission`) before touching hardware, and `PortLeases` guarantees two jobs never contend for one serial port or bootloader volume.
- **Workspace boundary**: `fs/read_text_file` and `fs/write_text_file` should canonicalize paths and reject escapes outside `Workspace::project_path` unless explicitly approved.
- **Zero credential storage in Anode**: Anode never stores Anthropic, OpenAI, or other provider API keys; each ACP agent manages its own auth flow.

---

## 7. Phased Rollout Plan

1. **Phase 1 — Core ACP Client & Agent Panel**
   - Add `agent-client-protocol` (v1) and a background Tokio runtime in `src/agent/`.
   - Implement `stdio` subprocess spawning for configured agents (`claude-code-acp`, `codex-acp`, `opencode acp`).
   - Wire `fs/*` to `Workspace` buffers and build the `Tool::Agent` panel with streaming updates and `session/request_permission`.
2. **Phase 2 — Hardware Context & MCP Tools**
   - Inject `BoardDef`, `Runtime`, `ProjectKind`, and `Toolchain` status into new sessions.
   - Add the Hardware MCP server (`mcp_hardware.rs`) exposing `anode_board_status`, `anode_build`, `anode_upload`, `anode_output_tail`, and `anode_repl_eval` with `PortLeases` enforcement.
3. **Phase 3 — Ecosystem Polish**
   - Add ACP Registry discovery/installation UI.
   - Add a standalone MCP bridge mode so external desktop apps (Claude Desktop, Codex Desktop) can also use Anode's Hardware MCP server.
   - Track future ACP protocol versions separately from agent product versions.
