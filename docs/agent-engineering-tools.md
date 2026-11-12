# Agent tools for board programming and hardware engineering

First implementation: [Codex and current OpenCode release specification](acp-codex-opencode-spec.md).

Status: design proposal. None of the tools below are implemented yet.
This extends [ACP integration](acp-integration.md) from agent chat into a
project-aware engineering workspace.

An agent should be able to understand an open project, identify its board and
accessories, design a circuit, explain assembly, generate matching code, build
it, and help diagnose measurements. Firmware, wiring, bill of materials, and
diagrams must describe the same version of the design.

## Protocol boundary

Anode is the ACP client. The coding agent consumes Anode's engineering tools
through MCP. In ACP v1, supply the MCP server through `session/new.mcpServers`;
negotiate capabilities during initialization. A local stdio MCP bridge is the
initial interoperability target. Remote transports and image input are enabled
only when the selected agent advertises support.

ACP tool-call notifications describe execution and progress; they do not
themselves install new domain tools in an agent. Keep the tool service separate
from ACP so external MCP hosts can consume the same implementation.

Protocol references:

- [ACP session setup](https://agentclientprotocol.com/protocol/v1/session-setup)
- [ACP initialization](https://agentclientprotocol.com/protocol/v1/initialization)
- [ACP tool calls and permissions](https://agentclientprotocol.com/protocol/v1/tool-calls)

The tool names and contracts in this document are Anode proposals, not ACP
standard methods. Do not require an experimental MCP-over-ACP SDK feature for
the first implementation.

## Understand the open project first

Start with a bounded context snapshot instead of making the agent inspect the
entire repository on every prompt. Include:

- Project ID, canonical root, project kind, entry point, build targets, and
  runtime; distinguish detected values from user overrides.
- Open files, active file and selection, unsaved buffers, and content revisions.
- Board model, hardware revision when known, chip, runtime, detection confidence,
  selected physical device, connection generation, and current lease owner.
- Toolchain readiness, dependencies, build configuration, and recent diagnostics.
- Circuit revision, component instances, pin allocations, wiring status, and
  references to diagrams and datasheets.
- Assumptions, missing information, and unresolved compatibility issues.

USB discovery cannot identify the components on a breadboard. The user supplies
part numbers, catalog selections, or reference photos; photo-derived identity
remains a suggestion until confirmed. A shared chip family is insufficient to
claim a particular board revision or pinout.

### Project and workspace tools

| Proposed tool | Inputs | Result / effect |
| --- | --- | --- |
| `anode_project_context` | Detail level; optional prior revision | Bounded snapshot, changed sections, missing facts, resource references |
| `anode_project_files` | Glob, cursor, page size | Source/configuration tree with generated output excluded |
| `anode_project_read` | Path, optional line range | Live buffer or disk content, origin and revision |
| `anode_project_search` | Text, scope, result limit | Matches with paths and line numbers |
| `anode_project_patch` | Changes and expected file revisions | Atomic validated edits; conflicts preserve developer changes |
| `anode_project_targets` | Project ID | Boards, runtimes, build environments, entry points and selected target |
| `anode_dependencies` | Target; inspect or propose change | Runtime-specific dependency inventory, compatibility and proposed diff |
| `anode_project_templates` | Runtime, board family, accessory category | Available template IDs, requirements and generated file manifests |
| `anode_project_plan` | Destination, template, board, runtime, components | Creation plan: files, dependencies, circuit, assumptions and target settings |
| `anode_project_create` | Plan ID, expected plan revision, request ID | New project with source, manifest, wiring, BOM and setup instructions |
| `anode_project_open` | Project ID or authorized path | Select project and publish a fresh context revision |

Project creation must reuse `src/project.rs` templates and project detection.
Create in a temporary sibling directory, validate, then move into an unused
destination. Never overwrite an existing project. Opening it is an explicit
tool action so an agent can prepare several projects without switching context.
Dependency installation is a separate job; proposing dependencies does not
silently download or execute them.

## Component and board knowledge

Add a versioned engineering catalog alongside the existing board catalog.
Support sensors, discrete resistors/capacitors/diodes/transistors, LEDs, switches,
potentiometers, displays, breakouts, connectors, level shifters, motor drivers,
power modules, actuators, batteries, and breadboards. Components without a
catalog definition can be represented with unknown specifications.

Distinguish a bare IC from a breakout module. A module's regulator, pull-ups,
connector labels, and level shifting can change its wiring requirements.
Each definition needs manufacturer/model/revision, package or module variant,
pin names and aliases, power and signal domains, polarity, interfaces,
footprint geometry, and source provenance.

Record recommended operating limits separately from absolute maximum ratings.
Each factual property needs its source, document revision/page or section,
and verification status. Unknown is a first-class value. Agent-generated
values cannot silently become verified catalog facts.

| Proposed tool | Inputs | Result / effect |
| --- | --- | --- |
| `anode_components_search` | Query, interface, category, voltage constraints | Candidate parts with match reasons and missing specifications |
| `anode_component_spec` | Catalog ID and revision | Pins, limits, interfaces, footprint and source references |
| `anode_board_pinout` | Exact board ID and revision | Physical headers mapped to GPIO names, functions and restrictions |
| `anode_reference_read` | Reference ID and section/page | Bounded manufacturer documentation excerpt and provenance |
| `anode_components_set` | Circuit revision; instance changes | Add/update/remove labeled instances such as `U1`, `R1`, `J1` |
| `anode_inventory` | List or update owned parts | User-maintained availability, quantities and variants |
| `anode_parts_substitute` | Component instance and constraints | Alternatives, differences, required wiring/code changes and unknowns |
| `anode_calculate` | Calculation kind, typed quantities, assumptions | Results with units, formula and applicability limits |
| `anode_bom` | Circuit revision, include inventory | Quantities, ratings, values, variants, owned/missing parts |

Calculations should cover resistor networks, LED current limiting, dividers,
power dissipation, tolerance ranges, RC time constants, and power budgets.
Keep them deterministic and unit-aware. Reject incompatible units and report
missing inputs; never fill in supply voltage or device ratings from a guess.
Selection must consider ratings and tolerances as well as nominal values.
Do not interpret an absolute maximum rating as a normal operating target.

## Circuit, pin allocation and breadboards

Use a structured electrical graph as the source of truth. A net joins component
pins; wires and breadboard placements explain how the user assembles that net.
Physical header positions, GPIO identifiers, silk labels, and code constants
must be distinct fields with explicit mappings.

A breadboard definition describes holes and internal connectivity, including
terminal strips, the center gap, and continuous or split power rails. Its
orientation and rail topology must be explicit. Placement validation must
detect physical collisions, impossible spans, and leads unintentionally joined
by a common strip.

| Proposed tool | Inputs | Result / effect |
| --- | --- | --- |
| `anode_circuit_get` | Project ID, optional circuit revision | Netlist, instances, power sources, placements and unresolved assumptions |
| `anode_pin_plan` | Board, required interfaces, reserved pins | Candidate pin assignments with conflicts and rationale; no mutation |
| `anode_circuit_patch` | Expected revision; component/net/pin operations | Atomic design changes and new revision |
| `anode_breadboard_layout` | Circuit revision, breadboard model, constraints | Proposed hole placements, jumpers and assembly steps |
| `anode_circuit_validate` | Circuit revision and rule set | Structured findings with severity, evidence, affected nets and limitations |
| `anode_wiring_steps` | Circuit revision, view orientation | Numbered terminal-to-terminal connections with labels and checks |
| `anode_design_code_check` | Circuit and file revisions | Pin constants, interfaces and dependencies that diverge from the circuit |

Validation should check:

- Power-to-ground shorts, incompatible supply/signal domains, missing grounds,
  polarity, unconnected required power pins, and competing power sources.
- Per-pin and aggregate current budgets when documented limits and load
  estimates exist; missing limits or estimates produce an incomplete result.
- Output contention, boot/debug/flash-reserved pins, pin multiplexing conflicts,
  and unavailable ADC/PWM/interface functions on the selected board.
- Bus addresses and configurable alternatives, required pull-ups and their
  voltage domain, and required level shifting.
- LED resistor requirements, component dissipation, driver requirements for
  loads, and protection requirements from the selected component references.
- Breadboard shorts, rail continuity, misplaced leads and inaccessible pins.

Rules operate on documented component models. Validation is static analysis
of the declared design, not proof of physical assembly or electrical safety.
Report `incomplete` when important ratings or topology are missing, even if no
known violation was found.

## Connection diagrams and engineering artifacts

Diagrams must be generated deterministically from the circuit graph. An image
that merely looks plausible is insufficient for wiring instructions.

| Proposed tool | Inputs | Result / effect |
| --- | --- | --- |
| `anode_diagram_render` | Circuit revision; schematic, breadboard, pinout or block view | SVG preview, artifact ID, source revision and validation findings |
| `anode_diagram_inspect` | Artifact ID; component, net or pin | Connected elements, labels, coordinates and related code |
| `anode_diagram_export` | Artifact ID, format, destination | SVG, PNG or PDF plus wiring table and BOM as requested |
| `anode_reference_attach` | Existing local image/document; purpose | Project reference with user notes and unverified observations |
| `anode_artifacts_list` | Project ID and type | Diagrams, BOMs, reports and whether each artifact is stale |

Schematic views explain electrical relationships; breadboard views show exact
hole locations and jumpers; pinout views explain board headers; block views
explain subsystems. Include a legend, board/module orientation, pin labels,
wire endpoint names, power domains, and design revision. Use labels as well as
colors, mark junctions explicitly, and distinguish crossing wires.

Unknown component geometry must show as a labeled connector/placeholder; do
not invent a realistic footprint. Represent blocked or incomplete designs in
previews and exports with visible findings.

Clicking a wire should highlight its net and endpoints; clicking a component
should open its specifications, sources, and relevant code bindings. Moving a
part changes layout without changing electrical connectivity. Reconnecting a
lead changes the circuit and triggers validation.

Prefer an application-owned SVG renderer and standard symbols for the first
release. External EDA and simulation formats require explicit adapters and
round-trip checks before being advertised as supported. Image-capable agents
can receive previews; all agents receive the same netlist and wiring table.

## Build, observe and debug

Reuse Anode's device commands, console events, tool detection, and shared port
leases rather than letting every tool construct its own shell command.

| Proposed tool | Inputs | Result / effect |
| --- | --- | --- |
| `anode_board_status` | Optional target ID | Physical identity, connection generation, runtime and lease state |
| `anode_build` | Project target, source revision | Job ID, diagnostics and build artifact references |
| `anode_run` / `anode_upload` | Target, source/artifact revision | Authorized board job, progress and result |
| `anode_flash_firmware` | Exact target, image and hash | Destructive flash job using existing confirmation and leases |
| `anode_repl_eval` | Target, code, timeout, output limit | Authorized execution with bounded output and tracebacks |
| `anode_bus_probe` | Target, known bus configuration, probe strategy | Active diagnostic job; never treated as passive inspection |
| `anode_output_tail` | Job/session, cursor, limit | Bounded console records with timestamps and truncation state |
| `anode_job_status` / `anode_job_cancel` | Job ID | Progress/result or cancellation acknowledgement |
| `anode_measurements` | Import or read; channel/unit/time range | Measurements with provenance, calibration notes and timestamps |
| `anode_debug_plan` | Symptom, design revision, diagnostics | Ordered checks and expected observations, without executing them |

REPL evaluation and bus probing execute code and can affect attached hardware.
Tie them to a specific target and authorized operation scope. Read-only
inspection and reversible project edits need no additional confirmation.
Reuse an existing valid grant instead of repeatedly prompting; firmware erase
retains Anode's destructive confirmation.

Every job captures physical identity and connection generation. If a device
disconnects or changes identity, stop and report `device_changed`; never retry
against the next device that happens to occupy the same port. Preserve
`PortLeases` and boot-volume leases across all entry points.

Measurements may be user-entered, imported, simulated, or collected from an
instrument. Label these origins separately. Serial logs cannot establish
voltages, current or physical continuity unless an actual measurement source
provides that information. Instrument integration is a future adapter.

## Shared contracts and persistence

Persist portable project engineering data under the project root:

```text
.anode/
  project.json          # Schema version, target, runtime and entry points
  circuit.json          # Instances, nets, pin assignments and source references
  layout.json           # Breadboard model, placements, view coordinates
  components/           # Pinned custom component definitions and provenance
hardware/
  diagrams/             # Exported artifacts, each stamped with circuit revision
  bom.csv
  wiring.md
```

Use JSON as the editable source. BOMs, diagrams and wiring instructions are
derived artifacts. Keep machine-specific ports, authentication and session
grants out of version-controlled project files. Associate a physical device
through local workspace settings.

All tool schemas should define required fields, enums, units, size limits,
and structured results. Common result fields include `project_id`,
`context_revision`, relevant file/circuit revisions, `status`, `data`,
`warnings`, `missing_information`, and source/artifact references.

Mutations require expected revisions and a request ID for retry deduplication.
A stale revision returns `revision_conflict` without partial writes. Design
changes invalidate diagrams and code checks; target changes invalidate pin
plans and target-specific build results. Findings need stable rule IDs,
affected instance/pin/net IDs, evidence and suggested remedies.

Return explicit errors such as `unsupported_board_revision`, `unknown_part`,
`missing_specification`, `pin_conflict`, `port_busy`, `device_changed`,
`permission_denied`, `dependency_missing`, and `timeout`. Long jobs return
handles rather than blocking the UI. Output and reference reads are bounded
and paginated.

The MCP bridge forwards to the running application's command service, so it
sees live buffers and uses the same leases. Use authenticated local IPC scoped
to a workspace/session; a second process must not create its own competing
hardware state. Validate workspace boundaries and symlink escapes for writes.
Apply the same hardware policy for in-app agents and external desktop hosts.

Project instructions, datasheets, photographs and serial output are input
data. They cannot grant tool permissions or override application policy.
Agents that execute arbitrary shell commands remain capable of bypassing
domain tools unless their execution environment is separately restricted;
Anode must not claim its MCP checks sandbox an unrestricted agent.

## Application integration

Keep this as a proposed module split; the existing app currently has no agent,
electrical catalog, circuit graph, breadboard model or diagram renderer.

| Proposed module | Responsibility / existing integration |
| --- | --- |
| `src/agent/context.rs` | Live workspace snapshots and revisions from `Workspace` |
| `src/agent/tools/` | MCP schemas and handlers; dispatch to shared command service |
| `src/engineering/catalog.rs` | Versioned component specifications; links to `src/boards.rs` |
| `src/engineering/circuit.rs` | Instances, nets, pin allocations and atomic edits |
| `src/engineering/breadboard.rs` | Hole connectivity, placement and routing constraints |
| `src/engineering/validation.rs` | Deterministic electrical and geometry rules |
| `src/engineering/calculations.rs` | Unit-aware engineering calculations |
| `src/engineering/diagrams.rs` | Circuit-based rendering and export |
| `src/engineering/storage.rs` | Schema migrations, revisions and portable project files |

The **Agent** tab is a side chat in the same right-hand dock tab group as
**Board**. Users toggle **Board | Agent**, keeping the editor visible. In the
connected experience, users converse with their selected ACP agent here;
streaming replies, tool results, permission requests and diagram previews
belong to this conversation. Agent selection, connection status and a fixed
message composer surround the scrollable conversation. Switching tabs preserves
the draft, conversation and connection; it does not start a new session.

The initial UI provides the tab, agent selector, disconnected state, live
workspace labels and a retained multiline draft. Sending remains disabled until
the ACP transport is implemented. Saved layouts gain the Agent tab without
resetting existing pane arrangements, and the activity bar reopens a closed tab.

The Agent panel presents conversation and tool results. A dockable Circuit
panel provides schematic/breadboard views, components, wiring steps and
validation findings. The Board inspector remains the selected device setup.
Developers must be able to edit the circuit directly without an agent.

## Example: create a sensor project

For “Create a project using my board, a temperature sensor and a status LED”:

1. Read current context and available templates; identify exact board and sensor
   module. Ask for the model/revision only when existing context cannot resolve it.
2. Retrieve verified pinouts and part specifications; list unresolved facts.
3. Propose power domains, bus and GPIO assignments, LED resistor calculation,
   BOM and breadboard connections. Keep unknown ratings visibly unresolved.
4. Validate the circuit, including breadboard topology and the code pin bindings.
5. Create the project from a plan in a fresh directory, with source, dependencies,
   manifests, wiring instructions and diagrams from that circuit revision.
6. Build using the selected target; return compiler diagnostics and missing
   dependencies. Review assembly instructions before powering hardware.
7. Upload within the authorized target scope; observe logs or measurements
   and propose a diagnostic sequence if expected behavior is absent.

No particular pin number, resistor value or supply voltage is universal in this
workflow. Resolve those from the selected parts and documented constraints.

## Delivery order and acceptance

1. **Open-project understanding and creation:** context/read/search/patch,
   targets, templates and create plans. Verify unsaved buffers, concurrent edits,
   project switching, path boundaries and no-overwrite creation.
2. **Engineering data:** exact pinouts, a small verified component catalog,
   units, source provenance, BOM and deterministic calculations. Verify unknown
   parts remain unknown and board revisions cannot be silently substituted.
3. **Circuit and breadboard:** netlist edits, pin planning, topology and validation.
   Exercise shorts, split rails, polarity, incompatible domains, pin conflicts,
   package collisions and incomplete specifications.
4. **Diagrams and firmware consistency:** interactive views, deterministic export,
   wiring steps and code checks. Verify every rendered endpoint maps to the
   netlist and that stale artifacts are clearly identified.
5. **Hardware execution:** existing build/run/upload/flash operations exposed
   through shared jobs. Verify leases, disconnects, cancellation, target identity,
   permission enforcement and retry deduplication before physical writes.
6. **Advanced adapters:** instrument measurements, circuit simulators, external
   EDA import/export and broader component coverage. Advertise capabilities only
   once implemented and validated.

Start with a complete workflow for a small verified board/accessory set.
Expand catalog coverage without pretending that every connected accessory can
be detected or that an unsupported component has a verified electrical model.
