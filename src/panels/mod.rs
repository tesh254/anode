//! Dockable workspace tools. Each tool owns retained focus and lifecycle;
//! device panels additionally own their target and background jobs.
mod agent;
mod catalog;
mod flash;
mod settings;

use crate::hardware::{PortLeases, RecentBoard};
use crate::workspace_state::Workspace;
use agent::AgentPanel;
use catalog::CatalogPanel;
use flash::FlashPanel;
use gpui_kit::component::dock::{
    panel_handle, register_panel, BasePanel, DockArea, DockAreaState, DockEvent, DockLayout,
    DockPlacement, DockSkin, InsertTarget, NodeId, Panel, PanelEvent, PanelId, PanelInfo,
    PanelStyle,
};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, WeakEntity, Window,
};
use settings::SettingsScreen;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tool {
    Files,
    Boards,
    Catalog,
    Editor,
    Output,
    Terminal,
    Repl,
    Device,
    Agent,
}

/// The region of the window a panel lives in. One panel per group is shown at
/// a time, chosen from the activity bar, so the docks carry no tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolGroup {
    /// The left dock: project navigation.
    Navigator,
    /// The center: the editor.
    Workbench,
    /// The right dock: the board, the agent, and board tasks.
    Inspector,
    /// The bottom dock: output, REPL and terminal.
    Console,
}

/// Which panel a group is currently showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PanelChoice {
    Tool(Tool),
    /// A board task panel, keyed by its [`RecentBoard`] key.
    Board(String),
}

impl PanelChoice {
    pub(crate) fn group(&self) -> ToolGroup {
        match self {
            Self::Tool(tool) => tool.group(),
            Self::Board(_) => ToolGroup::Inspector,
        }
    }
}

impl Tool {
    /// Every tool that renders through the shared [`ToolPanel`]. `Catalog`
    /// owns its own entity, so it is deliberately not here.
    const SHARED: &'static [Tool] = &[
        Tool::Files,
        Tool::Boards,
        Tool::Editor,
        Tool::Output,
        Tool::Terminal,
        Tool::Repl,
        Tool::Device,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Boards => "Boards",
            Self::Catalog => "Catalog",
            Self::Editor => "Editor",
            Self::Output => "Output",
            Self::Terminal => "Terminal",
            Self::Repl => "REPL",
            Self::Device => "Board",
            Self::Agent => "Agent",
        }
    }

    pub fn group(self) -> ToolGroup {
        match self {
            Self::Files | Self::Boards | Self::Catalog => ToolGroup::Navigator,
            Self::Editor => ToolGroup::Workbench,
            Self::Output | Self::Repl | Self::Terminal => ToolGroup::Console,
            Self::Device | Self::Agent => ToolGroup::Inspector,
        }
    }

    fn registry_name(self) -> &'static str {
        match self {
            Self::Files => "anode.files",
            Self::Boards => "anode.boards",
            Self::Catalog => "anode.catalog",
            Self::Editor => "anode.editor",
            Self::Output => "anode.output",
            Self::Terminal => "anode.terminal",
            Self::Repl => "anode.repl",
            Self::Device => "anode.device",
            Self::Agent => "anode.agent",
        }
    }

    fn placement(self) -> DockPlacement {
        match self.group() {
            ToolGroup::Navigator => DockPlacement::Left,
            ToolGroup::Workbench => DockPlacement::Center,
            ToolGroup::Console => DockPlacement::Bottom,
            ToolGroup::Inspector => DockPlacement::Right,
        }
    }

    /// True for the tools that share the bottom console tab group.
    fn is_console(self) -> bool {
        self.group() == ToolGroup::Console
    }
}

struct ToolPanel {
    tool: Tool,
    owner: WeakEntity<Workspace>,
    focus: FocusHandle,
    _subscription: Option<Subscription>,
}

impl ToolPanel {
    fn new(tool: Tool, owner: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let subscription = owner
            .upgrade()
            .map(|owner| cx.observe(&owner, |_, _, cx| cx.notify()));
        Self {
            tool,
            owner,
            focus: cx.focus_handle(),
            _subscription: subscription,
        }
    }
}

impl EventEmitter<PanelEvent> for ToolPanel {}

