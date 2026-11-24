//! An interactive native terminal backed by the user's login shell in a PTY.
//! Sessions belong to project directories and live until the workspace closes.
mod input;
mod session;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    canvas, div, point, size, App, Bounds, ClipboardItem, Context, ElementInputHandler,
    EntityInputHandler, FocusHandle, Focusable, Font, FontStyle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Pixels,
    Point, Render, ShapedLine, Styled, Task, TextAlign, TextRun, UTF16Selection, UnderlineStyle,
    Window,
};
use session::{starting_directory, Session};
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) struct TerminalView {
    focus: FocusHandle,
    sessions: BTreeMap<PathBuf, Session>,
    directory: PathBuf,
    startup_error: Option<String>,
    bounds: Bounds<Pixels>,
    cell: gpui_kit::Size<Pixels>,
    selection: Option<(usize, usize)>,
    marked: String,
    scroll_delta: f32,
    _poll: Task<()>,
}

impl TerminalView {
    /// Queues a user-requested command in the visible interactive shell.
    /// Used for provider-owned flows such as `codex login`; Anode does not
    /// receive, parse, or persist credentials from that flow.
    pub(crate) fn run_command(&mut self, command: &str, cx: &mut Context<Self>) {
        self.send(format!("{command}\n").into_bytes(), cx);
    }

    pub(crate) fn agent_output(&self, directory: &Path) -> Option<String> {
        self.sessions
            .get(directory)
            .map(|session| crate::device::strip_ansi(&session.captured_output()))
    }
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            if this
                .update(cx, |this, cx| {
                    let mut active_changed = false;
                    for (directory, session) in &mut this.sessions {
                        if session.drain() && directory == &this.directory {
                            active_changed = true;
                        }
                    }
                    if active_changed {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        });
        let mut this = Self {
            focus: cx.focus_handle().tab_stop(true),
            sessions: BTreeMap::new(),
            directory: starting_directory(None),
            startup_error: None,
            bounds: Bounds::default(),
            cell: size(gpui_kit::px(1.), gpui_kit::px(1.)),
            selection: None,
            marked: String::new(),
            scroll_delta: 0.,
            _poll: poll,
        };
        this.open_directory(None, cx);
        this
    }

    pub(crate) fn open_directory(&mut self, project: Option<&Path>, cx: &mut Context<Self>) {
        self.directory = starting_directory(project);
        self.selection = None;
        self.marked.clear();
        self.startup_error = None;
        if !self.sessions.contains_key(&self.directory) {
            match Session::spawn(&self.directory) {
                Ok(session) => {
                    self.sessions.insert(self.directory.clone(), session);
                }
                Err(error) => self.startup_error = Some(error),
            }
        }
        cx.notify();
    }

