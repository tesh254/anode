//! The Anode workspace shell: title bar, activity bar, editor, project files
//! and status bar.
//!
//! Each region is a `render_*` helper on `Workspace` rather than a separate
//! entity, because they all read the one workspace state and hold no retained
//! state of their own. The exceptions are the Select controls, whose
//! `SelectState` entities live in [`Pickers`] and are created once so their
//! selection survives re-renders.

mod console;
mod device;
mod recovery;

pub(crate) use device::{board_groups, flash_detail, flash_method_label, BoardItem};
pub(crate) use recovery::open_recovery_dialog;

use crate::panels::Tool;
use crate::project::TEMPLATES;
use crate::workspace_state::Workspace;
use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::select::{
    SearchableVec, Select, SelectEvent, SelectGroup, SelectItem, SelectState,
};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, svg, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
};

// ---------------------------------------------------------------------------
// Shared presentation helpers
// ---------------------------------------------------------------------------

/// A section heading inside an inspector column: icon, title, and an optional
/// trailing control on the same baseline.
pub(crate) fn section_header(
    icon: IconName,
    title: impl Into<SharedString>,
    cx: &gpui_kit::App,
) -> gpui_kit::Div {
    h_flex()
        .gap_2()
        .child(
            Icon::new(icon)
                .size_4()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().muted_foreground)
                .child(title.into()),
        )
}

/// A label/value row that keeps values on a shared trailing spine.
pub(crate) fn detail_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    cx: &gpui_kit::App,
) -> gpui_kit::Div {
    h_flex()
        .justify_between()
        .gap_4()
        .py_0p5()
        .text_sm()
        .child(
            div()
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(div().min_w_0().truncate().child(value.into()))
}

/// What a running flash is doing, and how far it has got.
///
/// A flash is minutes of work on hardware the user cannot see, so the phase
/// is named in words and the bar is determinate wherever the flasher reports
/// a percentage — state is never left to a spinner that could mean anything.
pub(crate) fn flash_progress(
    description: &str,
    progress: Option<&crate::esp::Progress>,
    cx: &gpui_kit::App,
) -> gpui_kit::Div {
    let phase = progress
        .map(|progress| progress.phase.to_string())
        .unwrap_or_else(|| description.to_string());
    let percent = progress.and_then(|progress| progress.percent);
    v_flex()
        .gap_1p5()
        .child(
            h_flex()
                .gap_2()
                .justify_between()
                .text_xs()
                .child(div().min_w_0().truncate().child(phase.clone()))
                .when_some(percent, |this, percent| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{percent:.0}%")),
                    )
                }),
        )
        .child(
            gpui_kit::component::progress::Progress::new("flash-progress")
                .loading(percent.is_none())
                .value(percent.unwrap_or_default())
                .accessibility_label(phase),
        )
}

/// The connection dot used in the status bar and every board row, so one
/// state reads the same everywhere.
pub(crate) fn status_dot(connected: bool, cx: &gpui_kit::App) -> gpui_kit::Div {
    div().size_2().rounded_full().bg(if connected {
        cx.theme().success
    } else {
        cx.theme().muted_foreground
    })
}

// ---------------------------------------------------------------------------
// Select items
// ---------------------------------------------------------------------------

/// A Select item wrapping a port path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortItem {
    pub(crate) path: String,
    pub(crate) board: Option<&'static str>,
}

impl SelectItem for PortItem {
    type Value = String;

    fn title(&self) -> SharedString {
        match self.board {
            Some(board) => format!("{} · {}", self.path, board).into(),
            None => self.path.clone().into(),
        }
    }

    fn value(&self) -> &Self::Value {
        &self.path
    }
}

/// A Select item wrapping a runtime profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) pinned: bool,
}