impl Focusable for ToolPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.tool == Tool::Terminal {
            if let Some(owner) = self.owner.upgrade() {
                return owner.read(cx).terminal.read(cx).focus_handle(cx);
            }
        }
        self.focus.clone()
    }
}

impl BasePanel for ToolPanel {
    fn panel_name(&self) -> &'static str {
        self.tool.registry_name()
    }
    fn closable(&self, _: &App) -> bool {
        self.tool != Tool::Editor
    }
    /// One panel per group is shown at a time, chosen from the activity bar.
    /// Hiding the rest is what leaves the dock without a tab strip.
    fn visible(&self, cx: &App) -> bool {
        self.owner
            .upgrade()
            .is_none_or(|owner| owner.read(cx).is_tool_visible(self.tool))
    }
}

impl Panel for ToolPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.tool.name()
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(self.tool.name().into())
    }
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
    fn title_bar(&self, _: &App) -> bool {
        // These three draw their own header row, so a second one would be a
        // duplicate boundary.
        !matches!(self.tool, Tool::Editor | Tool::Files | Tool::Device)
    }
}

impl Render for ToolPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = self.tool;
        let content = self
            .owner
            .update(cx, |workspace, cx| match tool {
                Tool::Files => workspace.render_sidebar(cx).into_any_element(),
                Tool::Boards => workspace.render_recent_boards(cx).into_any_element(),
                Tool::Editor => gpui_kit::base::v_flex()
                    .size_full()
                    .min_h_0()
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().border)
                    .overflow_hidden()
                    .child(workspace.render_editor_tabs(window, cx))
                    .child(workspace.render_editor(cx))
                    .into_any_element(),
                Tool::Output => workspace.render_output(cx).into_any_element(),
                Tool::Terminal => workspace.terminal.clone().into_any_element(),
                Tool::Repl => workspace.render_repl(cx).into_any_element(),
                Tool::Device => workspace
                    .render_device_inspector(window, cx)
                    .into_any_element(),
                // Rendered by panels that own their own state.
                Tool::Catalog | Tool::Agent => div().into_any_element(),
            })
            .unwrap_or_else(|_| div().into_any_element());
        gpui_kit::base::v_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .when(tool == Tool::Editor, |this| {
                this.p_2().bg(cx.theme().sidebar)
            })
            .child(content)
    }
}

type FlashPanels = Rc<RefCell<BTreeMap<String, WeakEntity<FlashPanel>>>>;

pub(crate) struct PanelWorkspace {
    pub(crate) dock: Entity<DockArea>,
    tools: BTreeMap<Tool, Entity<ToolPanel>>,
    catalog: Entity<CatalogPanel>,
    agent: Entity<AgentPanel>,
    settings: Entity<SettingsScreen>,
    flashes: FlashPanels,
    owner: WeakEntity<Workspace>,
    leases: PortLeases,
}

