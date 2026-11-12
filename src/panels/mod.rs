//! Dockable workspace tools. Each tool owns retained focus and lifecycle;
//! device panels additionally own their target and background jobs.
mod flash;

use crate::hardware::{PortLeases, RecentBoard};
use crate::workspace_state::Workspace;
use flash::FlashPanel;
use gpui_kit::component::dock::{
    panel_handle, register_panel, BasePanel, DockArea, DockAreaState, DockEvent, DockLayout,
    DockPlacement, DockSkin, Panel, PanelEvent, PanelId, PanelInfo, PanelStyle,
};
use gpui_kit::{
    div, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, WeakEntity, Window,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tool {
    Files,
    Boards,
    Editor,
    Output,
    Repl,
}
impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Boards => "Boards",
            Self::Editor => "Editor",
            Self::Output => "Output",
            Self::Repl => "REPL",
        }
    }
    fn registry_name(self) -> &'static str {
        match self {
            Self::Files => "anode.files",
            Self::Boards => "anode.boards",
            Self::Editor => "anode.editor",
            Self::Output => "anode.output",
            Self::Repl => "anode.repl",
        }
    }
    fn placement(self) -> DockPlacement {
        match self {
            Self::Files | Self::Boards => DockPlacement::Left,
            Self::Editor => DockPlacement::Center,
            Self::Output | Self::Repl => DockPlacement::Bottom,
        }
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
    fn focus_handle(&self, _: &App) -> FocusHandle {
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
}
impl Render for ToolPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = self.tool;
        let content = self
            .owner
            .update(cx, |workspace, cx| match tool {
                Tool::Files => workspace.render_sidebar(window, cx).into_any_element(),
                Tool::Boards => workspace.render_recent_boards(cx).into_any_element(),
                Tool::Editor => gpui_kit::base::v_flex()
                    .size_full()
                    .min_h_0()
                    .child(workspace.render_editor_tabs(cx))
                    .child(workspace.render_editor(cx))
                    .into_any_element(),
                Tool::Output => workspace.render_shell_log(cx).into_any_element(),
                Tool::Repl => workspace.render_repl(cx).into_any_element(),
            })
            .unwrap_or_else(|_| div().into_any_element());
        gpui_kit::base::v_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .child(content)
    }
}

type FlashPanels = Rc<RefCell<BTreeMap<String, WeakEntity<FlashPanel>>>>;

pub(crate) struct PanelWorkspace {
    pub(crate) dock: Entity<DockArea>,
    tools: BTreeMap<Tool, Entity<ToolPanel>>,
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
        let (dock, skin) = DockSkin::dock_area("anode-workspace", Some(1), window, cx);
        skin.set_panel_style(PanelStyle::TabBar, cx);
        skin.set_close_button_visible(true, cx);
        let mut tools = BTreeMap::new();
        for tool in [
            Tool::Files,
            Tool::Boards,
            Tool::Editor,
            Tool::Output,
            Tool::Repl,
        ] {
            let panel = cx.new(|cx| ToolPanel::new(tool, owner.clone(), cx));
            let restored = panel.clone();
            register_panel(cx, tool.registry_name(), move |_, _, _| {
                panel_handle(restored.clone())
            });
            tools.insert(tool, panel);
        }
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
            flashes,
            owner,
            leases,
        };
        let restored = saved
            .filter(|state| state.version == Some(1))
            .is_some_and(|state| {
                this.dock
                    .update(cx, |dock, cx| dock.load(state, window, cx).is_ok())
            });
        if !restored {
            this.reset_layout(window, cx);
        }
        this
    }

    fn reset_layout(&self, window: &mut Window, cx: &mut App) {
        // Retain all board panels and any running jobs when resetting the tools.
        let boards: Vec<_> = self
            .flashes
            .borrow()
            .values()
            .filter_map(WeakEntity::upgrade)
            .collect();
        let rem = window.rem_size();
        let layout =
            |tool| DockLayout::tabs().panel_view(panel_handle(self.tools[&tool].clone()), cx);
        let editor = layout(Tool::Editor);
        let left = DockLayout::v_split()
            .child(layout(Tool::Files), Some(rem * 26.0))
            .child(layout(Tool::Boards), None);
        let bottom = DockLayout::tabs()
            .panel_view(panel_handle(self.tools[&Tool::Output].clone()), cx)
            .panel_view(panel_handle(self.tools[&Tool::Repl].clone()), cx);
        self.dock.update(cx, |dock, cx| {
            dock.set_center(editor, window, cx);
            if dock.has_dock(DockPlacement::Right) {
                dock.remove_dock(DockPlacement::Right, window, cx);
            }
            dock.set_dock(DockPlacement::Left, left, window, cx);
            dock.set_dock_size(DockPlacement::Left, rem * 18.0, window, cx);
            dock.set_dock(DockPlacement::Bottom, bottom, window, cx);
            dock.set_dock_size(DockPlacement::Bottom, rem * 15.0, window, cx);
            for placement in [DockPlacement::Left, DockPlacement::Bottom] {
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

    fn open_tool(&self, tool: Tool, window: &mut Window, cx: &mut App) {
        let panel = self.tools[&tool].clone();
        let id = PanelId::from(panel.entity_id());
        self.dock.update(cx, |dock, cx| {
            if dock.panel(id).is_none() {
                dock.add_panel_view(panel_handle(panel), tool.placement(), None, window, cx);
            }
            reveal_panel(dock, id, window, cx);
            dock.select_panel(id, window, cx);
        });
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

    pub(crate) fn show_tool(&self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panels) = &self.panels {
            panels.open_tool(tool, window, cx);
        }
    }
    pub(crate) fn open_board_panel(
        &self,
        record: RecentBoard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panels) = &self.panels {
            panels.open_flash(record, window, cx);
        }
    }
    pub(crate) fn reset_panels(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panels) = &self.panels {
            panels.reset_layout(window, cx);
        }
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
