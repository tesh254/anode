//! The two console surfaces: Output, which carries everything the external
//! tools say, and REPL, which is a live serial session with the board.

use crate::workspace_state::{HistoryStep, LogFilter, LogKind, Workspace};

use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled,
};

impl Workspace {
    // -- Output ---------------------------------------------------------------

    pub(crate) fn render_output(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.visible_log_entries();
        let problems = self
            .log_entries
            .iter()
            .filter(|entry| entry.kind.is_problem())
            .count();
        let filter = self.log_filter;
        let follow = self.log_follow;
        let empty = entries.is_empty();
        let last = entries.len().saturating_sub(1);

        // Keeping the newest line in view is a scroll request, not state: the
        // handle resolves it against the laid-out rows after this frame.
        if follow && !empty {
            self.log_scroll.scroll_to_item(last);
        }

        v_flex()
            .size_full()
            .min_h_0()
            .bg(crate::theme::terminal_background(cx))
            .child(
                h_flex()
                    .h_9()
                    .px_3()
                    .gap_2()
                    .flex_shrink_0()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        ButtonGroup::new("output-filter")
                            .outline()
                            .small()
                            .child(
                                Button::new("output-all")
                                    .label("All")
                                    .selected(filter == LogFilter::All)
                                    .toggled(filter == LogFilter::All),
                            )
                            .child(
                                Button::new("output-problems")
                                    .label(if problems > 0 {
                                        format!("Problems ({problems})")
                                    } else {
                                        "Problems".to_string()
                                    })
                                    .selected(filter == LogFilter::Problems)
                                    .toggled(filter == LogFilter::Problems),
                            )
                            .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                                this.set_log_filter(match clicked.first() {
                                    Some(1) => LogFilter::Problems,
                                    _ => LogFilter::All,
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("output-follow")
                                    .icon(IconName::Pin)
                                    .ghost()
                                    .small()
                                    .selected(follow)
                                    .toggled(follow)
                                    .accessibility_label("Follow new output")
                                    .tooltip("Follow new output")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.log_follow = !this.log_follow;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("output-copy")
                                    .icon(IconName::Copy)
                                    .ghost()
                                    .small()
                                    .disabled(empty)
                                    .accessibility_label("Copy output")
                                    .tooltip("Copy output")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            this.log_as_text(),
                                        ));
                                    })),
                            )
                            .child(
                                Button::new("output-clear")
                                    .icon(IconName::Trash)
                                    .ghost()
                                    .small()
                                    .disabled(self.log_entries.is_empty())
                                    .accessibility_label("Clear output")
                                    .tooltip("Clear output")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.clear_log();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("output-rows")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.log_scroll)
                    .px_4()
                    .py_3()
                    .gap_1()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_sm()
                    .when(empty, |this| {
                        this.child(div().text_color(crate::theme::console_palette().dim).child(
                            match filter {
                                LogFilter::Problems => "No warnings or errors.",
                                LogFilter::All => {
                                    "Build, upload or flash a board to see what the tools say."
                                }
                            },
                        ))
                    })
                    .children(entries.into_iter().map(|entry| {
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(crate::theme::console_palette().dim)
                                    .child(entry.time),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_color(log_color(entry.kind))
                                    .child(entry.message),
                            )
                    })),
            )
    }

    // -- REPL -----------------------------------------------------------------

    pub(crate) fn render_repl(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.repl_connected;
        let port = self.selected_port.clone();
        let output = self.repl_output.clone();
        let has_port = !port.is_empty();

        v_flex()
            .size_full()
            .min_h_0()
            .bg(crate::theme::terminal_background(cx))
            .child(
                h_flex()
                    .h_9()
                    .px_3()
                    .gap_2()
                    .flex_shrink_0()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("repl-toggle")
                                    .label(if connected { "Disconnect" } else { "Connect" })
                                    .small()
                                    .compact()
                                    .outline()
                                    .disabled(!has_port)
                                    .tooltip(if has_port {
                                        SharedString::from(port.clone())
                                    } else {
                                        SharedString::from("Connect a board first")
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.repl_connected {
                                            this.repl_disconnect();
                                        } else {
                                            this.repl_connect();
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("repl-interrupt")
                                    .label("^C")
                                    .small()
                                    .compact()
                                    .ghost()
                                    .disabled(!connected)
                                    .tooltip("Interrupt the running program")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.repl_break();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("repl-soft-reset")
                                    .label("Soft reset")
                                    .small()
                                    .compact()
                                    .ghost()
                                    .disabled(!connected)
                                    .tooltip("Restart the interpreter without a power cycle")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.repl_soft_reset();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("repl-copy")
                                    .icon(IconName::Copy)
                                    .ghost()
                                    .small()
                                    .disabled(output.is_empty())
                                    .accessibility_label("Copy session")
                                    .tooltip("Copy session")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            this.repl_output.clone(),
                                        ));
                                    })),
                            )
                            .child(
                                Button::new("repl-clear")
                                    .icon(IconName::Trash)
                                    .ghost()
                                    .small()
                                    .disabled(output.is_empty())
                                    .accessibility_label("Clear session")
                                    .tooltip("Clear session")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.repl_output.clear();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("repl-output")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_4()
                    .py_3()
                    .child(
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_sm()
                            .text_color(if output.is_empty() {
                                crate::theme::console_palette().dim
                            } else {
                                crate::theme::console_palette().text
                            })
                            .child(if output.is_empty() {
                                self.repl_placeholder()
                            } else {
                                output
                            }),
                    ),
            )
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    // Up and Down belong to the session's history, not to the
                    // single-line input's cursor, so claim them before the
                    // input's own key bindings run.
                    .capture_key_down(cx.listener(
                        |this, event: &gpui_kit::KeyDownEvent, window, cx| {
                            if !this.repl_connected || event.keystroke.modifiers.modified() {
                                return;
                            }
                            let direction = match event.keystroke.key.as_str() {
                                "up" => HistoryStep::Older,
                                "down" => HistoryStep::Newer,
                                _ => return,
                            };
                            let Some(line) = this.repl_history_step(direction) else {
                                return;
                            };
                            this.repl_input.update(cx, |state, cx| {
                                state.set_value(line, window, cx);
                            });
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ))
                    .child(
                        Input::new(&self.repl_input)
                            .id("repl-input")
                            .small()
                            .flex_1(),
                    )
                    .child(
                        Button::new("repl-send")
                            .label("Send")
                            .small()
                            .compact()
                            .primary()
                            .disabled(!connected)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let line = this.repl_input.read(cx).value().to_string();
                                this.repl_send_raw(line);
                                this.repl_input.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                                cx.notify();
                            })),
                    ),
            )
    }

    /// What the REPL says before a session starts, which depends on why there
    /// isn't one.
    fn repl_placeholder(&self) -> String {
        if self.selected_port.is_empty() {
            "Connect a board over USB, then press Connect.".into()
        } else if !self.active_runtime().is_interpreted() {
            format!(
                "{} is not an interpreted runtime, so there is no REPL. \
                 The serial monitor still shows what the program prints.",
                self.active_runtime().label()
            )
        } else {
            format!("Press Connect to open a REPL on {}.", self.selected_port)
        }
    }
}

/// The colour a log line carries, from the console's own palette rather than
/// the window theme — see [`crate::theme::ConsolePalette`].
fn log_color(kind: LogKind) -> gpui_kit::Hsla {
    let palette = crate::theme::console_palette();
    match kind {
        LogKind::Error => palette.error,
        LogKind::Warn => palette.warn,
        LogKind::Ok => palette.ok,
        LogKind::Install | LogKind::Info => palette.info,
        LogKind::Flash => palette.text,
    }
}