impl SelectItem for ProfileItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }

    fn render(&self, _: &mut Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .justify_between()
            .child(div().min_w_0().truncate().child(self.label.clone()))
            .when(self.pinned, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(cx.theme().success)
                        .child("detected"),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Retained pickers
// ---------------------------------------------------------------------------

/// The workspace's retained Select entities, created once so their selection
/// state survives re-renders.
#[derive(Default)]
pub struct Pickers {
    pub(crate) port: Option<Entity<SelectState<SearchableVec<PortItem>>>>,
    pub(crate) board: Option<Entity<SelectState<SearchableVec<SelectGroup<BoardItem>>>>>,
    pub(crate) profile: Option<Entity<SelectState<SearchableVec<ProfileItem>>>>,
}

/// Pickers are rebuilt only when their item set changes; these caches hold
/// the last-seen keys so `sync_pickers` stays cheap on every frame.
#[derive(Default)]
pub struct PickerCaches {
    pub port_items: Vec<String>,
    pub board_chip: Option<crate::boards::Chip>,
    pub profile_items: Vec<String>,
}

// ---------------------------------------------------------------------------
// Root view
// ---------------------------------------------------------------------------

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Drain async sources before painting so the frame shows fresh state.
        self.drain_repl_events();
        self.drain_background_events(window, cx);
        self.sync_pickers(window, cx);

        if self.screen == crate::workspace_state::Screen::Settings {
            return self.render_settings_screen(cx).into_any_element();
        }

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .child(self.render_header(cx))
            .child(self.render_recovery_notice("workspace-recovery", cx))
            .when(
                self.connection_dropped && !self.selected_port.is_empty(),
                |this| this.child(self.render_connection_banner(cx)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.render_activity_bar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .when_some(self.panels.as_ref(), |this, panels| {
                                this.child(panels.dock.clone())
                            }),
                    ),
            )
            .child(self.render_status_bar(cx))
            .into_any_element()
    }
}

/// Brand lockup using the wordmark's standalone `a` glyph.
fn render_logo(cx: &Context<Workspace>) -> impl IntoElement {
    h_flex()
        .gap_2()
        .child(
            svg()
                .path(crate::assets::MARK_PATH)
                .size_5()
                .text_color(cx.theme().primary),
        )
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::BOLD)
                .child("anode"),
        )
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

impl Workspace {
    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let project = if self.project_name.is_empty() {
            "No project".to_string()
        } else {
            format!("{} · {}", self.project_name, self.project_kind.label())
        };
        let mode = self.workspace_mode;

