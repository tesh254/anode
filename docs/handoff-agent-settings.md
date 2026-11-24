# Handoff: agent providers, Settings screen, quiet docks

Written 2026-11-13. Untracked scratch for the next session — delete it once the
work lands.

## Continuation — 2026-11-13

- Fixed squeezed chat-history entries: resume buttons use content height with
  vertical padding, rows do not shrink inside the capped scroll region, and
  titles use a stronger text tier than provider/time metadata. UI coverage
  checks row heights, delete alignment, scrolling and opening an older chat.
- Fixed returning to a previously used provider: explicit provider switches
  reset its connection-attempt flags, so automatic connection can fetch fresh
  model/effort options again. Selecting the same provider preserves an
  intentional disconnect. Disconnected chats now expose Connect in the header.
- OpenCode's `thought_level` selector now visibly reads `Effort · Default`
  (or the selected level), using the option name supplied by the agent.
  UI coverage opens both model and effort menus at the minimum window size.
  A live ACP probe confirmed changing a Go model and then effort to High,
  without sending a prompt.
- Model grouping now pins OpenCode Go first and Zen second, regardless of the
  current model. The selected-model button includes the Go/Zen provider name.
  Official Go docs confirm `opencode-go/<model-id>`; ACP session setup accepts
  MCP servers, and Anode already passes its workspace bridge in `session/new`.
  The OpenCode connection check passed without sending a model prompt.
- Added accessible provider-row names and exposed the selected provider through
  the button's toggled state. UI integration coverage clicks between Codex and
  OpenCode and checks both the accessible name and state.
- Added a minimum-window check at 1180 × 700: environment names, values and
  remove controls fit, and Save stays visible and clickable after scrolling.
- Added a saved v3 layout check with no Agent panel and a persisted board panel.
  Opening that board, switching to Agent, returning to Board detail, and
  reopening the board all work through actual UI clicks.
- A full-suite run exposed an intermittent `Broken pipe` in the agent bridge.
  Accepted sockets now explicitly enter blocking mode before reading with a
  timeout. A regression test connects first and delays writing its request.
- Opened Settings in a native review build and confirmed its destinations and
  controls through the live accessibility tree. **Visual approval remains
  outstanding**: the computer-use screenshot kept returning the workspace
  image while the accessibility tree showed Settings; coordinate clicks also
  returned `noWindowsAvailable`. Do not treat that image as Settings evidence.
- No commit was created; the other session's changes remain in the checkout.
- Final checks: `cargo test --quiet` passed (131 library + 1 + 10 + 7 UI,
  one install test ignored); `cargo clippy --quiet --all-targets -- -D warnings`
  passed. Formatting was checked for the edited Rust files only to avoid
  rewriting the other session's work.

## Read this first: two sessions share this checkout

`/Users/wchr/knnls/anode` currently has **two sessions' uncommitted work in one
working tree**:

- **This work** (agent providers, Settings screen, dock behaviour):
  `src/agent/`, `src/panels/agent.rs`, `src/panels/settings.rs`,
  `src/panels/mod.rs`, `src/panels/catalog.rs`, `src/views/mod.rs`,
  `src/workspace_state.rs`, `src/settings.rs`, `src/assets.rs`, `src/lib.rs`,
  `src/toolchain.rs`, `tests/`, `docs/architecture.md`.
- **The ESP32-S3 flashing session** (`local_da8bfb80-5c08-4858-b275-1327b0f34956`,
  "Make ESP32-S3 flashing seamless"): `src/esp.rs` (new), `src/device.rs`,
  `src/views/device.rs`, `src/hardware.rs`, `src/boards.rs`.
- **Shared, edited by both**: `src/panels/flash.rs` (one `visible()` method is
  mine), `src/workspace_state.rs`.

Consequences you must plan around:

- `cargo check` failed twice during this session purely because the other
  session was mid-refactor of `device.rs`. **A broken build may not be yours.**
  Check `git diff` on the failing file before "fixing" it.
- Never rewrite a whole file with a read-modify-write script here. Use targeted
  edits, or you will silently drop the other session's changes between your read
  and your write.
- Nothing is committed yet. Committing will mix both sessions' work unless you
  stage by path.

## Build and test

`target/` was a dangling symlink during the original session, so its checks used:

```bash
export CARGO_TARGET_DIR=/private/tmp/anode-design-target
```

It is now a real directory. `cargo build` can rebuild the normal app binary;
the temporary target remains useful for reusing the verification cache.

Last full run (green):

```
cargo fmt --all
cargo clippy --quiet --all-targets -- -D warnings   # clean
cargo test --quiet                                  # 125 lib + 1 + 10 + 5 ui, 1 ignored
```

