//! A recovery dialog shared by the workspace and per-board task panels.
use crate::{
    panels::Tool,
    recovery::{RecoveryContext, RecoveryIssue, RecoveryKind},
    workspace_state::Workspace,
};
use gpui_kit::base::{h_flex, v_flex, Link};
use gpui_kit::component::{
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    scroll::ScrollableElement as _,
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, App, ClipboardItem, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled, WeakEntity, Window,
};

pub(crate) fn open_recovery_dialog(
    context: RecoveryContext,
    kind: RecoveryKind,
    detail: Option<String>,
    owner: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, window, cx| {
        let board_name = context
            .board
            .map(|board| board.name)
            .unwrap_or("Unidentified board");
        let port = if context.port.is_empty() {
            "No port selected"
        } else {
            &context.port
        };
        let summary = format!("{board_name} · {} · {port}", context.runtime.label());
        let mut content = v_flex()
            .gap_4()
            .text_sm()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(summary.clone()),
            )
            .child(div().child(kind.explanation()));
        for (ix, step) in context.steps(kind).into_iter().enumerate() {
            content = content.child(
                h_flex()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{}.", ix + 1)),
                    )
                    .child(div().flex_1().min_w_0().child(step)),
            );
        }
        content = content
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child("If firmware is not starting"),
                    )
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(context.firmware_help()),
                    ),
            )
            .child(
                Link::new("recovery-docs")
                    .href(context.documentation())
                    .open_with(|url, _, _, cx| cx.open_url(url))
                    .text_color(cx.theme().link)
                    .text_decoration_1()
                    .rounded_sm()
                    .focus_visible(|style| style.bg(cx.theme().accent))
                    .child("Open runtime troubleshooting docs"),
            );
        if let Some(detail) = &detail {
            content = content.child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Reported error"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(detail.clone()),
                    ),
            );
        }
        let diagnostics = format!(
            "{summary}\n{}",
            detail.as_deref().unwrap_or(kind.explanation())
        );
        let repl_owner = owner.clone();
        let scan_owner = owner.clone();
        let setup_owner = owner.clone();
        let target = context.port.clone();
        let ready = owner.upgrade().is_some_and(|owner| {
            let workspace = owner.read(cx);
            !workspace.device_busy && workspace.ports.iter().any(|port| port.path == target)
        });
        dialog
            .title(kind.title())
            .w(window.rem_size() * 36.)
            .child(content.max_h(gpui_kit::rems(23.)).overflow_y_scrollbar())
            .footer(
                DialogFooter::new()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("recovery-copy")
                            .label("Copy details")
                            .ghost()
                            .small()
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    diagnostics.clone(),
                                ))
                            }),
                    )
                    .child(
                        Button::new("recovery-scan")
                            .label("Rescan USB")
                            .outline()
                            .small()
                            .on_click(move |_, _, cx| {
                                _ = scan_owner
                                    .update(cx, |workspace, cx| workspace.refresh_ports(cx));
                            }),
                    )
                    .when(context.runtime.is_interpreted(), |footer| {
                        footer.child(
                            Button::new("recovery-repl")
                                .label("Open REPL")
                                .outline()
                                .small()
                                .disabled(!ready)
                                .on_click(move |_, window, cx| {
                                    _ = repl_owner.update(cx, |workspace, cx| {
                                        if workspace.device_busy
                                            || !workspace
                                                .ports
                                                .iter()
                                                .any(|port| port.path == target)
                                        {
                                            return;
                                        }
                                        window.close_dialog(cx);
                                        workspace.select_port(target.clone());
                                        workspace.repl_connect();
                                        workspace.show_tool(Tool::Repl, window, cx);
                                    });
                                }),
                        )
                    })
                    .child(
                        Button::new("recovery-setup")
                            .label("Board setup")
                            .outline()
                            .small()
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                _ = setup_owner.update(cx, |workspace, cx| {
                                    workspace.show_tool(Tool::Device, window, cx)
                                });
                            }),
                    )
                    .child(
                        Button::new("recovery-done")
                            .label("Done")
                            .small()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
    });
}

impl Workspace {
    pub(crate) fn recovery_context(&self) -> RecoveryContext {
        RecoveryContext {
            port: self.selected_port.clone(),
            runtime: self.active_runtime(),
            board: self.active_board(),
        }
    }

    pub(crate) fn show_connection_help(&self, window: &mut Window, cx: &mut Context<Self>) {
        open_recovery_dialog(
            self.recovery_context(),
            RecoveryKind::General,
            None,
            cx.weak_entity(),
            window,
            cx,
        );
    }

    pub(crate) fn render_recovery_notice(
        &self,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div().when_some(
            self.device_recovery.clone(),
            |notice, issue: RecoveryIssue| {
                let help = issue.clone();
                notice
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(div().flex_1().min_w_0().text_sm().child(format!(
                                "{} · {}",
                                issue.kind.title(),
                                issue.context.port
                            )))
                            .child(
                                Button::new(SharedString::from(format!("{id}-help")))
                                    .label("How to fix…")
                                    .outline()
                                    .small()
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        open_recovery_dialog(
                                            help.context.clone(),
                                            help.kind,
                                            Some(help.detail.clone()),
                                            cx.weak_entity(),
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("{id}-dismiss")))
                                    .label("Dismiss")
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.device_recovery = None;
                                        cx.notify();
                                    })),
                            ),
                    )
            },
        )
    }
}
