//! UI integration coverage for the agent surfaces: the activity-bar cog opens
//! the Settings screen, Providers lists the configured agents, chat refuses to
//! pick one on its own, and the activity bar switches dock panels instead of
//! stacking them into tabs.
//!
//! These render the real `Workspace` in a headless window and dispatch real
//! pointer events, so a regression in the panel wiring fails here rather than
//! only in a running app.
use anode::Workspace;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, size, AppContext as _, Bounds, TestAppContext, WindowBounds, WindowOptions};
use std::sync::OnceLock;

/// One throwaway config directory for the whole test binary, so no test reads
/// or writes a developer's real Anode settings.
fn config_directory() -> &'static std::path::Path {
    static CONFIG: OnceLock<tempfile::TempDir> = OnceLock::new();
    let directory = CONFIG.get_or_init(|| {
        let directory = tempfile::Builder::new()
            .prefix("anode-ui-test")
            .tempdir()
            .expect("config directory");
        std::env::set_var("ANODE_CONFIG_DIR", directory.path());
        // Never reset a board that happens to be plugged into the
        // machine running the tests.
        std::env::set_var("ANODE_DISABLE_HARDWARE_PROBE", "1");
        directory
    });
    directory.path()
}

/// Open the application window the way `main` does.
fn open_workspace(
    cx: &mut TestAppContext,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Workspace>) {
    open_workspace_with_layout(cx, None)
}

fn open_workspace_with_layout(
    cx: &mut TestAppContext,
    layout: Option<gpui_kit::component::dock::DockAreaState>,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Workspace>) {
    assert!(config_directory().is_dir());
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    // Tall enough that a settings detail pane fits without
                    // scrolling, so tests act on real geometry.
                    size(px(1540.), px(1400.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let workspace = cx.new(|cx| Workspace::new(window, cx));
                workspace.update(cx, |workspace, cx| {
                    workspace.settings.panel_layout = layout;
                    workspace.initialize_panels(window, cx);
                });
                workspace
            },
        )
        .expect("open the Anode window")
    })
}