impl PanelWorkspace {
    fn new(
        owner: WeakEntity<Workspace>,
        leases: PortLeases,
        saved: Option<DockAreaState>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let (dock, skin) = DockSkin::dock_area("anode-workspace", Some(3), window, cx);
        skin.set_panel_style(PanelStyle::Auto, cx);
        skin.set_close_button_visible(true, cx);

        let mut tools = BTreeMap::new();
        for tool in Tool::SHARED.iter().copied() {
            let panel = cx.new(|cx| ToolPanel::new(tool, owner.clone(), cx));
            let restored = panel.clone();
            register_panel(cx, tool.registry_name(), move |_, _, _| {
                panel_handle(restored.clone())
            });
            tools.insert(tool, panel);
        }

        let catalog = cx.new(|cx| CatalogPanel::new(owner.clone(), window, cx));
        let restored_catalog = catalog.clone();
        register_panel(cx, Tool::Catalog.registry_name(), move |_, _, _| {
            panel_handle(restored_catalog.clone())
        });

        let agent = cx.new(|cx| AgentPanel::new(owner.clone(), window, cx));
        let restored_agent = agent.clone();
        register_panel(cx, Tool::Agent.registry_name(), move |_, _, _| {
            panel_handle(restored_agent.clone())
        });

        // Settings is a full-window destination, not a dock panel, so it is
        // built here but never registered with the dock.
        let settings = cx.new(|cx| SettingsScreen::new(owner.clone(), window, cx));

        let flashes: FlashPanels = Rc::default();
        let restored_flashes = flashes.clone();
        let restored_owner = owner.clone();
        let restored_leases = leases.clone();
        register_panel(cx, "anode.flash", move |context, window, cx| {
            let record = match context.info() {
                PanelInfo::Panel(value) => {
                    serde_json::from_value::<RecentBoard>(value.clone()).ok()
                }
                _ => None,
            }
            .unwrap_or_else(|| RecentBoard {
                key: "unavailable".into(),
                path: String::new(),
                name: "Unavailable board".into(),
                board_id: None,
                vendor_id: None,
                product_id: None,
                serial_number: None,
                last_connected: 0,
            });
            let key = record.key.clone();
            let panel = cx.new(|cx| {
                FlashPanel::new(
                    record,
                    restored_owner.clone(),
                    restored_leases.clone(),
                    window,
                    cx,
                )
            });
            restored_flashes.borrow_mut().insert(key, panel.downgrade());
            panel_handle(panel)
        });

        let this = Self {
            dock,
            tools,
            catalog,
            agent,
            settings,
            flashes,
            owner,
            leases,
        };
        // Version 3 added the catalog panel; an older saved layout is dropped
        // rather than restored without it.
        let restored = saved
            .filter(|state| state.version == Some(3))
            .is_some_and(|state| {
                this.dock
                    .update(cx, |dock, cx| dock.load(state, window, cx).is_ok())
            });
        if !restored {
            this.reset_layout(window, cx);
        } else {
            this.ensure_agent_tab(window, cx);
        }
        this
    }

    fn reset_layout(&self, window: &mut Window, cx: &mut App) {
        // Retain all board panels and any running jobs when resetting tools.
        let boards: Vec<_> = self
            .flashes
            .borrow()
            .values()
            .filter_map(WeakEntity::upgrade)
            .collect();
        let rem = window.rem_size();
        let tabs =
            |tool| DockLayout::tabs().panel_view(panel_handle(self.tools[&tool].clone()), cx);
        let bottom = DockLayout::tabs()
            .panel_view(panel_handle(self.tools[&Tool::Output].clone()), cx)
            .panel_view(panel_handle(self.tools[&Tool::Repl].clone()), cx)
            .panel_view(panel_handle(self.tools[&Tool::Terminal].clone()), cx);
        let center = DockLayout::v_split()
            .child(tabs(Tool::Editor), None)
            .child(bottom, Some(rem * 15.0));
        let left = DockLayout::tabs()
            .panel_view(panel_handle(self.tools[&Tool::Files].clone()), cx)
            .panel_view(panel_handle(self.tools[&Tool::Boards].clone()), cx)
            .panel_view(panel_handle(self.catalog.clone()), cx);
        let right = tabs(Tool::Device).panel_view(panel_handle(self.agent.clone()), cx);
        self.dock.update(cx, |dock, cx| {
            dock.set_center(center, window, cx);
            if dock.has_dock(DockPlacement::Bottom) {
                dock.remove_dock(DockPlacement::Bottom, window, cx);
            }
            dock.set_dock(DockPlacement::Left, left, window, cx);
            dock.set_dock_size(DockPlacement::Left, rem * 17.0, window, cx);
            dock.set_dock(DockPlacement::Right, right, window, cx);
            dock.set_dock_size(DockPlacement::Right, rem * 26.0, window, cx);
            for placement in [DockPlacement::Left, DockPlacement::Right] {
                if !dock.is_dock_open(placement) {
                    dock.toggle_dock(placement, window, cx);
                }
            }
            for board in boards {
                dock.add_panel_view(
                    panel_handle(board),
                    DockPlacement::Right,
                    Some(rem * 26.0),
                    window,
                    cx,
                );
            }
        });
    }

    /// Add the new tab to saved layouts without resetting the user's panes.
    fn ensure_agent_tab(&self, window: &mut Window, cx: &mut App) {
        let id = PanelId::from(self.agent.entity_id());
        self.dock.update(cx, |dock, cx| {
            if dock.panel(id).is_some() {
                return;
            }
            let board = PanelId::from(self.tools[&Tool::Device].entity_id());
            let node = [
                DockPlacement::Right,
                DockPlacement::Center,
                DockPlacement::Left,
                DockPlacement::Bottom,
            ]
            .into_iter()
            .find_map(|placement| dock.layout(placement)?.find_panel_node(board));
            dock.add_panel_view(
                panel_handle(self.agent.clone()),
                DockPlacement::Right,
                None,
                window,
                cx,
            );
            if let Some(node) = node {
                dock.move_panel(
                    id,
                    InsertTarget::Tabs {
                        node,
                        ix: None,
                        activate: false,
                    },
                    window,
                    cx,
                );
            }
        });
    }