    fn send(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        self.selection = None;
        if let Some(session) = self.sessions.get_mut(&self.directory) {
            session.send(bytes);
        }
        cx.notify();
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.paste_text(&text, cx);
        }
    }

    fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.selection = None;
        if let Some(session) = self.sessions.get_mut(&self.directory) {
            session.paste(text);
        }
        cx.notify();
    }

    fn selected_text(&self) -> Option<String> {
        let (anchor, head) = self.selection?;
        if anchor == head {
            return None;
        }
        let screen = self.sessions.get(&self.directory)?.screen();
        let cols = usize::from(screen.size().1.get());
        let start = anchor.min(head);
        let end = anchor.max(head);
        let mut text = String::new();
        for row in start / cols..=end / cols {
            let first = if row == start / cols { start % cols } else { 0 };
            let last = if row == end / cols {
                end % cols
            } else {
                cols - 1
            };
            let mut line = String::new();
            for col in first..=last {
                if let Some(cell) = screen.cell(row as u16, col as u16) {
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    if cell.contents().is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(cell.contents());
                    }
                }
            }
            text.push_str(line.trim_end());
            if row < end / cols && !screen.row_wrapped(row as u16) {
                text.push('\n');
            }
        }
        Some(text)
    }

    fn copy_selection(&self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let clipboard = key.modifiers.platform || (key.modifiers.control && key.modifiers.shift);
        if clipboard && key.key.eq_ignore_ascii_case("v") {
            self.paste_clipboard(cx);
        } else if clipboard && key.key.eq_ignore_ascii_case("c") {
            self.copy_selection(cx);
        } else if let Some(bytes) = input::key_bytes(
            key,
            self.sessions
                .get(&self.directory)
                .is_some_and(|session| session.screen().application_cursor()),
        ) {
            self.marked.clear();
            self.send(bytes, cx);
        } else {
            return; // Let native text input handle printable and composed characters.
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    fn cell_index(&self, position: Point<Pixels>) -> usize {
        let (rows, cols) = self
            .sessions
            .get(&self.directory)
            .map(|session| session.screen().size())
            .map(|(rows, cols)| (rows.get(), cols.get()))
            .unwrap_or((1, 1));
        let row = ((position.y - self.bounds.top()) / self.cell.height)
            .floor()
            .max(0.) as usize;
        let col = ((position.x - self.bounds.left()) / self.cell.width)
            .floor()
            .max(0.) as usize;
        row.min(usize::from(rows - 1)) * usize::from(cols) + col.min(usize::from(cols - 1))
    }

    fn frame(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Frame {
        let font_size = window.rem_size() * 0.8125;
        let font = Font {
            family: cx.theme().mono_font_family.clone(),
            ..Default::default()
        };
        let fg = crate::theme::terminal_foreground(cx);
        let bg = crate::theme::terminal_background(cx);
        let sample = window.text_system().shape_line(
            "M".into(),
            font_size,
            &[TextRun {
                len: 1,
                font: font.clone(),
                color: fg,
                ..Default::default()
            }],
            None,
        );
        self.cell = size(sample.width.max(gpui_kit::px(1.)), font_size * 1.5);
        self.bounds = bounds;
        let rows = ((bounds.size.height / self.cell.height).floor() as u16).clamp(1, 500);
        let cols = ((bounds.size.width / self.cell.width).floor() as u16).clamp(1, 1000);
        let Some(session) = self.sessions.get_mut(&self.directory) else {
            return Frame::default();
        };
        if session.screen().size().0.get() != rows || session.screen().size().1.get() != cols {
            self.selection = None;
            session.resize(rows, cols);
        }
        let screen = session.screen();
        let position = screen.cursor_position();
        let focused = self.focus.is_focused(window);
        let show_cursor =
            !screen.hide_cursor() && screen.scrollback() == 0 && session.exit.is_none();
        let cursor = show_cursor.then_some(Cursor {
            row: position.0,
            column: position.1,
            focused,
        });
        let selection = self
            .selection
            .map(|(a, b)| a.min(b)..=a.max(b))
            .filter(|range| range.start() != range.end());
        let mut lines = Vec::with_capacity(usize::from(rows));
        for row in 0..rows {
            let mut text = String::new();
            let mut runs: Vec<TextRun> = Vec::new();
            for col in 0..cols {
                let Some(cell) = screen.cell(row, col) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let contents = if cell.contents().is_empty() {
                    " "
                } else {
                    cell.contents()
                };
                let mut foreground = crate::theme::terminal_ansi_color(cell.fgcolor(), fg);
                let mut background = crate::theme::terminal_ansi_color(cell.bgcolor(), bg);
                if cell.inverse() {
                    std::mem::swap(&mut foreground, &mut background);
                }
                if cell.dim() {
                    foreground = foreground.opacity(0.65);
                }
                if selection.as_ref().is_some_and(|range| {
                    range.contains(&(usize::from(row) * usize::from(cols) + usize::from(col)))
                }) {
                    background = cx.theme().primary.opacity(0.4);
                }
                // The caret is painted as its own quad beneath this line, so
                // this cell suppresses its background rather than drawing one
                // over it, and inverts its glyph to read against the caret.
                let under_cursor = cursor.is_some_and(|cursor| {
                    cursor.focused && (cursor.row, cursor.column) == (row, col)
                });
                if under_cursor {
                    foreground = bg;
                }
                let run = TextRun {
                    len: contents.len(),
                    font: Font {
                        weight: if cell.bold() {
                            FontWeight::BOLD
                        } else {
                            font.weight
                        },
                        style: if cell.italic() {
                            FontStyle::Italic
                        } else {
                            FontStyle::Normal
                        },
                        ..font.clone()
                    },
                    color: foreground,
                    background_color: (!under_cursor).then_some(background),
                    underline: cell.underline().then_some(UnderlineStyle {
                        thickness: font_size / 14.,
                        color: Some(foreground),
                        wavy: false,
                    }),
                    strikethrough: None,
                };
                if let Some(previous) = runs.last_mut().filter(|previous| {
                    previous.font == run.font
                        && previous.color == run.color
                        && previous.background_color == run.background_color
                        && previous.underline == run.underline
                }) {
                    previous.len += run.len;
                } else {
                    runs.push(run);
                }
                text.push_str(contents);
            }
            lines.push(
                window
                    .text_system()
                    .shape_line(text.into(), font_size, &runs, None),
            );
        }
        let marked = (!self.marked.is_empty()).then(|| {
            let run = TextRun {
                len: self.marked.len(),
                font,
                color: fg,
                background_color: Some(bg),
                underline: Some(UnderlineStyle {
                    thickness: font_size / 14.,
                    color: Some(fg),
                    wavy: false,
                }),
                ..Default::default()
            };
            (
                window.text_system().shape_line(
                    self.marked.clone().into(),
                    font_size,
                    &[run],
                    None,
                ),
                position,
            )
        });
        Frame {
            lines,
            cell: self.cell,
            marked,
            cursor,
            caret: crate::theme::terminal_caret(cx),
        }
    }
}

/// Where the terminal's caret is, and whether it owns the keyboard.
#[derive(Clone, Copy)]
struct Cursor {
    row: u16,
    column: u16,
    /// A focused terminal fills its caret; an unfocused one outlines it.
    focused: bool,
}

#[derive(Default)]
struct Frame {
    lines: Vec<ShapedLine>,
    cell: gpui_kit::Size<Pixels>,
    marked: Option<(ShapedLine, (u16, u16))>,
    cursor: Option<Cursor>,
    caret: gpui_kit::Hsla,
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let shell = self
            .sessions
            .get(&self.directory)
            .map(|session| session.shell.clone())
            .unwrap_or_else(|| "Shell".into());
        let shell_name = Path::new(&shell)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let exited = self
            .sessions
            .get(&self.directory)
            .and_then(|session| session.exit.clone());
        let error = self.startup_error.clone().or_else(|| {
            self.sessions
                .get(&self.directory)
                .and_then(|session| session.error.clone())
        });
        let can_restart = exited.is_some() || error.is_some();
        v_flex()
            .size_full()
            .min_h_0()
            .bg(crate::theme::terminal_background(cx))
            .text_color(crate::theme::terminal_foreground(cx))
            .child(
                h_flex()
                    .h_9()
                    .flex_shrink_0()
                    .px_3()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().sidebar)
                    .text_color(cx.theme().foreground)
                    .child(div().text_xs().child(shell_name))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .child(self.directory.display().to_string()),
                    )
                    .child(
                        Button::new("terminal-copy")
                            .icon(IconName::Copy)
                            .ghost()
                            .small()
                            .tooltip("Copy selected text (⌘C)")
                            .disabled(self.selected_text().is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.copy_selection(cx))),
                    )
                    .child(
                        Button::new("terminal-paste")
                            .label("Paste")
                            .ghost()
                            .small()
                            .tooltip("Paste into shell (⌘V)")
                            .disabled(exited.is_some() || self.startup_error.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.paste_clipboard(cx);
                                this.focus.focus(window, cx);
                            })),
                    )
                    .when(can_restart, |this| {
                        this.child(
                            Button::new("terminal-restart")
                                .label("Restart shell")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.sessions.remove(&this.directory);
                                    let directory = this.directory.clone();
                                    this.open_directory(Some(&directory), cx);
                                    this.focus.focus(window, cx);
                                })),
                        )
                    }),
            )
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .when_some(exited, |this, status| {
                this.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .child(format!("Shell exited: {status}")),
                )
            })
            .child(
                div()
                    .id("terminal-grid")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .p_2()
                    .overflow_hidden()
                    .track_focus(&self.focus)
                    .key_context("Terminal")
                    .cursor_text()
                    .on_key_down(cx.listener(Self::key_down))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            this.focus.focus(window, cx);
                            let cell = this.cell_index(event.position);
                            this.selection = Some((cell, cell));
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(|this, event: &gpui_kit::MouseMoveEvent, _, cx| {
                            if event.pressed_button == Some(MouseButton::Left) {
                                if let Some((anchor, _)) = this.selection {
                                    this.selection =
                                        Some((anchor, this.cell_index(event.position)));
                                    cx.notify();
                                }
                            }
                        }),
                    )
                    .on_scroll(cx.listener(|this, event: &gpui_kit::ScrollEvent, _, cx| {
                        this.scroll_delta +=
                            event.delta.pixel_delta(this.cell.height).y / this.cell.height;
                        let rows = this.scroll_delta.trunc() as i32;
                        this.scroll_delta -= rows as f32;
                        if let Some(session) = this.sessions.get_mut(&this.directory) {
                            if session.screen().alternate_screen() {
                                let code = if rows > 0 { b"\x1bOA" } else { b"\x1bOB" };
                                for _ in 0..rows.unsigned_abs().min(100) {
                                    session.send(code.to_vec());
                                }
                            } else {
                                session.scroll(rows);
                            }
                        }
                        this.selection = None;
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .child(
                        canvas(
                            move |bounds, window, cx| {
                                entity.update(cx, |this, cx| this.frame(bounds, window, cx))
                            },
                            move |bounds, frame, window, cx| {
                                // The caret goes down before the glyphs, so the
                                // character under it stays readable on top.
                                if let Some(cursor) = frame.cursor {
                                    let caret = Bounds {
                                        origin: bounds.origin
                                            + point(
                                                frame.cell.width * f32::from(cursor.column),
                                                frame.cell.height * f32::from(cursor.row),
                                            ),
                                        size: frame.cell,
                                    };
                                    window.paint_quad(if cursor.focused {
                                        gpui_kit::fill(caret, frame.caret)
                                    } else {
                                        gpui_kit::outline(
                                            caret,
                                            frame.caret,
                                            gpui_kit::BorderStyle::Solid,
                                        )
                                    });
                                }
                                for (row, line) in frame.lines.iter().enumerate() {
                                    _ = line.paint(
                                        bounds.origin
                                            + point(
                                                gpui_kit::px(0.),
                                                frame.cell.height * row as f32,
                                            ),
                                        frame.cell.height,
                                        TextAlign::Left,
                                        None,
                                        window,
                                        cx,
                                    );
                                }
                                if let Some((line, (row, col))) = frame.marked {
                                    _ = line.paint(
                                        bounds.origin
                                            + point(
                                                frame.cell.width * f32::from(col),
                                                frame.cell.height * f32::from(row),
                                            ),
                                        frame.cell.height,
                                        TextAlign::Left,
                                        None,
                                        window,
                                        cx,
                                    );
                                }
                                let focus = paint_entity.read(cx).focus.clone();
                                window.handle_input(
                                    &focus,
                                    ElementInputHandler::new(bounds, paint_entity),
                                    cx,
                                );
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}

// Native text services own composition. Only committed text is sent to the PTY.
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let utf16: Vec<_> = self.marked.encode_utf16().collect();
        let range = range.start.min(utf16.len())..range.end.max(range.start).min(utf16.len());
        *adjusted = Some(range.clone());
        Some(String::from_utf16_lossy(&utf16[range]))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.marked.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked.clear();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked.clear();
        self.send(text.as_bytes().to_vec(), cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = text.into();
        cx.notify();
    }
    fn paste(&mut self, item: ClipboardItem, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = item.text() {
            self.paste_text(&text, cx);
        }
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (row, col) = self
            .sessions
            .get(&self.directory)?
            .screen()
            .cursor_position();
        Some(Bounds {
            origin: self.bounds.origin
                + point(
                    self.cell.width * f32::from(col),
                    self.cell.height * f32::from(row),
                ),
            size: self.cell,
        })
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}
