# ACP release specification: Codex and OpenCode

Status: ready for implementation planning; transport and connected chat are not
implemented. Updated November 13, 2026.

## Objective and scope

Deliver working conversations with **Codex** and **OpenCode** in Anode's
existing **Agent** tab, alongside **Board** in the right dock. Developers should
be able to ask questions about an open firmware project, review agent edits,
run builds, and inspect results without leaving the editor.

“OpenCode V2” means the current upgraded OpenCode release and its changed
integration behavior. It is not an alternate executable or an instruction to
require a literal `2.x` version string. Use the installed current `opencode`,
record its version, and validate its actual ACP capabilities. The UI label is
**OpenCode**. OpenCode product versions and ACP protocol versions are separate.

This document is the first-release implementation contract. It narrows the
broader [ACP architecture](acp-integration.md) and adopts the tool contracts in
[Agent engineering tools](agent-engineering-tools.md). Existing UI behavior is
described in [Native architecture](architecture.md#agent-side-chat).

Included:

- Local Codex adapter and OpenCode ACP processes, existing provider login,
  capability negotiation, project-scoped sessions and connection diagnostics.
- Streaming conversation, tool results, permissions, cancellation and recovery.
- Honest project/board context, live-buffer integration and initial engineering
  MCP tools backed by existing project and hardware services.

Deferred: Claude and other agents, ACP registry installation, remote ACP,
external desktop-app control, subagent-specific navigation, instrument adapters,
full component catalogs, circuit editing and breadboard/diagram rendering.
The engineering specification remains the roadmap for those later tools.

## Verified integration baseline

The following are upstream facts; the requirements after this section are
Anode design decisions. Recheck and record tested package versions when building
and releasing; documentation changes alone are not a compatibility test.

| Provider | Current integration | Sources |
| --- | --- | --- |
| Codex | `codex-acp` is a stdio ACP adapter that starts Codex App Server. Its npm package is `@agentclientprotocol/codex-acp` and includes a compatible Codex dependency; `CODEX_PATH` can override the executable. | [Adapter README](https://github.com/agentclientprotocol/codex-acp#readme), [Codex App Server](https://developers.openai.com/codex/app-server) |
| OpenCode | Launch `opencode acp`; newline-delimited JSON-RPC uses ACP protocol v1. Use the current CLI documentation, not the older `/docs/acp/` page redirected into V1 docs. | [Current ACP reference](https://opencode.ai/docs/cli/acp/) |

OpenCode's current reference specifies existing provider credentials, absolute
session `cwd`, one active prompt per session, and stdio/HTTP MCP server support.
Client-supplied MCP servers are shared with sessions in the same directory for
the process lifetime; they can replace a configured server with the same name.
It does not support MCP over ACP. Model, effort and mode options are supplied
by the agent. See the authentication, sessions, config-options and MCP sections
of the [current reference](https://opencode.ai/docs/cli/acp/).

Codex adapter authentication methods and optional extensions are described in
its [README](https://github.com/agentclientprotocol/codex-acp#authentication).
Anode must negotiate capabilities instead of assuming that every adapter release
supports every advertised extension.

Protocol references: [initialization](https://agentclientprotocol.com/protocol/initialization),
[session setup](https://agentclientprotocol.com/protocol/session-setup),
[permission/tool calls](https://agentclientprotocol.com/protocol/tool-calls),
and [prompt turns](https://agentclientprotocol.com/protocol/prompt-turn).
Pin the Rust SDK version and verify its actual APIs before implementation.

## Launch profiles and compatibility

Define provider profiles with stable IDs `codex` and `opencode`:

| Setting | Codex | OpenCode |
| --- | --- | --- |
| Display label | Codex | OpenCode |
| Default executable | Resolved absolute path to `codex-acp` | Resolved absolute path to `opencode` |
| Arguments | Empty | `["acp"]` |
| Version probes | Adapter version; bundled/overridden Codex version when available | `opencode --version` |
| Working directory | Canonical project root | Canonical project root |

Expose executable-path and argument overrides in provider setup, using argv
arrays rather than shell strings. Resolve GUI PATH differences through existing
`src/toolchain.rs` conventions; do not infer that a Dock-launched process inherits
the developer's shell setup. Report the resolved executable in diagnostics.

The Codex adapter can be installed using the upstream npm package. Show manual
setup guidance if absent; do not auto-download an unpinned package at every
Connect. Prefer the adapter's compatible bundled Codex dependency. An explicit
`CODEX_PATH` override must be visible in diagnostics and the compatibility
record. Do not launch plain `codex` and interpret its interactive output as ACP.

Maintain a checked-in compatibility record containing tested adapter/CLI
versions, platform, negotiated protocol, supported capabilities, test date and
known limitations. Record the OpenCode upgrade's actual behavior through the
acceptance suite; do not copy V1 configuration assumptions. Unknown versions
may connect after a successful handshake, with an “untested version” diagnostic.
Unsupported protocol negotiation is a connection failure with recovery guidance.
No forced product-major gate and no silent downgrade.

## Connection and session lifecycle

The session service owns child processes and protocol state; the panel owns
presentation and drafts. Spawn and I/O run away from GPUI's render thread.
Use bounded event channels, request timeouts and explicit process ownership.

```text
Disconnected -> Starting -> Initializing -> Authenticating (if needed)
                         -> Opening session -> Ready -> Responding -> Ready
Any connected state -> Disconnecting -> Disconnected
Startup/transport failure -> Failed -> explicit Reconnect
Responding -> Cancelling -> Ready (only after completion acknowledgement)
```

Requirements:

1. Connect resolves the profile and project root, starts the child process,
   sends `initialize`, records its response and negotiates only implemented
   client capabilities. Keep stderr separate from ACP stdout.
2. If authentication is required, present the advertised method and provider
   guidance. Reuse existing agent credentials. Do not add API-key storage to
   Anode. Advertise terminal-auth only after implementing the terminal handoff.
   A successful login is followed by reconnect/reinitialization as required.
3. Create a project-scoped session with absolute `cwd` and Anode's MCP bridge.
   Do not connect without a project root in this release; show Open/Create
   project actions. Project creation before connection uses Anode's templates.
4. Keep the session alive across Board/Agent tab switches and panel close/reopen.
   Disconnect explicitly or on application exit; closing a tab is not process
   termination. New chat gets a distinct session and empty conversation.
5. Switching provider selects a separate provider/project conversation and draft.
   Never send a message to a different provider because its selector changed.
   Let an active response finish or Stop it before changing its session binding.
6. Project switches isolate transcripts, grants and MCP access. Stop/detach the
   previous project's session before opening another; preserve the draft.
   Project changes must never silently retarget a running hardware operation.
7. Cancel uses the negotiated protocol, disables duplicate Stop actions, and
   retains partial output. Bound the cancellation wait; forced shutdown is
   labeled as interruption, not successful completion.
8. On crash or malformed protocol output, retain transcript and unsent draft,
   surface bounded redacted diagnostics, reject pending requests and offer
   Reconnect. Never automatically replay a prompt or hardware mutation.
9. Resume/load is capability-gated. If unavailable or incompatible after an
   upgrade, offer a new session with an explicit context summary. Do not present
   a local transcript as restored provider-side memory.

For initial process isolation, use one agent process per active conversation.
In particular, do not multiplex unrelated OpenCode projects or independently
authorized sessions through one process. Use a unique Anode MCP server name and
warn on a configuration collision rather than replacing a user's server silently.

## Agent side-chat UI

Extend `src/panels/agent.rs`; retain the right dock grouping implemented in
`src/panels/mod.rs`. Do not add an Agent editor mode or take over the center pane.

- **Header:** Codex/OpenCode selector, Connect/Disconnect, status, New chat and
  provider setup. Show Model and Mode only from agent-supplied session options;
  show effort only when available. Never hard-code model IDs.
- **Conversation:** user messages, streamed assistant Markdown, collapsible
  tool cards keyed by session/tool-call ID, diffs, terminal output, permission
  requests and errors. Render only supported content types; unknown optional
  updates cannot crash the view. Keep internal reasoning hidden by default.
- **Composer:** multiline draft, Send, and Stop while responding. Enter sends,
  Shift+Enter adds a newline; respect IME composition. Empty/whitespace input
  cannot send. One prompt per session; preserve edits while a response runs.
- **Context:** visible project, board, runtime, active file and attachment list.
  Allow current selection/output attachment. Distinguish included context from
  merely displayed workspace labels. Disclose content sent to the agent.
- **Scroll/focus:** follow new output only while the user is at the bottom;
  provide Jump to latest otherwise. Preserve position and draft across tabs.
  Keep the composer reachable at narrow widths, with keyboard navigation and
  accessible names. Permission resolution returns focus to the conversation.
- **Connection states:** actionable missing executable, auth required,
  incompatible version/protocol, disconnected, reconnecting and failed states.
  A connected indicator appears only after a usable session exists.

Select only Codex and OpenCode for the first release; remove unavailable Claude
and Custom choices from the current placeholder selector until supported.
Provider names and version details must not suggest ACP v2 is required.

## Workspace and hardware context

Build an immutable revisioned context snapshot from `Workspace`: project root,
kind, entry point, selected target/runtime, active file/selection, unsaved buffer
revisions, detected board confidence and toolchain readiness. Include missing
facts explicitly. Use a bounded initial snapshot and refresh changed context
before each turn. MCP reads obtain the current snapshot, not stale prompt text.

A family-level detection cannot establish a verified pinout. Until engineering
catalog and circuit tools exist, the agent must request exact board/module
information and report unsupported tools honestly. Never generate a fake tool
success or claim a wiring diagram has been validated by unavailable checks.

Integrate ACP file operations when supported, but do not assume agents always
use client-provided filesystem calls. Agent-native disk edits also need a file
watcher and reconciliation path. If an open buffer has unsaved edits, surface
an explicit conflict and preserve both versions. Builds must declare whether
inputs came from saved files or an explicit snapshot; do not build stale disk
content while implying the live editor was compiled.

## Initial MCP tool slice

Use the schemas and revision/error contracts from the engineering specification.
These names are Anode tools, not ACP methods. Register only implemented tools:

| Delivery step | Tools | Existing integration |
| --- | --- | --- |
| Project awareness | `anode_project_context`, `anode_project_files`, `anode_project_read`, `anode_project_search`, `anode_board_status` | Workspace, project loader, board catalog |
| Create and build | `anode_project_templates`, `anode_project_plan`, `anode_project_create`, `anode_project_open`, `anode_build`, `anode_output_tail`, `anode_job_status`, `anode_job_cancel` | Project templates, device commands, console/jobs |
| Hardware execution | `anode_run`, `anode_upload`, `anode_repl_eval`, `anode_flash_firmware` | Existing commands, confirmations and shared leases |

Circuit/BOM/breadboard/diagram tools follow separately. A request about them may
produce an explanation or proposal, but the UI must not imply those artifacts
already have Anode electrical validation.

Use an authenticated local stdio MCP bridge that forwards to the running app's
command service. Pass it via standard session MCP configuration; do not require
MCP-over-ACP or a speculative SDK extension. Scope its token to the current
conversation/project and revoke it at disconnect. Share the app's existing port
and bootloader-volume leases. Capture device identity and connection generation
in hardware jobs; a reused port path cannot authorize a replacement device.

## Permissions and trust

Render the agent's actual permission options and IDs; do not rename an
`allow_always` choice to “Allow for session” when its real persistence differs.
Do not synthesize a broader grant than requested. Bind requests to their
session/tool call; resolve each once. Cancellation, disconnect and stale sessions
must deny or cancel unresolved requests.

Anode's MCP command service enforces its own workspace and hardware policy even
when the agent does not send an ACP permission request. Reuse valid grants for
the authorized scope; preserve the existing firmware destructive confirmation.
An unrestricted agent shell is outside these MCP guarantees and must not be
presented as sandboxed by Anode. Project documents and serial output cannot
change grants or application policy.

Credentials remain agent-owned. Persist executable configuration, version
metadata, project/session IDs and user-chosen transcript history; never persist
secrets, inherited environment dumps or session-scoped bridge tokens. Redact
stderr and protocol diagnostics before display/export. Store local chat state
outside the project's source tree; define retention and Delete chat behavior.

## Proposed source ownership

| Module | Responsibility |
| --- | --- |
| `src/agent/mod.rs` | Provider/project conversation lifecycle and command API |
| `src/agent/profiles.rs` | Launch profiles, executable resolution and compatibility records |
| `src/agent/transport.rs` | Process ownership, framing, request IDs, timeouts and shutdown |
| `src/agent/client.rs` | ACP callbacks, capabilities, permission/file/terminal dispatch |
| `src/agent/context.rs` | Revisioned live workspace snapshots |
| `src/agent/tools/` | Initial MCP schemas and shared-service dispatch |
| `src/agent/history.rs` | Local drafts/transcripts and provider session references |
| `src/panels/agent.rs` | Retained chat presentation and interaction |

Promote session ownership out of the panel before transport implementation so
closing/recreating the panel cannot orphan or kill an active conversation.
Retain SDK handles/tasks explicitly and reject events from old connection
instances after reconnect. Do not instantiate protocols or processes in render.

## Implementation milestones and acceptance

1. **Provider setup and transport.** Connect both agents through real local
   binaries; verify executable discovery, isolated stderr, initialization,
   missing executable, auth required, unsupported protocol and cleanup. Capture
   tested versions, especially the upgraded OpenCode release.
2. **Working side chat.** Send a project question and display streamed replies
   from each provider. Verify multiline entry, Stop, one active turn, independent
   provider drafts, tab switching, project switching, long output, Unicode/IME,
   crash recovery, and no prompt replay after failure.
3. **Edits and tools.** Render real permission requests and diffs. Exercise
   unsaved-buffer conflicts, bounded context, native disk edits, project creation
   without overwrite and a real build with attributable diagnostics. Verify
   MCP registration for both providers and OpenCode process isolation.
4. **Hardware jobs.** Use an explicitly selected real device to verify leases,
   disconnects, cancellation, target changes and operation deduplication. Run
   firmware-erasing checks only as a deliberate separate verification action.
5. **Release readiness.** Run protocol fixture/contract tests, GPUI interaction
   tests and real-agent smoke checks. Record results separately: a mock reply
   does not establish Codex/OpenCode compatibility. Check light/dark, minimum
   window width, scroll behavior, focus and reconnection before release.

First release is complete when both providers can converse in the Agent tab,
use current project context, present edits and permissions accurately, and use
the implemented project/build tools, with documented tested versions and recovery
behavior. Hardware execution ships only after its separate acceptance checks;
advanced engineering tools retain their own milestones.