    fn open_tool(&self, tool: Tool, window: &mut Window, cx: &mut App) {
        if tool == Tool::Agent {
            self.ensure_agent_tab(window, cx);
        }
        let (id, handle) = match tool {
            Tool::Agent => (
                PanelId::from(self.agent.entity_id()),
                panel_handle(self.agent.clone()),
            ),
            Tool::Catalog => (
                PanelId::from(self.catalog.entity_id()),
                panel_handle(self.catalog.clone()),
            ),
            _ => {
                let panel = self.tools[&tool].clone();
                (PanelId::from(panel.entity_id()), panel_handle(panel))
            }
        };
        self.dock.update(cx, |dock, cx| {
            if dock.panel(id).is_none() {
                // A console tool joins the existing console group wherever the
                // user has moved it, rather than opening a second one.
                let target = tool.is_console().then(|| self.console_node(dock)).flatten();
                dock.add_panel_view(
                    handle,
                    if target.is_some() {
                        DockPlacement::Center
                    } else {
                        tool.placement()
                    },
                    None,
                    window,
                    cx,
                );
                if let Some(node) = target {
                    dock.move_panel(
                        id,
                        InsertTarget::Tabs {
                            node,
                            ix: Some(0),
                            activate: true,
                        },
                        window,
                        cx,
                    );
                }
            }
            reveal_panel(dock, id, window, cx);
            dock.select_panel(id, window, cx);
        });
    }

    /// Whether the dock a tool currently lives in is open.
    fn is_dock_open(&self, tool: Tool, cx: &App) -> bool {
        let Some(id) = self.panel_id(tool, cx) else {
            return false;
        };
        let dock = self.dock.read(cx);
        [
            DockPlacement::Left,
            DockPlacement::Right,
            DockPlacement::Bottom,
        ]
        .into_iter()
        .find(|placement| {
            dock.layout(*placement)
                .is_some_and(|tree| tree.find_panel_node(id).is_some())
        })
        .is_none_or(|placement| dock.is_dock_open(placement))
    }

    /// Collapse the dock a tool lives in. The center area has no dock to close.
    fn close_dock(&self, tool: Tool, window: &mut Window, cx: &mut App) {
        let Some(id) = self.panel_id(tool, cx) else {
            return;
        };
        self.dock.update(cx, |dock, cx| {
            let placement = [
                DockPlacement::Left,
                DockPlacement::Right,
                DockPlacement::Bottom,
            ]
            .into_iter()
            .find(|placement| {
                dock.layout(*placement)
                    .is_some_and(|tree| tree.find_panel_node(id).is_some())
            });
            if let Some(placement) = placement {
                if dock.is_dock_open(placement) {
                    dock.toggle_dock(placement, window, cx);
                }
            }
        });
    }

    fn panel_id(&self, tool: Tool, cx: &App) -> Option<PanelId> {
        let _ = cx;
        Some(match tool {
            Tool::Agent => PanelId::from(self.agent.entity_id()),
            Tool::Catalog => PanelId::from(self.catalog.entity_id()),
            other => PanelId::from(self.tools.get(&other)?.entity_id()),
        })
    }

    /// The dock node that already holds a console tool, if any.
    fn console_node(&self, dock: &DockArea) -> Option<NodeId> {
        Tool::SHARED
            .iter()
            .filter(|tool| tool.is_console())
            .find_map(|tool| {
                let id = PanelId::from(self.tools[tool].entity_id());
                [
                    DockPlacement::Center,
                    DockPlacement::Bottom,
                    DockPlacement::Left,
                    DockPlacement::Right,
                ]
                .into_iter()
                .find_map(|placement| dock.layout(placement)?.find_panel_node(id))
            })
    }

