//! The Anode workspace UI.
//!
//! Translated from `src/App.tsx` and its child components. Each React
//! component became a render helper on `Workspace` (or a small entity where it
//! owns retained state). Theme values come from `cx.theme()`; layout uses the
//! rem-based helpers.
//!
//! Select controls own their `SelectState` as child entities created once per
//! picker and kept in `Workspace::pickers`; their `SelectEvent::Confirm`
//! subscriptions commit selections back into workspace state.

use crate::boards::{self, BoardType};
use crate::panels::Tool;
use crate::profiles::Confidence;
use crate::project::TEMPLATES;
use crate::workspace_state::{LogKind, Workspace};

use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::Input;
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, img, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled,
    StyledImage as _, Window,
};

// ---------------------------------------------------------------------------
// Select items
// ---------------------------------------------------------------------------

/// A Select item wrapping a port path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PortItem(String);

impl SelectItem for PortItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.0.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

/// A Select item wrapping a board id from the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BoardItem {
    id: &'static str,
    value_string: String,
}

impl SelectItem for BoardItem {
    type Value = String;

    fn title(&self) -> SharedString {
        boards::board_by_id(self.id)
            .map(|b| SharedString::from(b.name))
            .unwrap_or_else(|| self.id.into())
    }

    fn value(&self) -> &Self::Value {
        &self.value_string
    }
}

/// A Select item wrapping an execution profile id.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileItem(String, String);

impl SelectItem for ProfileItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.1.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

/// A Select item wrapping a legacy board kind for the flash dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FlashBoardItem {
    board: BoardType,
    value: String,
}

impl SelectItem for FlashBoardItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.board.label().into()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

impl BoardType {
    /// A stable string identity for Select values.
    fn value_key(&self) -> String {
        format!("{self:?}")
    }

    fn item(&self) -> FlashBoardItem {
        FlashBoardItem {
            board: *self,
            value: self.value_key(),
        }
    }

