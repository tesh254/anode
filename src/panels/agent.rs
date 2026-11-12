//! Retained side-chat surface. Transport and message delivery will be supplied
//! by the ACP client; disconnected drafts never masquerade as sent messages.
use crate::workspace_state::Workspace;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::Button;
use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::select::{SearchableVec, Select, SelectItem, SelectState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _};
use gpui_kit::{
    div, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, WeakEntity, Window,
};

#[derive(Clone)]
pub(super) struct AgentChoice(&'static str);

impl SelectItem for AgentChoice {
    type Value = &'static str;
    fn title(&self) -> SharedString {
        self.0.into()
    }
    fn value(&self) -> &Self::Value {
        &self.0
    }
}

pub(super) struct AgentPanel {
    owner: WeakEntity<Workspace>,
    draft: Entity<TextareaState>,
    agent: Entity<SelectState<SearchableVec<AgentChoice>>>,
    _subscription: Subscription,
}

impl AgentPanel {
    pub(super) fn new(
        owner: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(4)
                .placeholder("Ask about your code, board or circuit…")
        });
        let agent = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(vec![
                    AgentChoice("Claude Code"),
                    AgentChoice("Codex"),
                    AgentChoice("OpenCode"),
                    AgentChoice("Custom ACP agent"),
                ]),
                None,
                window,
                cx,
            )
        });
        let subscription = owner
            .upgrade()
            .map(|owner| cx.observe(&owner, |_, _, cx| cx.notify()))
            .expect("workspace exists while creating Agent panel");
        Self {
            owner,
            draft,
            agent,
            _subscription: subscription,
        }
    }
}

impl EventEmitter<PanelEvent> for AgentPanel {}

impl Focusable for AgentPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.draft.read(cx).focus_handle(cx)
    }
}

impl BasePanel for AgentPanel {
    fn panel_name(&self) -> &'static str {
        super::Tool::Agent.registry_name()
    }
}

impl Panel for AgentPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Agent"
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some("Agent".into())
    }
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Render for AgentPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (project, board) = self
            .owner
            .upgrade()
            .map(|owner| {
                let workspace = owner.read(cx);
                (
                    if workspace.project_name.is_empty() {
                        "No project open".to_string()
                    } else {
                        workspace.project_name.clone()
                    },
                    workspace
                        .active_board()
                        .map(|board| board.name)
                        .unwrap_or("No board selected"),
                )
            })
            .unwrap_or_else(|| ("No project open".into(), "No board selected"));
        v_flex()
            .size_full().min_h_0().min_w_0()
            .bg(cx.theme().sidebar)
            .child(v_flex().p_3().gap_2().flex_shrink_0()
                .border_b_1().border_color(cx.theme().border)
                .child(h_flex().justify_between()
                    .child(div().text_xs().child("Coding agent"))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child("Not connected")))
                .child(Select::new(&self.agent).w_full().small()
                    .placeholder("Choose an ACP agent").accessibility_label("Coding agent"))
                .child(div().text_xs().text_color(cx.theme().muted_foreground)
                    .child("ACP connections are not available yet.")))
            .child(div().id("agent-conversation").flex_1().min_h_0().overflow_y_scroll()
                .child(v_flex().p_4().gap_3()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD)
                        .child("Build with your agent"))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("Discuss firmware, plan a circuit or debug your board alongside your code."))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child("Once connected, your conversation and tool results will appear here."))
                    .child(Button::new("agent-start-draft").label("Draft a project request")
                        .outline().small().on_click(cx.listener(|this, _, window, cx| {
                            this.draft.update(cx, |draft, cx| {
                                draft.set_value("Help me plan a board project with components, wiring and firmware.", window, cx);
                                draft.focus(window, cx);
                            });
                        })))))
            .child(v_flex().p_3().gap_2().flex_shrink_0()
                .border_t_1().border_color(cx.theme().border)
                .child(div().text_xs().text_color(cx.theme().muted_foreground)
                    .child("Current workspace"))
                .child(div().text_xs().truncate().child(project))
                .child(div().text_xs().truncate().child(board))
                .child(Textarea::new(&self.draft).aria_label("Message to coding agent"))
                .child(h_flex().gap_2().justify_between()
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child("Draft kept while switching tabs"))
                    .child(Button::new("agent-send").label("Send").small()
                        .disabled(true).tooltip("Connect an ACP agent to send messages"))))
    }
}
