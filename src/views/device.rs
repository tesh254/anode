//! The Board panel: what is on the port, which runtime it will execute, the
//! tools that takes, and the firmware actions.
//!
//! This is the surface that makes the catalog visible. Everything it shows is
//! derived from the resolved board, so a family guess, a pinned override and
//! an exact identification each read differently without the user having to
//! know how detection works.

use super::{detail_row, picker_or_placeholder, section_header, status_dot};
use crate::boards::{self, BoardDef, Chip, FlashMethod, Runtime};
use crate::device::FirmwareSource;
use crate::panels::Tool;
use crate::toolchain::Tool as DeviceTool;
use crate::workspace_state::Workspace;

use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::select::{SelectGroup, SelectItem};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, rems, AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled, Window,
};

// ---------------------------------------------------------------------------
// The board Select
// ---------------------------------------------------------------------------

/// A Select item naming one catalog board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoardItem {
    id: &'static str,
    /// `SelectItem::value` returns a reference, so the owned value lives here.
    value: String,
}

impl BoardItem {
    fn new(board: &BoardDef) -> Self {
        Self {
            id: board.id,
            value: board.id.to_string(),
        }
    }

    fn board(&self) -> Option<&'static BoardDef> {
        boards::board_by_id(self.id)
    }
}

impl SelectItem for BoardItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.board()
            .map(|board| SharedString::from(board.name))
            .unwrap_or_else(|| self.id.into())
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(board) = self.board() else {
            return div().child(self.id).into_any_element();
        };
        h_flex()
            .w_full()
            .gap_2()
            .justify_between()
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().truncate().child(board.name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(board.vendor),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(board.chip.label()),
            )
            .into_any_element()
    }

    /// Search across the name, the vendor and the chip, so "s3", "seeed" and
    /// "xiao" all reach the same board.
    fn matches(&self, query: &str) -> bool {
        self.board().is_some_and(|board| board.matches_query(query))
    }
}