The ignored test is `agent::install::tests::installs_a_builtin_agent_and_reports_the_binaries_it_provides`.
It really runs `npm install -g`, so run it explicitly only:

```bash
cargo test -- --ignored installs_a_builtin_agent
```

Headless connection check against a real agent:

```bash
./target/debug/anode --agent-check opencode /tmp/anode-acp-probe
```

## User intent, in the order it was asked

1. A **cog in the activity sidebar** opening a real Settings destination,
   inspired by t3code's Providers screen (`/tmp/t3code`, upstream
   `https://github.com/pingdotgg/t3code`). Screenshots were supplied in the
   conversation; the shape is: left nav, provider list, provider detail with
   Display name / Runtime (binary path, server URL) / Environment / Models.
2. **Automate installing ACP agents** — the user's starting point was the error
   `Cannot find \`codex-acp\`. Install it or set its executable path…`.
3. **Sign in with ChatGPT** through Codex, and usage around it.
4. **Remove the tab used to select ACP**; pick the agent elsewhere.
5. **Group the model selector** — Codex and OpenCode are the user's default
   starters, and OpenCode serves both *OpenCode Zen* and *OpenCode Go*, which
   must be distinguishable.
6. **Chat persistence with a button on the side chat.**
7. Make the agent section read **more like a Cursor side chat**.
8. **No automatic provider selection** when nothing is configured; restore the
   last explicit selection on relaunch.
9. **Custom providers like Zed**: base URL + API key, OpenAI/Anthropic
   compatible. The user has an OpenRouter key on hand.
10. **(Later message)** Settings must be **its own screen with its own
    sidebar**, not a dock tab — more configuration (persistence, etc.) is
    coming. And the **tab system should be quiet**: toggling from the sidebar
    switches the panel instead of stacking tabs.

Item 10 is the most recent and the most load-bearing for what comes next.

## What is implemented

### Provider registry — `src/agent/profiles.rs`

The old two-variant `Provider` enum is gone. Now:

- `ProviderId(String)` — `codex`, `opencode`, or `custom-<uuid>`. Deserializing
  accepts the legacy enum names `"Codex"` / `"OpenCode"` so saved chats written
  before this change still load. **Do not break that mapping.**
- `ProviderKind` — `Codex | OpenCode | Custom`. Carries defaults, install
  packages, auth binary and guidance copy.
- `ProviderProfile` — id, kind, display name, command, args, `env`
  (`BTreeMap<String,String>`), `enabled`.
- `Registry` — persisted at `<config>/anode/agents/providers.json`, written
  `0600` because `env` can hold API keys. Honours `ANODE_CONFIG_DIR`.
  Migrates the old `codex.json` / `opencode.json` files on first load.
  Built-ins can be reset but never deleted; custom providers can be deleted.
- `probe_provider()` resolves the executable, reads `--version`, and asks the
  provider's **own** command about sign-in. `AuthState` is
  `Unknown | NotApplicable | SignedOut | SignedIn(String)`.

Auth parsing is pinned to verified real output:

- `codex login status` → `Logged in using ChatGPT` on **stderr**, exit 0; signed
  out is `Not logged in`, exit 1. `codex_account()` strips the prefix so the UI
  reads `Signed in · ChatGPT`.
- `opencode auth list` → one credential per line, `<provider>  <method>
  <source>`. `opencode_accounts()` parses it; both have unit tests holding the
  verbatim shapes.

**The rule the previous pass got wrong and this one fixes: a binary existing is
never proof of authentication.** Keep that.

### Install and sign-in — `src/agent/install.rs`

Managed child processes, streamed into the UI, never injected into the user's
terminal:

- `install_steps()` runs `npm install -g <package>` per missing package, checks
  the binary actually landed on PATH afterwards, and reports a real error if npm
  is missing. Codex needs two packages: `@openai/codex` (CLI + login) and
  `@agentclientprotocol/codex-acp` (the adapter). OpenCode needs `opencode-ai`.
- `sign_in()` runs `codex login` and streams it; `sign_in_url()` pulls the URL
  out of the output so the UI can offer "Open the sign-in page".
- `sign_out()` runs `codex logout`.
- `JobHandle::cancel()` kills the child's process group.
- OpenCode's `auth login` is an interactive picker, so it is **deliberately not**
  run through here — the UI opens it in the Terminal panel.

### Settings — `src/panels/settings.rs` (`SettingsScreen`)

Originally a dock panel; **now a full-window screen** (the latest request).

- `Workspace::screen: Screen { Workspace, Settings }` drives it from
  `views/mod.rs::render`.