        gpui_kit::component::TitleBar::new()
            .h_12()
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .px_3()
                    .justify_between()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_3()
                            .child(render_logo(cx))
                            .child(div().h_4().border_l_1().border_color(cx.theme().border))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(project),
                            ),
                    )
                    .child(
                        ButtonGroup::new("workspace-mode")
                            .outline()
                            .small()
                            .child(
                                Button::new("mode-editor")
                                    .label("Editor")
                                    .icon(IconName::Code)
                                    .selected(mode == Tool::Editor)
                                    .toggled(mode == Tool::Editor),
                            )
                            .child(
                                Button::new("mode-repl")
                                    .label("REPL")
                                    .icon(IconName::SquareTerminal)
                                    .selected(mode == Tool::Repl)
                                    .toggled(mode == Tool::Repl),
                            )
                            .child(
                                Button::new("mode-board")
                                    .label("Board")
                                    .icon(IconName::Cpu)
                                    .selected(mode == Tool::Device)
                                    .toggled(mode == Tool::Device),
                            )
                            .on_click(cx.listener(|this, clicked: &Vec<usize>, window, cx| {
                                let tool = match clicked.first() {
                                    Some(0) => Tool::Editor,
                                    Some(1) => Tool::Repl,
                                    Some(2) => Tool::Device,
                                    _ => return,
                                };
                                this.show_tool(tool, window, cx);
                            })),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .justify_end()
                            .gap_1()
                            .child(
                                Button::new("header-catalog")
                                    .icon(IconName::Boxes)
                                    .ghost()
                                    .small()
                                    .accessibility_label("Supported boards")
                                    .tooltip("Supported boards")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_tool(Tool::Catalog, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("header-open-folder")
                                    .icon(IconName::FolderOpen)
                                    .ghost()
                                    .small()
                                    .accessibility_label("Open folder")
                                    .tooltip("Open folder…")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_folder_dialog(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("header-appearance")
                                    .icon(if cx.theme().is_dark() {
                                        IconName::Sun
                                    } else {
                                        IconName::Moon
                                    })
                                    .ghost()
                                    .small()
                                    .accessibility_label("Switch appearance")
                                    .tooltip("Switch appearance")
                                    .on_click(|_, _, cx| {
                                        let mode = if cx.theme().is_dark() {
                                            gpui_kit::component::ThemeMode::Light
                                        } else {
                                            gpui_kit::component::ThemeMode::Dark
                                        };
                                        gpui_kit::component::Theme::change(mode, None, cx);
                                        crate::theme::apply(cx);
                                    }),
                            ),
                    ),
            )
    }

    fn render_connection_banner(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .px_4()
            .py_1p5()
            .gap_2()
            .bg(cx.theme().warning.opacity(0.1))
            .border_b_1()
            .border_color(cx.theme().warning.opacity(0.25))
            .text_color(cx.theme().warning)
            .text_xs()
            .child(Icon::new(IconName::Unplug).size_4())
            .child(format!(
                "{} disconnected. Anode reconnects as soon as the board is back.",
                self.selected_port
            ))
    }

    // -- Activity bar ---------------------------------------------------------

    fn render_activity_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // The bar is the switcher: one panel per dock region, so the icon shows
        // which one is on screen and clicking it again hides that region.
        let showing = |tool: Tool, this: &Self| this.is_tool_visible(tool);
        let entry = |id: &'static str,
                     icon: IconName,
                     label: &'static str,
                     tool: Tool,
                     this: &Self,
                     cx: &mut Context<Self>| {
            Button::new(id)
                .icon(icon)
                .ghost()
                .selected(showing(tool, this))
                // It really is a toggle: clicking the shown panel hides it.
                .toggled(showing(tool, this))
                .accessibility_label(label)
                .tooltip(label)
                .on_click(
                    cx.listener(move |this, _, window, cx| this.toggle_tool(tool, window, cx)),
                )
        };

        v_flex()
            .w(gpui_kit::rems(3.25))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .items_center()
            .py_3()
            .gap_1p5()
            .bg(cx.theme().sidebar)
            .child(entry(
                "activity-files",
                IconName::Files,
                "Files",
                Tool::Files,
                self,
                cx,
            ))
            .child(entry(
                "activity-boards",
                IconName::Usb,
                "Boards",
                Tool::Boards,
                self,
                cx,
            ))
            .child(entry(
                "activity-device",
                IconName::Cpu,
                "Board detail",
                Tool::Device,
                self,
                cx,
            ))
            .child(entry(
                "activity-agent",
                IconName::Sparkles,
                "Agent",
                Tool::Agent,
                self,
                cx,
            ))
            .child(entry(
                "activity-catalog",
                IconName::Boxes,
                "Supported boards",
                Tool::Catalog,
                self,
                cx,
            ))
            .child(div().h_2())
            .child(entry(
                "activity-repl",
                IconName::SquareTerminal,
                "REPL",
                Tool::Repl,
                self,
                cx,
            ))
            .child(entry(
                "activity-terminal",
                IconName::Terminal,
                "Terminal",
                Tool::Terminal,
                self,
                cx,
            ))
            .child(entry(
                "activity-output",
                IconName::ScanLine,
                "Output",
                Tool::Output,
                self,
                cx,
            ))
            .child(div().flex_1())
            .child(
                Button::new("activity-reset-layout")
                    .icon(IconName::PanelsTopLeft)
                    .ghost()
                    .accessibility_label("Reset layout")
                    .tooltip("Reset layout")
                    .on_click(cx.listener(|this, _, window, cx| this.reset_panels(window, cx))),
            )
            .child(
                Button::new("activity-settings")
                    .icon(IconName::Settings)
                    .ghost()
                    .accessibility_label("Settings")
                    .tooltip("Settings")
                    .on_click(cx.listener(|this, _, window, cx| this.show_settings(window, cx))),
            )
    }

    // -- Settings screen ------------------------------------------------------

    /// Settings replaces the workspace rather than competing with it for dock
    /// space, so it brings its own navigation and its own way back.
    fn render_settings_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .child(self.render_settings_header(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .when_some(self.settings_screen(), |this, screen| this.child(screen)),
            )
    }

    fn render_settings_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::component::TitleBar::new()
            .h_12()
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .px_3()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_3()
                            .child(render_logo(cx))
                            .child(div().h_4().border_l_1().border_color(cx.theme().border))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Settings"),
                            ),
                    )
                    .child(
                        Button::new("settings-close")
                            .label("Done")
                            .outline()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.close_settings(cx))),
                    ),
            )
    }

    // -- Editor ---------------------------------------------------------------

    pub(crate) fn render_editor_tabs(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self
            .open_tabs
            .iter()
            .position(|tab| Some(&tab.path) == self.active_tab.as_ref());
        let mut bar = TabBar::new("editor-tabs").folder();
        for tab in &self.open_tabs {
            let label = if tab.is_dirty {
                format!("{} •", tab.name)
            } else {
                tab.name.clone()
            };
            bar = bar.child(Tab::new().label(label));
        }
        if let Some(index) = active {
            bar = bar.selected_index(index);
        }
        let tabs =
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(bar.w_full().menu(true).on_click(cx.listener(
                    |this, index: &usize, window, cx| {
                        this.select_tab(*index, window, cx);
                        cx.notify();
                    },
                )));

        let layout = self.workspace_layout();
        let runtime = self.active_runtime();
        let busy = self.device_busy;
        let has_project = self.project_path.is_some();
        let has_file = self.current_file.is_some();

        let actions = h_flex()
            .gap_1()
            .flex_shrink_0()
            .child(
                Button::new("editor-save-all")
                    .icon(IconName::Save)
                    .ghost()
                    .small()
                    .accessibility_label("Save all")
                    .tooltip("Save all (⇧⌘S)")
                    .disabled(!self.is_dirty)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_all_files();
                        cx.notify();
                    })),
            )
            .when(layout.show_run, |this| {
                this.child(
                    Button::new("editor-run")
                        .label("Run")
                        .icon(IconName::Play)
                        .primary()
                        .small()
                        .loading(busy)
                        .disabled(!has_file || busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_code();
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("editor-stop")
                        .icon(IconName::Square)
                        .ghost()
                        .small()
                        .accessibility_label("Stop")
                        .tooltip("Stop the running program")
                        .disabled(self.selected_port.is_empty())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.stop_code();
                            cx.notify();
                        })),
                )
            })
            .when(layout.show_build, |this| {
                this.child(
                    Button::new("editor-build")
                        .label("Build")
                        .icon(IconName::Hammer)
                        .small()
                        .loading(busy)
                        .disabled(!has_project || busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.build_project();
                            cx.notify();
                        })),
                )
            })
            .when(layout.show_upload, |this| {
                this.child(
                    Button::new("editor-upload")
                        .label("Upload")
                        .icon(IconName::Upload)
                        .with_variant(if layout.show_build {
                            gpui_kit::component::button::ButtonVariant::Primary
                        } else {
                            gpui_kit::component::button::ButtonVariant::Secondary
                        })
                        .small()
                        .loading(busy)
                        .disabled(!has_project || busy)
                        .tooltip(match runtime {
                            crate::boards::Runtime::CircuitPython => {
                                "Copy the project onto the CIRCUITPY drive"
                            }
                            crate::boards::Runtime::MicroPython => {
                                "Copy the project onto the board's filesystem"
                            }
                            _ => "Build, then write the program to the board",
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.upload_project();
                            cx.notify();
                        })),
                )
            });

        // Below this width the tab strip and the actions stop fitting on one
        // line, so they stack instead of truncating the file names.
        let compact = window.viewport_size().width < window.rem_size() * 78.0;
        v_flex()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().tab_bar)
            .child(if compact {
                v_flex()
                    .px_2()
                    .child(h_flex().h_10().child(tabs))
                    .child(actions.h_10().justify_end())
                    .into_any_element()
            } else {
                h_flex()
                    .h_11()
                    .px_2()
                    .gap_3()
                    .child(tabs)
                    .child(actions)
                    .into_any_element()
            })
    }

    pub(crate) fn render_editor(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.current_file.is_none() {
            return self.render_editor_empty_state(cx).into_any_element();
        }
        // The component-layer `Editor`, not base's: its `Input::render` is
        // what installs the tree-sitter highlighter factory and the theme's
        // syntax colours.
        v_flex()
            .flex_1()
            .min_h_0()
            .bg(cx.theme().background)
            .child(
                div().h_full().child(
                    gpui_kit::component::input::Editor::new(&self.editor)
                        .h(gpui_kit::relative(1.0))
                        .text_size(gpui_kit::rems(0.875))
                        .appearance(false)
                        .bordered(false),
                ),
            )
            .into_any_element()
    }

    fn render_editor_empty_state(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap_2()
            .p_8()
            .child(render_logo(cx))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("MicroPython, CircuitPython, Arduino and native C/C++ on one board."),
            )
            .child(
                h_flex()
                    .gap_2()
                    .mt_4()
                    .child(
                        Button::new("empty-open-folder")
                            .label("Open folder…")
                            .icon(IconName::FolderOpen)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_folder_dialog(window, cx);
                            })),
                    )
                    .child(
                        Button::new("empty-browse-boards")
                            .label("Supported boards")
                            .icon(IconName::Boxes)
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_tool(Tool::Catalog, window, cx)
                            })),
                    ),
            )
    }

    // -- Status bar -----------------------------------------------------------

    fn render_status_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.is_connected();
        let board = self.active_board();
        let runtime = self.active_runtime();
        let missing = self.missing_tools();
        let file = self
            .current_file
            .as_ref()
            .map(|file| format!("{} · {}", file.relative, file.language.label()))
            .unwrap_or_else(|| "No file".into());

        h_flex()
            .h_7()
            .px_3()
            .gap_5()
            .justify_between()
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().status_bar)
            .text_color(cx.theme().muted_foreground)
            .text_xs()
            .child(
                h_flex()
                    .min_w_0()
                    .gap_3()
                    .child(h_flex().gap_1p5().child(status_dot(connected, cx)).child(
                        if connected {
                            self.selected_port.clone()
                        } else {
                            "No board".to_string()
                        },
                    ))
                    .when_some(board, |this, board| {
                        this.child(
                            h_flex()
                                .gap_1p5()
                                .child(Icon::new(IconName::Microchip).size_3())
                                .child(format!("{} · {}", board.name, board.chip.label())),
                        )
                    })
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(Icon::new(IconName::Workflow).size_3())
                            .child(runtime.label()),
                    )
                    .when(!missing.is_empty(), |this| {
                        let names: Vec<&str> = missing.iter().map(|tool| tool.name()).collect();
                        this.child(
                            Button::new("status-missing-tools")
                                .label(format!("Missing: {}", names.join(", ")))
                                .icon(IconName::TriangleAlert)
                                .warning()
                                .xsmall()
                                .compact()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_tool(Tool::Device, window, cx)
                                })),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap_3()
                    .flex_shrink_0()
                    .font_family(cx.theme().mono_font_family.clone())
                    .child(file)
                    .child("UTF-8")
                    .child("LF"),
            )
    }

    // -- Project files --------------------------------------------------------

    pub(crate) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().sidebar)
            .child(
                h_flex()
                    .h_9()
                    .px_3()
                    .gap_2()
                    .flex_shrink_0()
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if self.project_name.is_empty() {
                                "Files".to_string()
                            } else {
                                self.project_name.clone()
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .when(self.project_path.is_some(), |this| {
                                this.child(
                                    Button::new("files-reload")
                                        .icon(IconName::RefreshCw)
                                        .ghost()
                                        .small()
                                        .accessibility_label("Reload files")
                                        .tooltip("Reload files")
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.reload_project(cx)),
                                        ),
                                )
                            })
                            .child(
                                Button::new("files-open-folder")
                                    .icon(IconName::FolderOpen)
                                    .ghost()
                                    .small()
                                    .accessibility_label("Open folder")
                                    .tooltip("Open folder…")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_folder_dialog(window, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .pb_4()
                    .child(if self.project_files.is_empty() {
                        self.render_start_section(cx).into_any_element()
                    } else {
                        self.render_file_list(cx).into_any_element()
                    }),
            )
    }

    fn render_file_list(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        /// What a row draws. Deliberately not `ProjectFile`: that carries the
        /// file's whole contents, and cloning the project to render its names
        /// copied every open file's text on every frame.
        struct Row {
            path: PathBuf,
            name: SharedString,
            relative: SharedString,
            depth: usize,
            icon: IconName,
            selected: bool,
        }

        let current = self.current_file.as_ref().map(|file| file.path.as_path());
        let rows: Vec<Row> = self
            .project_files
            .iter()
            .map(|file| Row {
                selected: current == Some(file.path.as_path()),
                // Nested files indent under their folder; a top-level file
                // sits on the column's own leading edge.
                depth: file.relative.as_str().matches('/').count(),
                icon: file_icon(file.language),
                path: file.path.clone(),
                name: file.name.clone().into(),
                relative: file.relative.clone().into(),
            })
            .collect();

        v_flex()
            .gap_0p5()
            .py_1()
            .children(rows.into_iter().map(|file| {
                Button::new(SharedString::from(format!("file:{}", file.relative)))
                    .ghost()
                    .small()
                    .w_full()
                    .justify_start()
                    .selected(file.selected)
                    .accessibility_label(file.relative.clone())
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .pl(gpui_kit::rems(file.depth as f32 * 0.75))
                            .child(
                                Icon::new(file.icon)
                                    .size_4()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(div().min_w_0().truncate().child(file.name.clone())),
                    )
                    .on_click({
                        let path = file.path.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.select_file(path.clone(), window, cx);
                            cx.notify();
                        })
                    })
            }))
    }

    /// What the Files panel shows with no project open: a way in, the recent
    /// projects, and the templates for each runtime.
    fn render_start_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let recent = self.recent_projects().to_vec();
        v_flex()
            .gap_4()
            .pt_2()
            .child(
                Button::new("start-open-folder")
                    .label("Open folder…")
                    .icon(IconName::FolderOpen)
                    .outline()
                    .small()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_folder_dialog(window, cx);
                    })),
            )
            .when(!recent.is_empty(), |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .child(section_header(IconName::Clock, "Recent", cx))
                        .children(recent.iter().map(|project| {
                            Button::new(SharedString::from(format!("recent:{}", project.path)))
                                .ghost()
                                .small()
                                .w_full()
                                .justify_start()
                                .label(project.name.clone())
                                .tooltip(project.path.clone())
                                .on_click({
                                    let path = project.path.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        this.open_recent_project(&path);
                                        cx.notify();
                                    })
                                })
                        }))
                        .child(
                            Button::new("recent-clear")
                                .label("Clear recent")
                                .ghost()
                                .small()
                                .justify_start()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_recent_projects();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                v_flex()
                    .gap_2()
                    .child(section_header(IconName::Sparkles, "New project", cx))
                    .children(TEMPLATES.iter().map(|template| {
                        // One line per template. A Button's size sets its
                        // height along with its typography and padding, so a
                        // stacked name-over-runtime overflows the frame and
                        // collides with the row beneath it; the runtime
                        // belongs in a trailing column instead.
                        Button::new(SharedString::from(format!("template:{}", template.id)))
                            .ghost()
                            .small()
                            .w_full()
                            .justify_start()
                            .tooltip(template.description)
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2()
                                    .child(div().flex_1().min_w_0().truncate().child(template.name))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(template.runtime.label()),
                                    ),
                            )
                            .on_click({
                                let id = template.id;
                                cx.listener(move |this, _, _, cx| {
                                    this.create_from_template(id, cx);
                                    cx.notify();
                                })
                            })
                    })),
            )
    }
}

