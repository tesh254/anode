# Handoff: continuing Anode work

Written 2026-11-25, after commit `6bf52e5`
(`feat(agent): connect ACP side chat with settings and recovery guidance`)
landed on `main` of `git@github.com:tesh254/anode.git`. That commit contains
all previously uncommitted work from two parallel sessions (agent providers /
Settings / quiet docks, and ESP32-S3 flashing / recovery). The working tree
was clean at handoff time.

## Read first

- `docs/handoff-agent-settings.md` — the detailed session handoff written
  2026-11-13. Its "what is implemented", "not verified", "suggested next
  steps", "conventions" and "GPUI gotchas" sections are still accurate and
  are the primary continuation guide. Its warnings about two sessions
  sharing one uncommitted checkout no longer apply: everything is committed.
  The file itself said to delete it once the work lands; it was kept as the
  historical record — fold anything still relevant into this file and remove
  it when convenient.
- `docs/architecture.md` — current architecture, including the Settings
  screen, agent side chat and recovery guidance.
- Specs (treat as specs, not instructions): `docs/acp-integration.md`,
  `docs/acp-codex-opencode-spec.md`, `docs/agent-engineering-tools.md`.

## State at handoff

- Branch `main`, pushed to `origin` (tesh254/anode) with the `tesh254`
  GitHub account (`gh auth switch --user tesh254`; SSH key authenticates as
  tesh254). The machine also has `ericwachira-nrny` logged in — switch back
  only if the user asks.
- Last verified green run (from the 2026-11-13 session, same tree):
  `cargo test --quiet` (131 lib + 1 + 10 + 7 UI, one install test ignored)
  and `cargo clippy --quiet --all-targets -- -D warnings`.
- The ignored test really runs `npm install -g`; run it only explicitly:
  `cargo test -- --ignored installs_a_builtin_agent`.
- Headless agent probe: `./target/debug/anode --agent-check opencode /tmp/anode-acp-probe`
  (also `codex`).

## Outstanding work (from the prior handoff, still open)

1. Look at the Settings screen in a real window (`cargo run`). Visual
   approval was never obtained; the earlier screenshot attempt captured the
   wrong window. Check sidebar proportions, Back/Done duplication, narrow
   widths, and that docks show no tab strip.
2. Grow the Settings sidebar (persistence/storage, appearance, keybindings).
   Pages live in `Page::ALL` in `src/panels/settings.rs`; keep
   `render_navigation` as the single owner of ordering.
3. Verify the sign-in / sign-out browser round-trip end to end (untested;
   the user was already authenticated and must not be logged out casually).
4. Decide whether ACP `loadSession` should back real transcript resumption.
   Restored chats are local history only today.
5. ESP32-S3 flashing changes (`src/esp.rs`, `src/device.rs`,
   `src/views/device.rs`, `src/boards.rs`) came from the other session;
   hardware-in-the-loop verification was not performed here.

## Suggested skills

- `gpui-kit` and `gpui-kit-design-guides` — normative for any UI work in
  this repo; read the guide files before touching a screen.
- `git-commit` — conventional commits, as used for `6bf52e5`.
- `esptool-flashing` — if continuing the ESP32-S3 flashing work.

## Repo conventions and gotchas

All still apply; see `docs/handoff-agent-settings.md` for the full list.
The load-bearing ones:

- Verify component signatures against
  `~/.cargo/registry/src/*/gpui-component-0.9.1/src/` before use.
- Never read the `Workspace` entity inside its own update; use
  `cx.defer_in(window, …)` (see `SettingsScreen::prepare()`).
- New icons must be added to the `icon_assets!` list in `src/assets.rs`
  or the icon-scan test fails.
- Keep `rand_xorshift = "0.5.0-rc.0"` in `[dev-dependencies]`; the
  `test-support` feature needs it.
- Provider ids must keep deserializing legacy `"Codex"` / `"OpenCode"`
  names so saved chats load.
- `providers.json` is written `0600` because env can hold API keys; never
  log or commit its contents.