/// The board picker's groups: the detected chip's boards first, then every
/// other board under its vendor.
///
/// No board appears twice — the leading group removes that chip from the
/// vendor groups — so a checkmark always points at one row.
pub(crate) fn board_groups(detected: Option<Chip>) -> Vec<SelectGroup<BoardItem>> {
    let mut groups = Vec::new();
    if let Some(chip) = detected {
        let items: Vec<BoardItem> = boards::boards_for_chip(chip)
            .into_iter()
            .map(BoardItem::new)
            .collect();
        if !items.is_empty() {
            groups.push(SelectGroup::new(format!("{} boards", chip.label())).items(items));
        }
    }
    for vendor in boards::vendors() {
        let items: Vec<BoardItem> = boards::BOARD_CATALOG
            .iter()
            .filter(|board| board.vendor == vendor && Some(board.chip) != detected)
            .map(BoardItem::new)
            .collect();
        if !items.is_empty() {
            groups.push(SelectGroup::new(vendor).items(items));
        }
    }
    groups
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

impl Workspace {
    pub(crate) fn render_device_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.sync_fqbn_input(window, cx);

        // Runtime, toolchain and firmware all describe a board. With none
        // chosen, each of them could only repeat "select a board" — three
        // sections saying the same thing, where one line says it once.
        let connected = self.is_connected();
        let active_chip = self.active_board().map(|board| board.chip.label());
        let mut sections = vec![
            self.render_connection_section(cx).into_any_element(),
            self.render_board_section(cx).into_any_element(),
        ];
        if self.active_board().is_some() {
            sections.push(self.render_runtime_section(cx).into_any_element());
            sections.push(self.render_toolchain_section(cx).into_any_element());
            sections.push(self.render_firmware_section(cx).into_any_element());
        } else {
            sections.push(
                GroupBox::new()
                    .outline()
                    .outline()
                    .title(section_header(IconName::Sparkles, "Setup", cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Runtime, toolchain and firmware appear once a board is chosen.",
                            ),
                    )
                    .into_any_element(),
            );
        }

        v_flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().sidebar)
            .child(
                h_flex()
                    .h_9()
                    .px_4()
                    .gap_2()
                    .flex_shrink_0()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Board"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .when_some(active_chip, |this, chip| {
                                this.child(Tag::secondary().small().child(chip))
                            })
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .text_xs()
                                    .text_color(if connected {
                                        cx.theme().success
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(status_dot(connected, cx))
                                    .child(if connected { "Connected" } else { "Offline" }),
                            ),
                    ),
            )
            .child(
                div()
                    .id("device-inspector-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(v_flex().p_4().gap_3().children(sections)),
            )
    }

    fn render_connection_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.is_connected();
        let port_picker = picker_or_placeholder(
            self.pickers.port.as_ref(),
            "Connect a board over USB",
            "Serial port",
            !self.ports.is_empty(),
            cx,
        );

        GroupBox::new()
            .outline()
            .title(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(section_header(IconName::Usb, "Connection", cx))
                    .child(
                        h_flex()
                            .gap_1p5()
                            .text_xs()
                            .text_color(if connected {
                                cx.theme().success
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(status_dot(connected, cx))
                            .child(if connected { "Connected" } else { "Offline" }),
                    ),
            )
            .child(
                v_flex()
                    .gap_3()
                    .child(port_picker)
                    .when(self.ports.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "No serial devices found. Connect a board, \
                                     or check that its USB cable carries data.",
                                ),
                        )
                    })
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new("device-scan")
                                    .label("Scan")
                                    .icon(IconName::RefreshCw)
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| this.refresh_ports(cx))),
                            )
                            .child(
                                Button::new("device-repl")
                                    .label("Open REPL")
                                    .icon(IconName::SquareTerminal)
                                    .ghost()
                                    .small()
                                    .disabled(!connected)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.repl_connect();
                                        this.show_tool(Tool::Repl, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("device-connection-help")
                                    .label("Connection help…")
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_connection_help(window, cx);
                                    })),
                            ),
                    ),
            )
    }

    fn render_board_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let resolved = self.resolved_board();
        let board = self.active_board();
        let connected = self.is_connected();
        let is_arduino = self.active_runtime() == Runtime::Arduino;
        let probing = self.chip_probe_running();
        let port = self.selected_port_identity();
        // Asking the chip is only offered where it could still answer
        // something new; a pinned board, or one a product id named outright,
        // has nothing left to learn.
        let can_probe = port
            .as_ref()
            .is_some_and(|port| self.chip_probe_is_useful(port))
            && !self.device_busy;
        let chip_detail = port
            .as_ref()
            .and_then(|port| self.chip_report_for(port))
            .map(crate::esp::ChipReport::summary);
        let probe_failure = match self.chip_probe() {
            Some(crate::workspace_state::ChipProbe::Silent) => Some(
                "No Espressif chip answered on this port, so the model below is a guess from the \
                 USB descriptor."
                    .to_string(),
            ),
            Some(crate::workspace_state::ChipProbe::Failed(detail)) => Some(detail.clone()),
            _ => None,
        };
        let picker = picker_or_placeholder(
            self.pickers.board.as_ref(),
            "Search boards or chips…",
            "Board model",
            true,
            cx,
        );

        GroupBox::new().outline()
            .title(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(section_header(IconName::Microchip, "Board", cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(if resolved.exact || resolved.from_override {
                                cx.theme().success
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(resolved.confidence()),
                    ),
            )
            .child(
                v_flex()
                    .gap_3()
                    .child(picker)
                    .when(probing, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().info)
                                .child("Asking the chip what it is…"),
                        )
                    })
                    .when_some(chip_detail.clone(), |this, detail| {
                        this.child(detail_row("Reported", detail, cx))
                    })
                    .when_some(probe_failure.clone(), |this, failure| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(failure),
                        )
                    })
                    .when(
                        !probing
                            && !resolved.exact
                            && !resolved.from_override
                            && !resolved.probed
                            && connected,
                        |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "A USB-serial adapter cannot name the chip behind it. Ask \
                                         the board, or pick your exact model and Anode remembers \
                                         it.",
                                    ),
                            )
                        },
                    )
                    .when(can_probe, |this| {
                        this.child(
                            Button::new("board-identify")
                                .label("Identify chip")
                                .icon(IconName::Microchip)
                                .outline()
                                .small()
                                .loading(probing)
                                .disabled(probing)
                                .tooltip("Reset the board into its bootloader and read its chip")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.probe_chip(true, cx);
                                })),
                        )
                    })
                    .when_some(board, |this, board| {
                        this.child(
                            h_flex()
                                .gap_1()
                                .flex_wrap()
                                .child(Tag::secondary().small().child(board.chip.label()))
                                .child(
                                    Tag::secondary()
                                        .small()
                                        .outline()
                                        .child(board.chip.architecture().label()),
                                )
                                .children(board.runtimes.iter().map(|runtime| {
                                    Tag::secondary().small().outline().child(runtime.label())
                                })),
                        )
                        .child(
                            h_flex()
                                .gap_4()
                                .flex_wrap()
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w(rems(7.5))
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Maker"),
                                        )
                                        .child(div().text_xs().truncate().child(board.vendor)),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w(rems(7.5))
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Silicon"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .truncate()
                                                .child(board.chip.silicon_vendor()),
                                        ),
                                ),
                        )
                        .when_some(board.note, |this, note| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(note),
                            )
                        })
                        .when(is_arduino, |this| {
                            this.child(
                                v_flex()
                                    .gap_1()
                                    .pt_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Arduino board name"),
                                    )
                                    .child(Input::new(&self.fqbn_input).id("fqbn-input").small())
                                    .child(
                                        h_flex()
                                            .flex_wrap()
                                            .gap_2()
                                            .child(
                                                Button::new("board-save-fqbn")
                                                    .label("Use this name")
                                                    .small()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.commit_fqbn_input(cx);
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                Button::new("board-install-core")
                                                    .label("Install core")
                                                    .icon(IconName::Download)
                                                    .ghost()
                                                    .small()
                                                    .disabled(self.device_busy)
                                                    .tooltip(
                                                        "Download the arduino-cli core this board compiles with",
                                                    )
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.install_board_core();
                                                        cx.notify();
                                                    })),
                                            ),
                                    ),
                            )
                        })
                    })
                    .when(board.is_none(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if connected {
                                    "Anode does not recognise this descriptor. \
                                     Choose your board above to set it up."
                                } else {
                                    "Connect a board over USB, or choose a model now to set \
                                     it up in advance."
                                }),
                        )
                    }),
            )
    }

    fn render_runtime_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let runtime = self.active_runtime();
        let layout = self.workspace_layout();
        let picker = picker_or_placeholder(
            self.pickers.profile.as_ref(),
            "Detected when a board connects",
            "Runtime",
            !self.available_profiles.is_empty(),
            cx,
        );
        // Say what choosing this runtime changes, in the words of the buttons
        // it turns on.
        let explanation = match runtime {
            Runtime::MicroPython => {
                "Run sends the open file over serial; Upload copies the project onto the board."
            }
            Runtime::CircuitPython => {
                "Upload copies the project onto the CIRCUITPY drive, and the board re-runs code.py."
            }
            Runtime::Arduino => "Build compiles the sketch; Upload compiles, then writes it.",
            Runtime::Native => {
                "Build and Upload run PlatformIO, or ESP-IDF when the project is one."
            }
        };

        GroupBox::new()
            .outline()
            .title(section_header(IconName::Workflow, "Runtime", cx))
            .child(
                v_flex()
                    .gap_2()
                    .child(picker)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(explanation),
                    )
                    .when(
                        (layout.show_device_files && self.is_connected())
                            || self.project_kind != crate::project::ProjectKind::Unknown,
                        |this| {
                            this.child(
                                h_flex()
                                    .gap_4()
                                    .flex_wrap()
                                    .when(layout.show_device_files && self.is_connected(), |row| {
                                        row.child(
                                            v_flex()
                                                .flex_1()
                                                .min_w(rems(7.5))
                                                .gap_0p5()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child("Board filesystem"),
                                                )
                                                .child(div().text_xs().child("Available")),
                                        )
                                    })
                                    .when(
                                        self.project_kind != crate::project::ProjectKind::Unknown,
                                        |row| {
                                            row.child(
                                                v_flex()
                                                    .flex_1()
                                                    .min_w(rems(7.5))
                                                    .gap_0p5()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child("Open project"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .child(self.project_kind.label()),
                                                    ),
                                            )
                                        },
                                    ),
                            )
                        },
                    ),
            )
    }

    fn render_toolchain_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let required = self.required_tools();
        let scanning = self.tools_scanning;
        let busy = self.device_busy;

        GroupBox::new()
            .outline()
            .title(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(section_header(IconName::Wrench, "Toolchain", cx))
                    .child(
                        Button::new("toolchain-rescan")
                            .icon(IconName::RefreshCw)
                            .ghost()
                            .small()
                            .loading(scanning)
                            .accessibility_label("Re-check tools")
                            .tooltip("Re-check tools")
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_tools(cx))),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .when(required.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{} needs no extra tools — Anode writes to the board's own \
                                     drive.",
                                    self.active_runtime().label()
                                )),
                        )
                    })
                    .children(
                        required.into_iter().map(|(tool, available)| {
                            self.render_tool_row(tool, available, busy, cx)
                        }),
                    ),
            )
    }

    fn render_tool_row(
        &self,
        tool: DeviceTool,
        available: bool,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let detail = self
            .tool_status(tool)
            .map(|status| status.detail())
            .unwrap_or_else(|| tool.purpose().to_string());

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap_1p5()
                            .child(
                                Icon::new(if available {
                                    IconName::CircleCheck
                                } else {
                                    IconName::CircleAlert
                                })
                                .size_3()
                                .text_color(if available {
                                    cx.theme().success
                                } else {
                                    cx.theme().warning
                                }),
                            )
                            .child(div().text_xs().truncate().child(tool.name())),
                    )
                    .when(!available && tool.is_installable(), |this| {
                        this.child(
                            Button::new(SharedString::from(format!("install-{}", tool.id())))
                                .label("Install")
                                .small()
                                .compact()
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.install_tool(tool);
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(detail),
            )
            .into_any_element()
    }

    fn render_firmware_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(board) = self.active_board() else {
            return GroupBox::new()
                .outline()
                .title(section_header(IconName::Zap, "Firmware", cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No board selected."),
                );
        };
        let connected = self.is_connected();
        let busy = self.device_busy;
        // Only esptool and the serial bootloaders need the port; a
        // mass-storage bootloader mounts as a disk and needs no connection.
        let needs_port = matches!(
            board.flash,
            FlashMethod::Esptool | FlashMethod::Bossac | FlashMethod::DfuUtil
        );
        let ready = !busy && (connected || !needs_port);
        let supported = board.flash != FlashMethod::Unsupported;
        // The one action worth emphasising names the exact build it will
        // write, because that build was chosen from what the chip reported
        // rather than from anything the user picked.
        let micropython_build = self
            .flash_request(FirmwareSource::MicroPython)
            .map(|request| request.image_label());
        let plan_detail = self
            .flash_request(FirmwareSource::MicroPython)
            .and_then(|request| request.plan())
            .map(|plan| plan.detail());
        let running = self.device_busy.then(|| self.flash_phase());

        GroupBox::new().outline()
            .title(section_header(IconName::Zap, "Firmware", cx))
            .child(
                v_flex()
                    .gap_2()
                    .when(!supported, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{} runs compiled programs only — there is no separate \
                                     firmware to replace.",
                                    board.name
                                )),
                        )
                    })
                    .when(supported, |this| {
                        this.child(detail_row("Method", flash_method_label(board.flash), cx))
                            .when_some(plan_detail.clone(), |this, detail| {
                                // Settings Anode worked out rather than asked
                                // for still have to be visible before the
                                // write, not only in the confirmation.
                                this.children(detail.into_iter().map(|line| {
                                    let (label, value) = line
                                        .split_once(": ")
                                        .unwrap_or(("Setting", line.as_str()));
                                    detail_row(label.to_string(), value.to_string(), cx)
                                }))
                            })
                            .when_some(board.flash.preparation(), |this, preparation| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(preparation),
                                )
                            })
                            .when_some(running, |this, phase| {
                                this.child(super::flash_progress(
                                    "Working on the board",
                                    phase.as_ref(),
                                    cx,
                                ))
                            })
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("firmware-micropython")
                                            .label("Flash MicroPython…")
                                            .icon(IconName::Download)
                                            .primary()
                                            .small()
                                            .w_full()
                                            .justify_start()
                                            .disabled(!ready || board.mpy_variant.is_none())
                                            .tooltip(match &micropython_build {
                                                Some(build) => {
                                                    format!("Writes the latest {build} build")
                                                }
                                                None => {
                                                    "No MicroPython build is published for this board"
                                                        .to_string()
                                                }
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.confirm_firmware(
                                                    FirmwareSource::MicroPython,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    )
                                    .child(
                                        Button::new("firmware-circuitpython")
                                            .label("Flash CircuitPython…")
                                            .icon(IconName::Download)
                                            .outline()
                                            .small()
                                            .w_full()
                                            .justify_start()
                                            .disabled(!ready || board.cpy_board.is_none())
                                            .tooltip(match board.cpy_board {
                                                Some(slug) => format!("Latest build of {slug}"),
                                                None => {
                                                    "Anode does not know this board's CircuitPython id"
                                                        .to_string()
                                                }
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.confirm_firmware(
                                                    FirmwareSource::CircuitPython,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    )
                                    .child(
                                        Button::new("firmware-file")
                                            .label("Flash an image from disk…")
                                            .icon(IconName::HardDrive)
                                            .ghost()
                                            .small()
                                            .w_full()
                                            .justify_start()
                                            .disabled(!ready)
                                            .tooltip(format!(
                                                "Write a {} file you already have",
                                                board.flash.firmware_extension()
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.pick_firmware_file(cx);
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .when(needs_port && !connected, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().warning)
                                        .child("Connect the board to flash it over serial."),
                                )
                            })
                    }),
            )
    }

    /// Replacing firmware wipes whatever is on the board, so name the board,
    /// the image and the consequence before doing it.
    fn confirm_firmware(
        &mut self,
        source: FirmwareSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(board) = self.active_board() else {
            return;
        };
        let Some(request) = self.flash_request(source.clone()) else {
            return;
        };
        let workspace = cx.weak_entity();
        let title = format!("Flash {} to {}?", request.image_label(), board.name);
        let detail = flash_detail(&request);
        let preparation = board.flash.preparation();

        window.open_dialog(cx, move |dialog, _, _| {
            let workspace = workspace.clone();
            let source = source.clone();
            dialog
                .title(title.clone())
                .child(
                    v_flex()
                        .gap_2()
                        .children(detail.clone().into_iter().map(|line| div().child(line)))
                        .when_some(preparation, |this, preparation| this.child(preparation)),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("firmware-cancel")
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("firmware-confirm")
                                .label("Flash")
                                .danger()
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    _ = workspace.update(cx, |workspace, cx| {
                                        workspace.flash_firmware(source.clone());
                                        cx.notify();
                                    });
                                }),
                        ),
                )
        });
    }

    // -- Recent boards --------------------------------------------------------

    pub(crate) fn render_recent_boards(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let records = self.settings.recent_boards.clone();
        v_flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().sidebar)
            .child(
                h_flex()
                    .h_8()
                    .px_3()
                    .gap_2()
                    .flex_shrink_0()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Boards"),
                    )
                    .child(
                        Button::new("boards-scan")
                            .icon(IconName::RefreshCw)
                            .ghost()
                            .small()
                            .accessibility_label("Scan USB")
                            .tooltip("Scan USB")
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_ports(cx))),
                    ),
            )
            .child(
                div()
                    .id("recent-boards-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .p_2()
                            .gap_1()
                            .when(records.is_empty(), |this| {
                                this.child(
                                    v_flex()
                                        .p_2()
                                        .gap_1()
                                        .child(div().text_xs().child("No boards yet"))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(
                                                    "Connect a board over USB. It appears here \
                                                     and stays in your history.",
                                                ),
                                        ),
                                )
                            })
                            .children(records.into_iter().map(|record| {
                                let port = self
                                    .ports
                                    .iter()
                                    .find(|port| crate::hardware::device_key(port) == record.key);
                                let connected = port.is_some();
                                let path = port
                                    .map(|port| port.path.as_str())
                                    .unwrap_or(&record.path)
                                    .to_string();
                                let chip = record
                                    .board_id
                                    .as_deref()
                                    .and_then(boards::board_by_id)
                                    .map(|board| board.chip.label());
                                v_flex()
                                    .id(SharedString::from(record.key.clone()))
                                    .p_2()
                                    .gap_1()
                                    .rounded(cx.theme().radius)
                                    .hover(|style| style.bg(cx.theme().accent))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(status_dot(connected, cx))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .truncate()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(record.name.clone()),
                                            )
                                            .when_some(chip, |this, chip| {
                                                this.child(Tag::secondary().small().child(chip))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .truncate()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .child(path),
                                    )
                                    .child(
                                        h_flex().gap_1().child(
                                            Button::new(SharedString::from(format!(
                                                "open-board:{}",
                                                record.key
                                            )))
                                            .label("Open panel")
                                            .ghost()
                                            .small()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_board_panel(record.clone(), window, cx)
                                            })),
                                        ),
                                    )
                            })),
                    ),
            )
    }
}