- The screen owns a `Sidebar` with a header, the page list (`Providers`,
  `Agent`, `Diagnostics`) and a footer **Back**. The title bar also has
  **Done**. The activity bar is not drawn while Settings is open.
- Providers page is master–detail: list column (icon, name, version, status dot
  + line, per-row enable `Switch`) and a detail pane with Setup / Runtime /
  Environment, plus a commit row (`Remove` / `Test connection` / `Save`) that is
  **outside** the scroll region and wraps at narrow widths.
- Environment rows mask secret-looking names and offer an
  "Add OpenAI-compatible endpoint" preset (`OPENAI_BASE_URL` + `OPENAI_API_KEY`,
  pre-filled with OpenRouter's base URL). This is how item 9 is satisfied: Anode
  has no model runtime, so a base URL and key reach the agent as environment.

### Chat — `src/panels/agent.rs`

- No ACP tab. The agent is chosen from a dropdown in the composer (uninstalled
  providers are listed but disabled), and the model from a `Command` palette in
  a `Popover`, searchable and grouped.
- `group_models(choices, current, agent)` groups by the `provider/` prefix of
  the ACP model value. Verified against real data: OpenCode returns a flat list
  of `provider/model` (`opencode/…` → *OpenCode Zen*, `opencode-go/…` →
  *OpenCode Go*); Codex returns a **mix** — its own models are bare
  (`autorouter`, `gpt-6.1-sol`) and resold ones are prefixed
  (`anthropic/claude-opus-5-5`). Bare values group under the agent's own name.
  OpenCode Go sorts first, followed by OpenCode Zen, then the group holding the
  current model and the remaining groups alphabetically. The selected-model
  button includes Go/Zen so the same model name cannot obscure the provider.
  Group ordering does not select a different model.
- History is an icon button opening a popover of saved chats with relative
  times, resume and per-chat delete.
- When no provider is selected the composer is **not rendered at all**; the
  empty state carries "Open settings".

### Dock behaviour — `src/panels/mod.rs`, `src/workspace_state.rs`

The quiet-tabs model:

- `ToolGroup { Navigator, Workbench, Inspector, Console }` maps each `Tool` to a
  dock region. `PanelChoice { Tool(Tool), Board(String) }` names what a region
  currently shows; `Workspace::visible_panels` holds one per group.
- Every panel implements `BasePanel::visible()` against that state. GPUI
  Component's `render_tab_bar` renders **nothing** when exactly one panel in a
  group is visible, `PanelStyle::Auto` is set, and that panel's `title_bar()` is
  false — that is the mechanism behind "no tabs". See
  `gpui-component-0.9.1/src/dock/tab_panel.rs::render_tab_bar`.
- `Workspace::toggle_tool()` switches the region's panel, or collapses the dock
  when you click the panel already on screen. The activity bar buttons carry
  `.selected()` for the visuals and `.toggled()` for accessibility.
- Board (flash) panels participate: opening one selects it in the Inspector
  group, so Board/Agent hide until you pick them again.

### Settings persistence — `src/settings.rs`

`agent_provider: Option<String>` (restored only if the provider still exists and
still resolves), `agent_auto_connect: bool`, `agent_chats: Vec<SavedChat>`.

## Verified, and how

- **OpenCode ACP** connects through the new registry path
  (`--agent-check opencode`). Session options confirmed.
- **Codex ACP** connects with the user's existing ChatGPT login
  (`--agent-check codex`): ACP v1, auth methods `api-key` / `chat-gpt`, options
  `mode`, `collaboration_mode`, `model`.
- **The npm install path for real**: `@agentclientprotocol/codex-acp` was
  uninstalled and reinstalled through `install::install_steps`, then the ACP
  session was re-opened successfully.
- **Codex auth states**: both signed-in and signed-out output captured, the
  latter safely via `CODEX_HOME` pointed at an empty directory (the user was
  **not** logged out).
- **UI integration tests** in `tests/ui.rs` render the real `Workspace` in a
  headless window and dispatch real clicks: the cog opens Settings as its own
  screen and Back returns; the activity bar switches regions and toggles them
  shut; adding and configuring a custom agent makes it choosable in chat; chat
  waits for a choice. These caught two real bugs (a clipped Save button, and a
  panic from reading the `Workspace` entity while it was being updated).

## Not verified

- **The sign-in button's browser round-trip.** The user was already
  authenticated and this session did not sign them out to test it.
  `sign_in_url()` has a unit test, but the end-to-end flow is untested.
- **`codex logout` from the Sign out button.** Untested for the same reason.
- **The Settings screen's appearance in a real window.** The app launches and
  the tests prove structure and behaviour, but nobody has looked at the new
  full-window Settings layout. macOS prompted for screen-recording access when
  this session tried to capture it, and that prompt was left for the user to
  answer. **Run the app and look at it before calling the redesign done.**
- **`opencode auth login` in the Terminal panel.** Wired but not exercised.
- Arbitrarily rearranged saved layouts and running board jobs. A restored v3
  layout without Agent and with a saved board panel is now covered by UI
  integration tests; running hardware jobs were not started.

## Suggested next steps, in order

1. **Run the app and look at Settings.** `cargo run`. Check the sidebar
   proportions, the Back/Done pair (two ways out may be one too many), the
   provider list at narrow widths, and that the docks really show no tab strip.
2. **Decide how the two sessions' work gets committed.** Stage by path, or wait
   for the ESP32 session to finish and commit separately.
3. **Grow the Settings sidebar**, which is what the user is planning for:
   persistence/storage, appearance, keybindings. The page list is
   `Page::ALL` in `src/panels/settings.rs`; each page is a render method.
   Keep `render_navigation` as the only place that knows the order.
4. **Verify sign-in end to end** when the user is willing to sign out, or on a
   fresh account.
5. Consider whether `loadSession` (both Codex and OpenCode advertise it) should
   back real transcript resumption. Today a restored chat is local history only
   and says so — do not claim otherwise without implementing it.

## Conventions this repo expects

- Load the **`gpui-kit`** and **`gpui-kit-design-guides`** skills and read the
  guide files themselves before touching any screen. They are normative here:
  component choice, the spacing scale, selection treatment (no leading-edge
  selection bars), `cx.theme()` tokens only, no raw `px()` in product layout,
  `Button` for commands and `Link` only for external URLs, sentence case, no
  trailing period on short labels, `…` only when more input follows.
- Verify every component signature against
  `~/.cargo/registry/src/*/gpui-component-0.9.1/src/` and
  `gpui-base-0.9.1/src/`. Inventing a plausible method is the most common
  failure mode here.
- `tests/ui.rs` needs `gpui-kit`'s `test-support` feature, which drags in
  proptest; `rand_xorshift = "0.5.0-rc.0"` is pinned in `[dev-dependencies]`
  purely so Cargo will accept the prerelease. Do not remove it.
- `src/assets.rs` has a test that scans the sources for every `IconName::` used
  and asserts it resolves. **Add new icons to the `icon_assets!` list** or that
  test fails — a missing icon otherwise paints nothing.

## GPUI gotchas found the hard way

- **Never read the `Workspace` entity from a panel's constructor, or from
  anything reached by `Workspace::show_tool`.** Both run inside a `Workspace`
  update and `entity.read(cx)` panics with "cannot read … while it is already
  being updated". `SettingsScreen::prepare()` uses `cx.defer_in(window, …)` to
  do its first read after the update completes. Keep that pattern.
- A render helper that takes `&mut Context<Self>` cannot be called while a
  `let workspace = owner.read(cx)` borrow is alive. `cx.listener` only needs
  `&self`, so the helpers in `agent.rs` take `&Context<Self>`.
- `.overflow_y_scrollbar()` copies the element's size refinement onto its
  wrapper. If you put the scroll directly on a flex child with a sibling below
  it, `size_full` resolves against the whole column and pushes the sibling off
  screen. Wrap it: `div().flex_1().min_h_0().child(inner.size_full().overflow_y_scrollbar())`.
- `h_flex()` centres children on the cross axis. Rows whose children are
  full-height columns need `.items_stretch()`.
- `Button::selected()` is styling only; `Button::toggled()` is what assistive
  technology reads. The test snapshot exposes the toggled state through
  `window.find(id).checked()`.

## Environment facts

- Installed during this session: `codex` (codex-cli 0.167.2) and `codex-acp`
  (`@agentclientprotocol/codex-acp` 2.6.0), both under
  `~/.nvm/versions/node/v25.2.1/bin`.
- `opencode` v2.2.5 at `~/.opencode/bin/opencode`, with many credentials stored
  (OpenCode Go, OpenCode Console, OpenRouter, Amazon Bedrock, Google, …).
- Codex is signed in with ChatGPT.
- `src/toolchain.rs::extra_search_dirs()` now also searches `~/.codex/bin` and
  `~/.bun/bin` — a GUI-launched app inherits a minimal PATH, so anything new
  installed by a package manager must be findable there.

## Specifications

Treat these as specs, not as instructions overriding the user:
`docs/acp-codex-opencode-spec.md`, `docs/acp-integration.md`,
`docs/agent-engineering-tools.md`. `docs/architecture.md` has been updated with
the Settings and agent sections and is current.