    fn from_value_key(key: &str) -> Option<BoardType> {
        Some(match key {
            "Espressif" => BoardType::Espressif,
            "M5StackAtom" => BoardType::M5StackAtom,
            "M5StampS3" => BoardType::M5StampS3,
            "RaspberryPi" => BoardType::RaspberryPi,
            "Arduino" => BoardType::Arduino,
            "Unknown" => BoardType::Unknown,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Retained pickers
// ---------------------------------------------------------------------------

/// The workspace's retained Select entities. Created once when the workspace
/// is built (or first needed) so their selection state survives re-renders.
#[derive(Default)]
pub struct Pickers {
    port: Option<Entity<SelectState<SearchableVec<PortItem>>>>,
    board: Option<Entity<SelectState<SearchableVec<BoardItem>>>>,
    profile: Option<Entity<SelectState<SearchableVec<ProfileItem>>>>,
    flash_board: Option<Entity<SelectState<SearchableVec<FlashBoardItem>>>>,
}

/// Pickers are rebuilt only when their item set changes; these caches hold
/// the last-seen keys so `sync_pickers` stays cheap on every frame.
#[derive(Default)]
pub struct PickerCaches {
    pub port_items: Vec<String>,
    pub board_items: Vec<&'static str>,
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

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .child(self.render_header(cx))
            .when(
                self.connection_dropped && !self.selected_port.is_empty(),
                |this| this.child(self.render_connection_banner(cx)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.render_activity_bar(window, cx))
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
    }
}

/// Brand lockup using the existing wordmark’s standalone a glyph.
fn render_logo(_cx: &Context<Workspace>) -> impl IntoElement {
    h_flex()
        .gap_2()
        .child(
            img(crate::assets::MARK_PATH)
                .size_6()
                .object_fit(gpui_kit::ObjectFit::Contain),
        )
        .child(
            div()
                .text_lg()
                .font_weight(gpui_kit::FontWeight::BOLD)
                .child("anode"),
        )
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

impl Workspace {
    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::component::TitleBar::new()
            .h_12()
            .bg(cx.theme().sidebar)
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .px_3()
                    .justify_between()
                    .child(
                        h_flex().gap_4().child(render_logo(cx)).child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Hardware IDE"),
                        ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("nav-boards")
                                    .label("Boards")
                                    .icon(IconName::Cpu)
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_tool(Tool::Boards, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("nav-code")
                                    .label("Code")
                                    .icon(IconName::Code)
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_tool(Tool::Editor, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("nav-run")
                                    .label("Run")
                                    .icon(IconName::Play)
                                    .ghost()
                                    .small()
                                    .disabled(self.current_file.is_none() || self.device_busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.run_code();
                                        this.show_tool(Tool::Output, window, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("nav-output")
                                    .label("Output")
                                    .icon(IconName::SquareTerminal)
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_tool(Tool::Output, window, cx)
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .when_some(self.pickers.profile.clone(), |this, state| {
                                this.child(
                                    Select::new(&state)
                                        .small()
                                        .w(gpui_kit::rems(12.0))
                                        .placeholder("Runtime…")
                                        .accessibility_label("Active runtime profile"),
                                )
                            })
                            .when_some(self.pickers.port.clone(), |this, state| {
                                this.child(
                                    Select::new(&state)
                                        .small()
                                        .w(gpui_kit::rems(14.0))
                                        .placeholder("Connect a board…")
                                        .accessibility_label("Active serial port"),
                                )
                            })
                            .child(
                                Button::new("open-folder-header")
                                    .icon(IconName::FolderOpen)
                                    .ghost()
                                    .small()
                                    .tooltip("Open project folder…")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_folder_dialog(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("theme-toggle")
                                    .icon(IconName::Sun)
                                    .ghost()
                                    .small()
                                    .tooltip("Switch light / dark appearance")
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
            .border_color(cx.theme().warning.opacity(0.2))
            .text_color(cx.theme().warning)
            .text_size(gpui_kit::rems(0.75))
            .child(Icon::new(IconName::TriangleAlert).size_4())
            .child(format!(
                "Connection lost on {}. Waiting for reconnect...",
                self.selected_port
            ))
    }

    // -- Activity bar ---------------------------------------------------------

    fn render_activity_bar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .w_12()
            .h_full()
            .items_center()
            .py_3()
            .gap_3()
            .bg(cx.theme().sidebar)
            .child(
                Button::new("activity-files")
                    .icon(IconName::Files)
                    .ghost()
                    .small()
                    .tooltip("Show project files")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.show_tool(Tool::Files, window, cx)),
                    ),
            )
            .child(
                Button::new("activity-boards")
                    .icon(IconName::Cpu)
                    .ghost()
                    .small()
                    .tooltip("Show recent boards")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.show_tool(Tool::Boards, window, cx)),
                    ),
            )
            .child(
                Button::new("activity-repl")
                    .icon(IconName::SquareTerminal)
                    .ghost()
                    .small()
                    .tooltip("Show REPL")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.show_tool(Tool::Repl, window, cx)),
                    ),
            )
            .child(div().flex_1())
            .child(
                Button::new("reset-panels")
                    .icon(IconName::PanelsTopLeft)
                    .ghost()
                    .small()
                    .tooltip("Reset panel layout")
                    .on_click(cx.listener(|this, _, window, cx| this.reset_panels(window, cx))),
            )
            .child(
                Button::new("activity-folder")
                    .icon(IconName::FolderOpen)
                    .ghost()
                    .small()
                    .tooltip("Open project folder…")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_folder_dialog(window, cx)),
                    ),
            )
    }

    pub(crate) fn render_editor_tabs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_index = self
            .open_tabs
            .iter()
            .position(|t| Some(&t.path) == self.active_tab.as_ref());
        let mut bar = TabBar::new("editor-tabs");
        for tab in &self.open_tabs {
            let label = if tab.is_dirty {
                format!("{} •", tab.name)
            } else {
                tab.name.clone()
            };
            bar = bar.child(Tab::new().label(label));
        }
        if let Some(index) = active_index {
            bar = bar.selected_index(index);
        }
        h_flex()
            .h_12()
            .px_2()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(bar.on_click(cx.listener(|this, index: &usize, window, cx| {
                        this.select_tab(*index, window, cx);
                        cx.notify();
                    }))),
            )
            .child(
                Button::new("save-all")
                    .icon(IconName::Save)
                    .ghost()
                    .small()
                    .tooltip("Save all files (⇧⌘S)")
                    .disabled(self.project_files.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_all_files();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("editor-run")
                    .label(if self.is_arduino() { "Compile" } else { "Run" })
                    .icon(IconName::Play)
                    .outline()
                    .small()
                    .disabled(self.current_file.is_none() || self.device_busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.is_arduino() {
                            this.arduino_compile();
                        } else {
                            this.run_code();
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("editor-stop")
                    .icon(IconName::Square)
                    .ghost()
                    .small()
                    .tooltip("Stop code")
                    .disabled(self.selected_port.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stop_code();
                        cx.notify();
                    })),
            )
    }

    pub(crate) fn render_editor(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.current_file.is_none() {
            // Empty state (the Electron app's welcome screen, condensed):
            // wordmark, one line of guidance, and the way in.
            return v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_3()
                .child(render_logo(cx))
                .child(
                    div()
                        .text_size(gpui_kit::rems(0.8125))
                        .text_color(cx.theme().muted_foreground)
                        .child("MicroPython & Arduino IDE"),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .mt_2()
                        .child(
                            Button::new("empty-open-folder")
                                .label("Open Project Folder…")
                                .outline()
                                .small()
                                .icon(IconName::FolderOpen)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_folder_dialog(window, cx);
                                })),
                        )
                        .child(
                            Button::new("empty-show-repl")
                                .label("Open REPL")
                                .ghost()
                                .small()
                                .icon(IconName::SquareTerminal)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_tool(Tool::Repl, window, cx)
                                })),
                        ),
                );
        }
        // The component-layer `Editor`, not base's: its `Input::render` is what
        // installs the tree-sitter highlighter factory and the theme's syntax
        // colors. Base's bare `Editor` renders the state without either, which
        // is why the editor showed plain text.
        v_flex().flex_1().min_h_0().bg(cx.theme().background).child(
            div().h_full().child(
                gpui_kit::component::input::Editor::new(&self.editor)
                    .appearance(false)
                    .bordered(false),
            ),
        )
    }

    // -- Bottom panel: shell log / REPL ---------------------------------------

    pub(crate) fn render_shell_log(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.log_entries.clone();
        let log_scroll = self.log_scroll.clone();
        div()
            .id("shell-log")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&log_scroll)
            .px_3()
            .py_2()
            .child(
                v_flex()
                    .gap_0p5()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(gpui_kit::rems(0.75))
                    .when(entries.is_empty(), |this| {
                        this.child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child("Shell output appears here."),
                        )
                    })
                    .children(entries.into_iter().map(|entry| {
                        let color = match entry.kind {
                            LogKind::Error => cx.theme().danger,
                            LogKind::Warn => cx.theme().warning,
                            LogKind::Ok => cx.theme().success,
                            LogKind::Install => cx.theme().info,
                            LogKind::Flash => cx.theme().cyan,
                            LogKind::Info => cx.theme().foreground,
                        };
                        div().text_color(color).child(entry.message)
                    })),
            )
    }

    pub(crate) fn render_repl(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.repl_connected;
        let port = self.selected_port.clone();
        let output = self.repl_output.clone();

        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("repl-toggle")
                            .label(if connected { "Disconnect" } else { "Connect" })
                            .small()
                            .compact()
                            .outline()
                            .disabled(port.is_empty())
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
                        Button::new("repl-break")
                            .label("^C")
                            .small()
                            .compact()
                            .ghost()
                            .disabled(!connected)
                            .tooltip("Interrupt running code")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.repl_break();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("repl-reset")
                            .label("Soft Reset")
                            .small()
                            .compact()
                            .ghost()
                            .disabled(!connected)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.repl_soft_reset();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("repl-clear")
                            .icon(IconName::CircleX)
                            .small()
                            .compact()
                            .ghost()
                            .tooltip("Clear output")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.repl_output.clear();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id("repl-output")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(gpui_kit::rems(0.75))
                            .text_color(if output.is_empty() {
                                cx.theme().muted_foreground
                            } else {
                                cx.theme().foreground
                            })
                            .child(if output.is_empty() {
                                format!("Connect to {port} to open the REPL.")
                            } else {
                                output
                            }),
                    ),
            )
            .child(
                h_flex()
                    .px_3()
                    .py_1p5()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(Input::new(&self.repl_input).small().flex_1())
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

    fn render_status_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = !self.selected_port.is_empty()
            && !self.connection_dropped
            && self
                .ports
                .iter()
                .any(|port| port.path == self.selected_port);
        let port_state = if connected {
            "Connected"
        } else {
            "Disconnected"
        };
        let board_label = self
            .detected_board_label()
            .unwrap_or_else(|| "No Board".into());
        let file_label = self
            .current_file
            .as_ref()
            .map(|f| f.name.clone())
            .unwrap_or_else(|| "No file".into());
        let mode = if self.is_arduino() {
            "Arduino"
        } else if self.is_circuit_python() {
            "CircuitPython"
        } else {
            "MicroPython"
        };

        h_flex()
            .h_7()
            .px_3()
            .justify_between()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().muted_foreground)
            .text_size(gpui_kit::rems(0.75))
            .font_family(cx.theme().mono_font_family.clone())
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().size_2().rounded_full().bg(if connected {
                                cx.theme().success
                            } else {
                                cx.theme().muted_foreground
                            }))
                            .child(port_state),
                    )
                    .child("·")
                    .child(board_label)
                    .child("|")
                    .child(mode),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(file_label)
                    .child("UTF-8")
                    .child("LF"),
            )
    }

    // -- Sidebar ---------------------------------------------------------------

    pub(crate) fn render_sidebar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex().size_full().bg(cx.theme().sidebar).child(
            div()
                .id("sidebar-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_4()
                .child(v_flex().gap_6().child(self.render_project_files(cx))),
        )
    }

    fn render_project_files(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let section_title = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "PROJECT".into());

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_size(gpui_kit::rems(0.75))
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{section_title}/")),
            )
            .when(self.project_files.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(gpui_kit::rems(0.8125))
                        .text_color(cx.theme().muted_foreground)
                        .child("No files in project"),
                )
            })
            .children(self.project_files.clone().into_iter().map(|file| {
                let selected = self
                    .current_file
                    .as_ref()
                    .is_some_and(|f| f.path == file.path);
                Button::new(format!("project-file:{}", file.path.to_string_lossy()))
                    .ghost()
                    .small()
                    .w_full()
                    .justify_start()
                    .selected(selected)
                    .icon(IconName::FileText)
                    .label(file.name.clone())
                    .on_click({
                        let path = file.path.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.select_file(path.clone(), window, cx);
                            cx.notify();
                        })
                    })
            }))
            .child(self.render_projects_section(cx))
    }

    /// Recent projects + templates. The Electron welcome screen folded into
    /// this sidebar section: opening a project and starting from a template
    /// stay reachable from a fresh launch.
    fn render_projects_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .pt_4()
            .child(
                div()
                    .text_size(gpui_kit::rems(0.75))
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("Projects"),
            )
            .child(
                Button::new("open-project")
                    .label("Open Folder…")
                    .outline()
                    .small()
                    .icon(IconName::FolderOpen)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_folder_dialog(window, cx);
                    })),
            )
            .when(!self.recent_projects().is_empty(), |this| {
                this.children(self.recent_projects().iter().map(|project| {
                    Button::new(format!("recent-project:{}", project.path))
                        .ghost()
                        .small()
                        .w_full()
                        .justify_start()
                        .label(project.name.clone())
                        .on_click({
                            let path = project.path.clone();
                            cx.listener(move |this, _, _, cx| {
                                this.open_recent_project(&path);
                                cx.notify();
                            })
                        })
                }))
                .child(
                    Button::new("clear-recents")
                        .label("Clear Recent Projects")
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_recent_projects();
                            cx.notify();
                        })),
                )
            })
            .child(
                div()
                    .pt_3()
                    .text_size(gpui_kit::rems(0.75))
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("New from template"),
            )
            .children(TEMPLATES.iter().map(|template| {
                Button::new(format!("template:{}", template.id))
                    .ghost()
                    .small()
                    .w_full()
                    .justify_start()
                    .label(template.name)
                    .on_click({
                        let template_id = template.id;
                        cx.listener(move |this, _, _, cx| {
                            this.create_from_template(template_id, cx);
                            cx.notify();
                        })
                    })
            }))
    }

    pub(crate) fn render_recent_boards(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let records = self.settings.recent_boards.clone();
        v_flex().size_full().min_h_0().bg(cx.theme().sidebar)
            .child(h_flex().px_3().py_2().gap_2().justify_between()
                .child(div().text_xs().font_weight(gpui_kit::FontWeight::SEMIBOLD).child("Recently connected"))
                .child(Button::new("refresh-boards").icon(IconName::RefreshCw).ghost().small().tooltip("Scan USB devices")
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_ports(cx)))))
            .child(div().id("recent-boards-list").flex_1().min_h_0().overflow_y_scroll()
                .child(v_flex().p_2().gap_1()
                    .when(records.is_empty(), |this| this.child(v_flex().p_2().gap_2()
                        .child(div().text_sm().child("Connect a board over USB"))
                        .child(div().text_xs().text_color(cx.theme().muted_foreground)
                            .child("Boards appear here automatically and stay in your recent history. Open a board to flash firmware or upload a project."))))
                    .children(records.into_iter().map(|record| {
                        let port = self.ports.iter().find(|port| crate::hardware::device_key(port) == record.key);
                        let connected = port.is_some();
                        let path = port.map(|port| port.path.as_str()).unwrap_or(&record.path).to_string();
                        v_flex().id(record.key.clone()).p_2().gap_1().rounded_md()
                            .hover(|style| style.bg(cx.theme().accent))
                            .child(h_flex().gap_2()
                                .child(Icon::new(IconName::Cpu).size_4().text_color(if connected { cx.theme().success } else { cx.theme().muted_foreground }))
                                .child(div().min_w_0().flex_1().truncate().font_weight(gpui_kit::FontWeight::MEDIUM).child(record.name.clone())))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).truncate().child(path))
                            .child(h_flex().justify_between().gap_2()
                                .child(div().text_xs().text_color(if connected { cx.theme().success } else { cx.theme().muted_foreground })
                                    .child(if connected { "Connected" } else { "Offline" }))
                                .child(Button::new(format!("open-board-{}", record.key)).label("Open panel").small().ghost()
                                    .on_click(cx.listener(move |this, _, window, cx| this.open_board_panel(record.clone(), window, cx)))))
                    }))))
    }
}