fn file_icon(language: crate::project::Language) -> IconName {
    use crate::project::Language;
    match language {
        Language::Python => IconName::FileCode,
        Language::Arduino | Language::Cpp | Language::C | Language::Header => IconName::SquareCode,
        Language::Cmake | Language::Ini | Language::Toml => IconName::Settings2,
        Language::Markdown | Language::Text => IconName::FileText,
    }
}

// ---------------------------------------------------------------------------
// Picker lifecycle (state sync + subscriptions)
// ---------------------------------------------------------------------------

impl Workspace {
    /// Ensure the picker entities exist and mirror workspace state into them.
    ///
    /// Called from `render`. Each entity is created once and only updated when
    /// its underlying value changed, so there is no per-frame entity churn and
    /// no confirm-loop.
    pub fn sync_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_port_picker(window, cx);
        self.sync_board_picker(window, cx);
        self.sync_profile_picker(window, cx);
    }

    fn sync_port_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pickers.port.is_none() {
            let state = cx.new(|cx| {
                SelectState::new(SearchableVec::new(Vec::<PortItem>::new()), None, window, cx)
            });
            let subscription = cx.subscribe_in(
                &state,
                window,
                |this, _, event: &SelectEvent<SearchableVec<PortItem>>, _, cx| {
                    if let SelectEvent::Confirm(Some(path)) = event {
                        this.select_port(path.clone());
                        cx.notify();
                    }
                },
            );
            self._subscriptions.push(subscription);
            self.pickers.port = Some(state);
        }
        let Some(state) = self.pickers.port.clone() else {
            return;
        };
        let overrides = self.settings.board_overrides.clone();
        let desired: Vec<PortItem> = self
            .ports
            .iter()
            .map(|port| PortItem {
                path: port.path.clone(),
                board: crate::boards::identify_board(Some(port), &overrides)
                    .board
                    .map(|board| board.name),
            })
            .collect();
        let keys: Vec<String> = desired
            .iter()
            .map(PortItem::title)
            .map(Into::into)
            .collect();
        if self.picker_caches.port_items != keys {
            self.picker_caches.port_items = keys;
            state.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(desired), window, cx);
            });
        }
        if !self.selected_port.is_empty()
            && state.read(cx).selected_value() != Some(&self.selected_port)
        {
            let selected = self.selected_port.clone();
            state.update(cx, |state, cx| {
                state.set_selected_value(&selected, window, cx);
            });
        }
    }

    fn sync_board_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let resolved = self.resolved_board();
        let chip = resolved.board.map(|board| board.chip);

        // The groups only change when the detected chip does, because the
        // leading group is "boards with this chip".
        if self.pickers.board.is_none() || self.picker_caches.board_chip != chip {
            self.picker_caches.board_chip = chip;
            let state = cx.new(|cx| {
                SelectState::new(SearchableVec::new(board_groups(chip)), None, window, cx)
            });
            let subscription = cx.subscribe_in(
                &state,
                window,
                |this, _, event: &SelectEvent<SearchableVec<SelectGroup<BoardItem>>>, _, cx| {
                    if let SelectEvent::Confirm(Some(id)) = event {
                        this.set_board_override(&id.clone());
                        cx.notify();
                    }
                },
            );
            self._subscriptions.push(subscription);
            self.pickers.board = Some(state);
        }
        if let (Some(state), Some(board)) = (self.pickers.board.clone(), resolved.board) {
            let id = board.id.to_string();
            if state.read(cx).selected_value() != Some(&id) {
                state.update(cx, |state, cx| {
                    state.set_selected_value(&id, window, cx);
                });
            }
        }
    }

    fn sync_profile_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items: Vec<ProfileItem> = self
            .available_profiles
            .iter()
            .map(|profile| ProfileItem {
                id: profile.id.clone(),
                label: profile.label.clone(),
                pinned: self.is_profile_pinned(profile),
            })
            .collect();
        let keys: Vec<String> = items.iter().map(|item| item.label.clone()).collect();

        if self.pickers.profile.is_none() {
            let state = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(Vec::<ProfileItem>::new()),
                    None,
                    window,
                    cx,
                )
            });
            let subscription = cx.subscribe_in(
                &state,
                window,
                |this, _, event: &SelectEvent<SearchableVec<ProfileItem>>, _, cx| {
                    if let SelectEvent::Confirm(Some(id)) = event {
                        this.select_profile(&id.clone());
                        cx.notify();
                    }
                },
            );
            self._subscriptions.push(subscription);
            self.pickers.profile = Some(state);
        }
        let Some(state) = self.pickers.profile.clone() else {
            return;
        };
        if self.picker_caches.profile_items != keys {
            self.picker_caches.profile_items = keys;
            state.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(items), window, cx);
            });
        }
        if let Some(active) = self
            .active_profile
            .as_ref()
            .map(|profile| profile.id.clone())
        {
            if state.read(cx).selected_value() != Some(&active) {
                state.update(cx, |state, cx| {
                    state.set_selected_value(&active, window, cx);
                });
            }
        }
    }
}

/// The Select for a picker entity, or a placeholder when it does not exist
/// yet — keeps the inspector's rows on the same spine either way.
pub(crate) fn picker_or_placeholder<D>(
    state: Option<&Entity<SelectState<D>>>,
    placeholder: &'static str,
    label: &'static str,
    enabled: bool,
    cx: &gpui_kit::App,
) -> gpui_kit::AnyElement
where
    D: gpui_kit::component::select::SelectDelegate,
{
    match state {
        Some(state) => Select::new(state)
            .small()
            .w_full()
            .disabled(!enabled)
            .placeholder(placeholder)
            .accessibility_label(label)
            .into_any_element(),
        None => div()
            .h_8()
            .w_full()
            .px_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(placeholder)
            .into_any_element(),
    }
}