/// Click a control, then let deferred work settle before the next assertion.
fn click(cx: &mut TestAppContext, handle: gpui_kit::AnyWindowHandle, id: &'static str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
async fn the_cog_opens_settings_as_its_own_screen(cx: &mut TestAppContext) {
    let (handle, _workspace) = open_workspace(cx);
    cx.run_until_parked();

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Settings is a destination, not a panel: nothing of it is on screen.
        assert!(window.try_find("settings-add-provider").is_none());
        assert!(window.try_find("settings-back").is_none());
    })
    .unwrap();

    click(cx, handle, "activity-settings");

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("settings-add-provider").is_some(),
            "the cog should open the Providers page"
        );
        // It replaces the workspace, so it brings its own way back…
        assert!(window.try_find("settings-back").is_some());
        assert!(window.try_find("settings-close").is_some());
        // …and the workspace chrome is gone while it is open.
        assert!(window.try_find("activity-files").is_none());
        // Both built-in agents are listed, whether or not they are installed.
        assert!(window.try_find("provider-codex").is_some());
        assert!(window.try_find("provider-opencode").is_some());
        assert!(window.try_find("provider-toggle-codex").is_some());
        assert_eq!(window.find("provider-codex").label(), Some("Codex"));
        assert_eq!(window.find("provider-codex").checked(), Some(true));
    })
    .unwrap();

    click(cx, handle, "provider-opencode");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("provider-codex").checked(), Some(false));
        assert_eq!(window.find("provider-opencode").checked(), Some(true));
        assert_eq!(window.find("provider-opencode").label(), Some("OpenCode"));
    })
    .unwrap();

    click(cx, handle, "settings-back");

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("activity-files").is_some());
        assert!(window.try_find("settings-add-provider").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn a_legacy_layout_without_agent_still_switches_from_board_to_chat(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::{
        DockAreaState, DockPlacement, DockState, PanelInfo, PanelState,
    };
    let tabs = |names: &[&str], active_index| PanelState {
        panel_name: "TabPanel".into(),
        children: names.iter().map(|name| PanelState::new(*name)).collect(),
        info: PanelInfo::tabs(active_index),
    };
    let board: anode::hardware::RecentBoard = serde_json::from_value(serde_json::json!({
        "key": "settings-review-board",
        "path": "/dev/anode-test-unavailable",
        "name": "Review board",
        "board_id": null,
        "vendor_id": null,
        "product_id": null,
        "serial_number": null,
        "last_connected": 0,
    }))
    .unwrap();
    let mut right = tabs(&["anode.device"], 0);
    right.children.push(PanelState {
        panel_name: "anode.flash".into(),
        children: Vec::new(),
        info: PanelInfo::panel(serde_json::to_value(&board).unwrap()),
    });
    let layout = DockAreaState {
        version: Some(3),
        center: tabs(&["anode.editor"], 0),
        left_dock: Some(DockState::new(
            tabs(&["anode.files", "anode.boards", "anode.catalog"], 0),
            DockPlacement::Left,
            px(360.),
            true,
        )),
        right_dock: Some(DockState::new(right, DockPlacement::Right, px(416.), true)),
        bottom_dock: None,
    };
    let (handle, workspace) = open_workspace_with_layout(cx, Some(layout));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // A reset would return the navigator to 272 px. Prove that the saved
        // layout loaded before exercising its restored board panel.
        assert!(window.find("start-open-folder").bounds().size.width > px(320.));
    })
    .unwrap();
    cx.update(|cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.settings.recent_boards = vec![board];
            cx.notify();
        });
    });
    click(cx, handle, "activity-boards");
    click(cx, handle, "open-board:settings-review-board");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("panel-connect-repl").visible());
    })
    .unwrap();
    cx.update(|cx| {
        assert!(!workspace.read(cx).is_tool_visible(anode::Tool::Device));
        assert!(!workspace.read(cx).is_tool_visible(anode::Tool::Agent));
    });
    click(cx, handle, "activity-agent");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("agent-empty-settings").visible());
        assert!(window.try_find("panel-connect-repl").is_none());
    })
    .unwrap();
    click(cx, handle, "activity-device");
    cx.update(|cx| {
        assert!(workspace.read(cx).is_tool_visible(anode::Tool::Device));
        assert!(!workspace.read(cx).is_tool_visible(anode::Tool::Agent));
    });
    click(cx, handle, "activity-boards");
    click(cx, handle, "activity-boards");
    click(cx, handle, "open-board:settings-review-board");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("panel-connect-repl").visible());
    })
    .unwrap();
    cx.update(|cx| {
        assert!(!workspace.read(cx).is_tool_visible(anode::Tool::Device));
    });
}