/// The flash method in the user's words.
pub(crate) fn flash_method_label(method: FlashMethod) -> String {
    match method {
        FlashMethod::Esptool => "esptool over serial".into(),
        FlashMethod::MassStorage { volume, .. } => match volume {
            Some(volume) => format!("Copy onto the {volume} volume"),
            None => "Copy onto the bootloader volume".into(),
        },
        FlashMethod::DfuUtil => "dfu-util over USB".into(),
        FlashMethod::Bossac => "bossac over serial".into(),
        FlashMethod::TeensyLoader => "Teensy loader over USB".into(),
        FlashMethod::Unsupported => "Not supported".into(),
    }
}

/// The lines that state what a flash is about to do, and what it costs.
///
/// Everything here is a setting Anode worked out rather than asked for, so
/// the confirmation is the one place the user sees them — the chip that will
/// be written, where the image lands, how fast, and that the board's own files
/// go with it.
pub(crate) fn flash_detail(request: &crate::device::FlashRequest) -> Vec<String> {
    let board = request.board;
    let mut detail = vec![format!("Board: {} · {}", board.name, board.chip.label())];
    if !request.port_path.is_empty() {
        detail.push(format!("Port: {}", request.port_path));
    }
    detail.push(format!("Image: {}", request.image_label()));
    match request.plan() {
        Some(plan) => detail.extend(plan.detail()),
        None => detail.push(format!("Method: {}", flash_method_label(board.flash))),
    }
    detail.push(
        "Everything currently on the board, including any files you saved to it, is erased.".into(),
    );
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_detected_chip_leads_and_nothing_is_listed_twice() {
        let groups = board_groups(Some(Chip::Rp2040));
        assert!(groups[0].title.contains("RP2040"));

        let mut ids: Vec<&str> = groups
            .iter()
            .flat_map(|group| group.items.iter().map(|item| item.id))
            .collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "a board appears in two groups");
        assert_eq!(
            total,
            boards::BOARD_CATALOG.len(),
            "every catalog board is reachable from the picker"
        );
    }

    #[test]
    fn without_a_detected_chip_the_groups_are_vendors() {
        let groups = board_groups(None);
        assert_eq!(groups.len(), boards::vendors().len());
        let total: usize = groups.iter().map(|group| group.items.len()).sum();
        assert_eq!(total, boards::BOARD_CATALOG.len());
    }

    #[test]
    fn board_items_search_by_vendor_and_chip() {
        let item = BoardItem::new(boards::board_by_id("xiao-esp32s3").unwrap());
        assert!(item.matches("seeed"));
        assert!(item.matches("ESP32-S3"));
        assert!(!item.matches("teensy"));
    }

    #[test]
    fn flash_methods_are_described_without_jargon_only_we_know() {
        assert_eq!(
            flash_method_label(FlashMethod::MassStorage {
                volume: Some("RPI-RP2"),
                marker: boards::UF2_MARKER
            }),
            "Copy onto the RPI-RP2 volume"
        );
        assert_eq!(
            flash_method_label(FlashMethod::Esptool),
            "esptool over serial"
        );
    }

    fn request(board_id: &str, port: &str) -> crate::device::FlashRequest {
        crate::device::FlashRequest {
            port_path: port.into(),
            board: boards::board_by_id(board_id).unwrap(),
            source: FirmwareSource::MicroPython,
            report: None,
            native_usb: false,
        }
    }

    #[test]
    fn the_confirmation_names_the_board_the_port_and_the_method() {
        let detail = flash_detail(&request("rpi-pico", "/dev/cu.usbmodem1101"));
        assert!(detail[0].contains("Raspberry Pi Pico"));
        assert!(detail
            .iter()
            .any(|line| line.contains("/dev/cu.usbmodem1101")));
        assert!(detail.iter().any(|line| line.contains("RPI-RP2")));
        assert!(detail.iter().any(|line| line.contains("is erased")));

        // With no port, the line is left out rather than shown empty.
        let detail = flash_detail(&request("rpi-pico", ""));
        assert!(!detail.iter().any(|line| line.starts_with("Port:")));
    }

    /// The settings the user never has to supply are the ones the
    /// confirmation has to state, since this is where they become visible.
    #[test]
    fn an_esptool_confirmation_states_the_settings_anode_derived() {
        let mut request = request("esp32-s3-generic", "/dev/cu.usbmodem1101");
        request.report = Some(crate::esp::ChipReport {
            chip: Some(Chip::Esp32S3),
            psram: Some(crate::esp::Psram {
                megabytes: 8,
                octal: true,
            }),
            ..Default::default()
        });
        let detail = flash_detail(&request);
        let text = detail.join("\n");
        assert!(
            text.contains("ESP32_GENERIC_S3-SPIRAM_OCT"),
            "the PSRAM build is named: {text}"
        );
        assert!(text.contains("Chip: ESP32-S3"));
        assert!(text.contains("0x0"), "the write offset is stated: {text}");
        assert!(text.contains("baud"), "the speed is stated: {text}");
        assert!(text.contains("erase"), "the erase is stated: {text}");
    }
}
