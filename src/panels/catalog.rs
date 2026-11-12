//! The supported-boards catalog.
//!
//! A browsable answer to "will Anode program my board?". It owns its search
//! query and filters, which is why it is an entity rather than another
//! `render_*` helper on the workspace: that state belongs to this panel and
//! nothing else reads it.

use crate::boards::{self, BoardDef, Runtime};
use crate::toolchain;
use crate::views::flash_method_label;
use crate::workspace_state::Workspace;

use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window,
};

/// The runtime the list is filtered to, or every runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeFilter {
    Any,
    Only(Runtime),
}

impl RuntimeFilter {
    const ALL: &'static [RuntimeFilter] = &[
        RuntimeFilter::Any,
        RuntimeFilter::Only(Runtime::MicroPython),
        RuntimeFilter::Only(Runtime::CircuitPython),
        RuntimeFilter::Only(Runtime::Arduino),
        RuntimeFilter::Only(Runtime::Native),
    ];

    fn label(self) -> &'static str {
        match self {
            RuntimeFilter::Any => "All",
            RuntimeFilter::Only(runtime) => runtime.label(),
        }
    }

    fn accepts(self, board: &BoardDef) -> bool {
        match self {
            RuntimeFilter::Any => true,
            RuntimeFilter::Only(runtime) => board.supports(runtime),
        }
    }
}

pub(super) struct CatalogPanel {
    owner: WeakEntity<Workspace>,
    focus: FocusHandle,
    search: Entity<InputState>,
    query: String,
    runtime: RuntimeFilter,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl CatalogPanel {
    pub(super) fn new(
        owner: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search by board, maker or chip…"));
        let subscription = cx.subscribe_in(&search, window, |this, state, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = state.read(cx).value().to_string();
                this.scroll.scroll_to_item(0);
                cx.notify();
            }
        });
        Self {
            owner,
            focus: cx.focus_handle(),
            search,
            query: String::new(),
            runtime: RuntimeFilter::Any,
            scroll: ScrollHandle::new(),
            _subscriptions: vec![subscription],
        }
    }

    /// The boards the current query and filter admit, grouped by maker.
    fn matches(&self) -> Vec<(SharedString, Vec<&'static BoardDef>)> {
        let admitted = |board: &&'static BoardDef| {
            self.runtime.accepts(board) && board.matches_query(&self.query)
        };
        let mut groups: Vec<(SharedString, Vec<&'static BoardDef>)> = Vec::new();

        // Support is really by chip, so the families lead. A board that is not
        // in the list by name is still programmable through its family, and
        // that has to be visible before the reader concludes otherwise.
        let families: Vec<&'static BoardDef> = boards::BOARD_CATALOG
            .iter()
            .filter(|board| board.generic)
            .filter(admitted)
            .collect();
        if !families.is_empty() {
            groups.push(("Chip families — any board using this chip".into(), families));
        }

        for vendor in boards::vendors() {
            let items: Vec<&'static BoardDef> = boards::BOARD_CATALOG
                .iter()
                .filter(|board| board.vendor == vendor && !board.generic)
                .filter(admitted)
                .collect();
            if !items.is_empty() {
                groups.push((vendor.into(), items));
            }
        }
        groups
    }

    /// Pin this board to the connected port, which is the only action the
    /// catalog can take on the user's behalf.
    fn use_board(&mut self, id: &'static str, cx: &mut Context<Self>) {
        _ = self.owner.update(cx, |workspace, cx| {
            workspace.set_board_override(id);
            cx.notify();
        });
        cx.notify();
    }

    fn render_board(
        &self,
        board: &'static BoardDef,
        selected: bool,
        connected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tools = toolchain::required_tools(board, board.default_runtime());
        let tool_names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();

        v_flex()
            .id(SharedString::from(board.id))
            .p_3()
            .gap_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(if selected {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .bg(cx.theme().background)
            .hover(|style| style.border_color(cx.theme().ring))
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(board.name),
                    )
                    .child(Tag::secondary().small().child(board.chip.label())),
            )
            .child(
                h_flex().gap_1().flex_wrap().children(
                    board
                        .runtimes
                        .iter()
                        .map(|runtime| Tag::secondary().small().outline().child(runtime.label())),
                ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Firmware: {}", flash_method_label(board.flash))),
            )
            .when(!tool_names.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Needs: {}", tool_names.join(", "))),
                )
            })
            .child(
                h_flex().pt_1().child(
                    Button::new(SharedString::from(format!("use-{}", board.id)))
                        .label(if selected {
                            "Selected"
                        } else {
                            "Use this board"
                        })
                        .ghost()
                        .small()
                        .selected(selected)
                        .tooltip(if connected {
                            "Pin this board to the connected port"
                        } else {
                            "Connect a board over USB to pin a model to it"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.use_board(board.id, cx))),
                ),
            )
    }
}