#[gpui_kit::test]
async fn the_activity_bar_switches_dock_panels_and_toggles_them_shut(cx: &mut TestAppContext) {
    let (handle, workspace) = open_workspace(cx);
    cx.run_until_parked();

    cx.update(|cx| {
        let workspace = workspace.read(cx);
        // One panel per region is shown at a time, so no region has tabs.
        assert!(workspace.is_tool_visible(anode::Tool::Files));
        assert!(workspace.is_tool_visible(anode::Tool::Device));
        assert!(!workspace.is_tool_visible(anode::Tool::Boards));
        assert!(!workspace.is_tool_visible(anode::Tool::Agent));
    });

    // Switching within a region replaces what it shows.
    click(cx, handle, "activity-agent");
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert!(workspace.is_tool_visible(anode::Tool::Agent));
        assert!(!workspace.is_tool_visible(anode::Tool::Device));
        // A different region is untouched.
        assert!(workspace.is_tool_visible(anode::Tool::Files));
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // The agent panel really is on screen, and the bar still offers the
        // panel it replaced.
        assert!(window.try_find("agent-empty-settings").is_some());
        assert!(window.try_find("activity-device").is_some());
    })
    .unwrap();

    // Clicking the shown panel again closes its region.
    click(cx, handle, "activity-agent");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("agent-empty-settings").is_none(),
            "the right dock should be closed"
        );
    })
    .unwrap();

    // And clicking once more brings it back.
    click(cx, handle, "activity-agent");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("agent-empty-settings").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn adding_a_provider_selects_it_and_offers_removal(cx: &mut TestAppContext) {
    let (handle, workspace) = open_workspace(cx);
    cx.run_until_parked();

    click(cx, handle, "activity-settings");
    let before = cx.update(|cx| workspace.read(cx).configured_agents().len());
    click(cx, handle, "settings-add-provider");

    cx.update(|cx| {
        let agents = workspace.read(cx).configured_agents();
        assert_eq!(agents.len(), before + 1, "the custom agent was added");
        assert!(agents.iter().any(|name| name == "New agent"));
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // A custom agent can be removed, and its environment edited.
        assert!(window.try_find("provider-remove").is_some());
        assert!(window.try_find("env-add").is_some());
        assert!(window.try_find("env-add-openai").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn configuring_a_custom_agent_makes_it_choosable_in_chat(cx: &mut TestAppContext) {
    let (handle, workspace) = open_workspace(cx);
    cx.run_until_parked();

    click(cx, handle, "activity-settings");
    click(cx, handle, "settings-add-provider");

    // A new agent has no command, so it cannot be used in chat yet.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("provider-name", cx);
        window.press("secondary-a", cx);
        window.input("Echo agent", cx);
        window.click("provider-command", cx);
        // An executable that certainly exists, so the probe resolves it.
        window.input("/bin/echo", cx);
        assert_eq!(window.find("provider-command").value(), Some("/bin/echo"));
        window.click("provider-save", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        let agents = workspace.read(cx).configured_agents();
        assert!(
            agents.iter().any(|name| name == "Echo agent"),
            "saving renamed the agent: {agents:?}"
        );
        // Saving does not choose it either.
        assert!(workspace.read(cx).selected_agent().is_none());
    });

    click(cx, handle, "provider-use");

    cx.update(|cx| {
        assert_eq!(
            workspace.read(cx).selected_agent().as_deref(),
            Some("Echo agent"),
            "Use in chat is what selects a provider"
        );
    });

    click(cx, handle, "settings-back");
    click(cx, handle, "activity-agent");

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // The composer replaces the empty state once an agent is chosen.
        assert!(window.try_find("agent-empty-settings").is_none());
        assert!(window.try_find("agent-send").is_some());
        assert!(window.try_find("agent-provider").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn chat_waits_for_an_agent_instead_of_choosing_one(cx: &mut TestAppContext) {
    let (handle, workspace) = open_workspace(cx);
    cx.run_until_parked();

    cx.update(|cx| {
        assert!(
            workspace.read(cx).selected_agent().is_none(),
            "no provider is selected until the user picks one"
        );
    });

    click(cx, handle, "activity-agent");

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // The empty state offers the one action that resolves it.
        assert!(
            window.try_find("agent-empty-settings").is_some(),
            "chat should send an undecided user to Settings"
        );
        // Nothing can be sent while there is no agent.
        assert!(window.try_find("agent-send").is_none());
    })
    .unwrap();

    click(cx, handle, "agent-empty-settings");

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("settings-add-provider").is_some());
    })
    .unwrap();

    cx.update(|cx| {
        // Opening Settings still does not choose an agent.
        assert!(workspace.read(cx).selected_agent().is_none());
    });
}

#[gpui_kit::test]
async fn provider_environment_and_save_fit_the_minimum_window(cx: &mut TestAppContext) {
    let (handle, _workspace) = open_workspace(cx);
    cx.run_until_parked();
    click(cx, handle, "activity-settings");
    click(cx, handle, "settings-add-provider");
    click(cx, handle, "env-add-openai");
    cx.simulate_window_resize(handle, size(px(1180.), px(700.)));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.scroll(
            "provider-command",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-800.))),
            cx,
        );
        let save = window.find("provider-save");
        assert!(save.visible());
        assert!(save.bounds().right() <= px(1180.));
        assert!(save.bounds().bottom() <= px(700.));
        let name = window.find("env-name-0");
        let value = window.find("env-value-0");
        let remove = window.find("env-remove-0");
        assert!(value.bounds().size.width >= px(120.));
        assert!(name.bounds().right() <= value.bounds().left());
        assert!(value.bounds().right() <= remove.bounds().left());
        assert!(remove.bounds().right() <= px(1180.));
        // A real click must reach Save after the environment rows are added.
        window.click("provider-save", cx);
    })
    .unwrap();
}