// ---------------------------------------------------------------------------
// Picker lifecycle (state sync + subscriptions)
// ---------------------------------------------------------------------------

impl Workspace {
    /// Ensure the picker entities exist and mirror workspace state into them.
    /// Called from `render` before the pickers are displayed; each entity is
    /// created once and only updated when its underlying value changed, so
    /// there is no per-frame entity churn and no confirm-loop.
    pub fn sync_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Port picker.
        if self.pickers.port.is_none() {
            self.pickers.port = Some(cx.new(|cx| {
                SelectState::new(SearchableVec::new(Vec::<PortItem>::new()), None, window, cx)
            }));
            let subscription = cx.subscribe_in(
                self.pickers.port.as_ref().unwrap(),
                window,
                |this, _state, event: &SelectEvent<SearchableVec<PortItem>>, _window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        this.select_port(value.clone());
                        cx.notify();
                    }
                },
            );
            self._subscriptions.push(subscription);
        }
        if let Some(state) = &self.pickers.port {
            let desired: Vec<PortItem> = self
                .ports
                .iter()
                .map(|p| PortItem(p.path.clone()))
                .collect();
            let next: Vec<String> = desired.iter().map(|i| i.0.clone()).collect();
            if self.picker_caches.port_items != next {
                self.picker_caches.port_items = next.clone();
                state.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(desired), window, cx);
                });
            }
            let selected = state.read(cx).selected_value().cloned();
            if selected.as_deref() != Some(self.selected_port.as_str())
                && !self.selected_port.is_empty()
            {
                state.update(cx, |state, cx| {
                    state.set_selected_value(&self.selected_port.clone(), window, cx);
                });
            }
        }

        // Board picker (recreated when the candidate set changes).
        let candidates = self.resolved_board().candidates;
        let board_key: Vec<&'static str> = candidates.clone();
        let needs_new =
            !candidates.is_empty() && self.picker_caches.board_items != board_key.clone();
        if needs_new {
            self.picker_caches.board_items = board_key;
            {
                let state = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(
                            candidates
                                .iter()
                                .map(|id| BoardItem {
                                    id,
                                    value_string: id.to_string(),
                                })
                                .collect::<Vec<BoardItem>>(),
                        ),
                        None,
                        window,
                        cx,
                    )
                });
                let subscription = cx.subscribe_in(
                    &state,
                    window,
                    |this, _state, event: &SelectEvent<SearchableVec<BoardItem>>, _window, cx| {
                        if let SelectEvent::Confirm(Some(value)) = event {
                            this.set_board_override(&value.clone());
                            cx.notify();
                        }
                    },
                );
                self._subscriptions.push(subscription);
                self.pickers.board = Some(state);
            }
        }

        // Runtime profile picker (recreated when the profile set changes).
        let profiles: Vec<(String, String)> = self
            .available_profiles
            .iter()
            .map(|p| {
                let star = if p.confidence == Confidence::High && !p.is_generic {
                    " ★"
                } else {
                    ""
                };
                (p.id.clone(), format!("{}{star}", p.board_name))
            })
            .collect();
        let active_profile_id = self.active_profile.as_ref().map(|p| p.id.clone());
        let profile_key: Vec<String> = profiles.iter().map(|(id, _)| id.clone()).collect();
        let needs_new = !profiles.is_empty() && self.picker_caches.profile_items != profile_key;
        if needs_new {
            self.picker_caches.profile_items = profile_key;
            {
                let state = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(
                            profiles
                                .iter()
                                .map(|(id, title)| ProfileItem(id.clone(), title.clone()))
                                .collect::<Vec<ProfileItem>>(),
                        ),
                        None,
                        window,
                        cx,
                    )
                });
                let subscription = cx.subscribe_in(
                    &state,
                    window,
                    |this, _state, event: &SelectEvent<SearchableVec<ProfileItem>>, _window, cx| {
                        if let SelectEvent::Confirm(Some(value)) = event {
                            this.select_profile(value);
                            cx.notify();
                        }
                    },
                );
                self._subscriptions.push(subscription);
                self.pickers.profile = Some(state);
            }
        }
        if let (Some(state), Some(active)) = (&self.pickers.profile, &active_profile_id) {
            let selected = state.read(cx).selected_value().cloned();
            if selected.as_deref() != Some(active.as_str()) {
                state.update(cx, |state, cx| {
                    state.set_selected_value(active, window, cx);
                });
            }
        }
    }

    /// Open the flash dialog. The board-family Select is a persistent entity
    /// created before opening, so rebuilding the dialog keeps the choice.
    pub(crate) fn open_flash_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Recreate the flash board picker each opening (cheap; the choice is
        // seeded from detection and read on confirm).
        let detected = self.flash_board_choice;
        let items: Vec<FlashBoardItem> = [
            BoardType::Espressif,
            BoardType::M5StackAtom,
            BoardType::M5StampS3,
            BoardType::RaspberryPi,
            BoardType::Arduino,
        ]
        .into_iter()
        .map(|b| b.item())
        .collect();
        let state = cx.new(|cx| SelectState::new(SearchableVec::new(items), None, window, cx));
        state.update(cx, |state, cx| {
            state.set_selected_value(&detected.value_key(), window, cx);
        });
        let subscription = cx.subscribe_in(&state, window, |this, state, event, _, _cx| {
            if let SelectEvent::Confirm(Some(value)) = event {
                if let Some(board) = BoardType::from_value_key(value) {
                    this.flash_board_choice = board;
                }
                let _ = state;
            }
        });
        self._subscriptions.push(subscription);
        self.pickers.flash_board = Some(state);

        let port = self.selected_port.clone();
        type ConfirmFn = dyn Fn((), &mut Window, &mut gpui_kit::App);
        let confirm: std::rc::Rc<ConfirmFn> = std::rc::Rc::new(cx.processor(
            move |this: &mut Workspace, _: (), window: &mut Window, cx| {
                window.close_dialog(cx);
                this.confirm_flash(this.flash_board_choice);
            },
        ));
        let _tool = self.flash_board_choice.tool().unwrap_or("esptool.py");

        // The dialog body reads the picker entity; the confirm button
        // dispatches through cx.processor so it runs with workspace access.
        let picker = self.pickers.flash_board.clone().expect("flash picker");
        let confirm_state = picker.clone();
        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .title("Flash Firmware")
                .child(
                    v_flex()
                        .gap_3()
                        .child(format!("Detected board on {port}."))
                        .child("Pick the board family to flash, then confirm. The REPL is released while esptool has the port.")
                        .child(Select::new(&confirm_state).small()),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("flash-cancel")
                                .label("Cancel")
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("flash-confirm")
                                .primary()
                                .label("Flash Firmware")
                                .on_click({
                                    let confirm = confirm.clone();
                                    move |_, window, cx| {
                                        confirm((), window, cx);
                                    }
                                }),
                        ),
                )
        });
    }
}