impl EventEmitter<PanelEvent> for CatalogPanel {}

impl Focusable for CatalogPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BasePanel for CatalogPanel {
    fn panel_name(&self) -> &'static str {
        super::Tool::Catalog.registry_name()
    }
}

impl Panel for CatalogPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Supported boards"
    }

    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(super::Tool::Catalog.name().into())
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Render for CatalogPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.matches();
        let shown: usize = groups.iter().map(|(_, boards)| boards.len()).sum();
        let total = boards::BOARD_CATALOG.len();
        let (selected, connected) = self
            .owner
            .upgrade()
            .map(|owner| {
                let workspace = owner.read(cx);
                (
                    workspace.active_board().map(|board| board.id),
                    workspace.is_connected(),
                )
            })
            .unwrap_or((None, false));
        let active = self.runtime;

        v_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .bg(cx.theme().sidebar)
            .child(
                v_flex()
                    .p_3()
                    .gap_3()
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Input::new(&self.search).id("catalog-search").small())
                    .child(
                        ButtonGroup::new("catalog-runtime")
                            .outline()
                            .small()
                            .children(RuntimeFilter::ALL.iter().enumerate().map(
                                |(index, filter)| {
                                    Button::new(("catalog-runtime", index))
                                        .label(filter.label())
                                        .selected(active == *filter)
                                        .toggled(active == *filter)
                                },
                            ))
                            .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                                if let Some(filter) =
                                    clicked.first().and_then(|ix| RuntimeFilter::ALL.get(*ix))
                                {
                                    this.runtime = *filter;
                                    this.scroll.scroll_to_item(0);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if shown == total {
                                format!(
                                    "{total} boards across {chips} chips. A board that is not \
                                     listed by name works through its chip family.",
                                    chips = boards::Chip::all().len()
                                )
                            } else {
                                format!("{shown} of {total} boards")
                            }),
                    ),
            )
            .child(
                div()
                    .id("catalog-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .p_3()
                    .gap_4()
                    .when(groups.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .items_center()
                                .gap_2()
                                .py_8()
                                .child(
                                    Icon::new(IconName::Search)
                                        .size_5()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(div().text_sm().child("No board matches that search"))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            "Search by chip instead — \"esp32-s3\", \"rp2040\", \
                                             \"samd21\" — and use that family's entry. Anode \
                                             programs the chip, so the board around it does not \
                                             have to be one it knows by name.",
                                        ),
                                ),
                        )
                    })
                    .children(groups.into_iter().map(|(group, items)| {
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child(group),
                            )
                            .children(items.into_iter().map(|board| {
                                self.render_board(board, selected == Some(board.id), connected, cx)
                            }))
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_filter_narrows_to_boards_that_run_it() {
        let micropython = RuntimeFilter::Only(Runtime::MicroPython);
        let uno = boards::board_by_id("arduino-uno").unwrap();
        let pico = boards::board_by_id("rpi-pico").unwrap();
        assert!(!micropython.accepts(uno), "the Uno has too little RAM");
        assert!(micropython.accepts(pico));
        assert!(RuntimeFilter::Any.accepts(uno));
    }

    #[test]
    fn every_runtime_has_boards_to_show() {
        for filter in RuntimeFilter::ALL {
            let count = boards::BOARD_CATALOG
                .iter()
                .filter(|board| filter.accepts(board))
                .count();
            assert!(count > 0, "{} matches no board", filter.label());
        }
    }
}