    fn open_flash(&self, record: RecentBoard, window: &mut Window, cx: &mut App) {
        let existing = self
            .flashes
            .borrow()
            .get(&record.key)
            .and_then(WeakEntity::upgrade);
        let panel = existing.unwrap_or_else(|| {
            let key = record.key.clone();
            let panel = cx.new(|cx| {
                FlashPanel::new(record, self.owner.clone(), self.leases.clone(), window, cx)
            });
            self.flashes.borrow_mut().insert(key, panel.downgrade());
            panel
        });
        let id = PanelId::from(panel.entity_id());
        self.dock.update(cx, |dock, cx| {
            if dock.panel(id).is_none() {
                dock.add_panel_view(
                    panel_handle(panel),
                    DockPlacement::Right,
                    Some(window.rem_size() * 26.0),
                    window,
                    cx,
                );
            }
            reveal_panel(dock, id, window, cx);
            dock.select_panel(id, window, cx);
        });
    }
}

impl Workspace {
    pub fn initialize_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panels = PanelWorkspace::new(
            cx.weak_entity(),
            self.port_leases.clone(),
            self.settings.panel_layout.clone(),
            window,
            cx,
        );
        let subscription = cx.subscribe(&panels.dock, |this, _, event, cx| {
            if matches!(event, DockEvent::LayoutChanged) {
                this.save_panel_layout(cx);
            }
        });
        self._subscriptions.push(subscription);
        self.panels = Some(panels);
        cx.notify();
    }

    pub(crate) fn save_panel_layout(&mut self, cx: &mut Context<Self>) {
        let Some(dock) = self.panels.as_ref().map(|panels| panels.dock.clone()) else {
            return;
        };
        self.layout_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            _ = this.update(cx, |this, cx| {
                this.settings.panel_layout = Some(dock.read(cx).dump(cx));
                this.settings.save();
            });
        }));
    }

    pub(crate) fn show_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(tool, Tool::Editor | Tool::Repl | Tool::Device) {
            self.workspace_mode = tool;
        }
        self.screen = crate::workspace_state::Screen::Workspace;
        self.select_panel(PanelChoice::Tool(tool));
        if let Some(panels) = &self.panels {
            panels.open_tool(tool, window, cx);
        }
        cx.notify();
    }

    /// Hide the dock a tool lives in when it is already the one on screen, so
    /// the activity bar reads as a toggle.
    pub(crate) fn toggle_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        let showing = self.screen == crate::workspace_state::Screen::Workspace
            && self.is_tool_visible(tool)
            && self.is_dock_open(tool, cx);
        if showing && tool != Tool::Editor {
            if let Some(panels) = &self.panels {
                panels.close_dock(tool, window, cx);
            }
            cx.notify();
            return;
        }
        self.show_tool(tool, window, cx);
    }

    fn is_dock_open(&self, tool: Tool, cx: &App) -> bool {
        self.panels
            .as_ref()
            .is_some_and(|panels| panels.is_dock_open(tool, cx))
    }

    pub(crate) fn open_board_panel(
        &mut self,
        record: RecentBoard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_panel(PanelChoice::Board(record.key.clone()));
        if let Some(panels) = &self.panels {
            panels.open_flash(record, window, cx);
        }
        cx.notify();
    }

    /// The Settings screen, built with the panels so it shares their lifetime.
    pub(crate) fn settings_screen(&self) -> Option<Entity<SettingsScreen>> {
        self.panels.as_ref().map(|panels| panels.settings.clone())
    }

    pub(crate) fn reset_panels(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panels) = &self.panels {
            panels.reset_layout(window, cx);
        }
    }

    pub(crate) fn persist_agent_chats(&mut self) {
        self.settings.agent_chats = self.agents.snapshot_chats();
        self.settings.save();
    }
}

fn reveal_panel(dock: &mut DockArea, id: PanelId, window: &mut Window, cx: &mut Context<DockArea>) {
    let placement = [
        DockPlacement::Left,
        DockPlacement::Right,
        DockPlacement::Bottom,
    ]
    .into_iter()
    .find(|placement| {
        dock.layout(*placement)
            .is_some_and(|tree| tree.find_panel_node(id).is_some())
    });
    if let Some(placement) = placement {
        if !dock.is_dock_open(placement) {
            dock.toggle_dock(placement, window, cx);
        }
    }
}
