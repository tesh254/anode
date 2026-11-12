//! One physical board's own panel: its identity, its firmware, and its own
//! job with its own output.
//!
//! The point of a per-board panel is concurrency — two boards can be flashed
//! or uploaded to at the same time, each with its own progress — so the job
//! lives here rather than on the workspace, and a port lease keeps the
//! workspace commands, the REPL and the other panels off this board while it
//! runs.

use super::Tool;
use crate::boards::{self, BoardDef, FlashMethod, PortIdentity, Runtime};
use crate::device::{self, DeviceEvent, FirmwareSource};
use crate::hardware::{device_key, PortLeases, RecentBoard};
use crate::views::{board_groups, flash_method_label, BoardItem};
use crate::workspace_state::Workspace;
use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent, PanelInfo, PanelState};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectGroup, SelectState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    WeakEntity, Window,
};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

/// What this panel is currently doing to its board.
#[derive(Clone)]
enum Job {
    Firmware(FirmwareSource),
    Upload,
}

impl Job {
    fn description(&self) -> String {
        match self {
            Job::Firmware(source) => format!("Flashing {}", source.label()),
            Job::Upload => "Uploading the project".into(),
        }
    }
}

enum JobEvent {
    Progress(String),
    Finished(Result<(), String>),
}

pub(super) struct FlashPanel {
    record: RecentBoard,
    owner: WeakEntity<Workspace>,
    leases: PortLeases,
    focus: FocusHandle,
    port: Option<PortIdentity>,
    board_id: Option<String>,
    picker: Entity<SelectState<SearchableVec<SelectGroup<BoardItem>>>>,
    running: Option<String>,
    log: Vec<String>,
    result: Option<Result<(), String>>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
    _job_task: Option<Task<()>>,
}

impl FlashPanel {
    pub(super) fn new(
        record: RecentBoard,
        owner: WeakEntity<Workspace>,
        leases: PortLeases,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected = record.board_id.clone();
        let chip = selected
            .as_deref()
            .and_then(boards::board_by_id)
            .map(|board| board.chip);
        let picker =
            cx.new(|cx| SelectState::new(SearchableVec::new(board_groups(chip)), None, window, cx));
        let subscription = cx.subscribe_in(
            &picker,
            window,
            |this, _, event: &SelectEvent<SearchableVec<SelectGroup<BoardItem>>>, _, cx| {
                if let SelectEvent::Confirm(Some(id)) = event {
                    this.pin_board(id.clone(), cx);
                }
            },
        );
        let mut subscriptions = vec![subscription];
        if let Some(owner) = owner.upgrade() {
            subscriptions.push(cx.observe_in(&owner, window, |this, _, window, cx| {
                this.synchronize(window, cx)
            }));
        }
        let panel = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            _ = panel.update(cx, |this, cx| this.synchronize(window, cx));
        });
        Self {
            record,
            owner,
            leases,
            focus: cx.focus_handle(),
            port: None,
            board_id: selected,
            picker,
            running: None,
            log: Vec::new(),
            result: None,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
            _job_task: None,
        }
    }

    fn is_busy(&self) -> bool {
        self.running.is_some()
    }

    /// Remember this model for this physical board, in the panel, the saved
    /// history and the workspace's overrides.
    fn pin_board(&mut self, id: String, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.board_id = Some(id.clone());
        self.record.board_id = Some(id.clone());
        if let Some(board) = boards::board_by_id(&id) {
            self.record.name = board.name.into();
        }
        let identity = self.port.clone().unwrap_or_else(|| PortIdentity {
            path: self.record.path.clone(),
            vendor_id: self.record.vendor_id.clone(),
            product_id: self.record.product_id.clone(),
            serial_number: self.record.serial_number.clone(),
            manufacturer: None,
        });
        let record = self.record.clone();
        _ = self.owner.update(cx, |workspace, cx| {
            workspace
                .settings
                .board_overrides
                .insert(boards::port_key(&identity), id.clone());
            if let Some(saved) = workspace
                .settings
                .recent_boards
                .iter_mut()
                .find(|saved| saved.key == record.key)
            {
                saved.board_id = record.board_id.clone();
                saved.name = record.name.clone();
            }
            workspace.settings.save();
            workspace.save_panel_layout(cx);
            cx.notify();
        });
        cx.notify();
    }

    fn synchronize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let workspace = owner.read(cx);
        let port = workspace
            .ports
            .iter()
            .find(|port| device_key(port) == self.record.key)
            .cloned();
        let saved = workspace
            .settings
            .recent_boards
            .iter()
            .find(|record| record.key == self.record.key)
            .cloned();
        let detected = port
            .as_ref()
            .and_then(|port| {
                boards::identify_board(Some(port), &workspace.settings.board_overrides).board
            })
            .or_else(|| {
                saved
                    .as_ref()
                    .and_then(|record| record.board_id.as_deref())
                    .and_then(boards::board_by_id)
            });
        if !self.is_busy() {
            let previous = self.record.clone();
            if let Some(saved) = saved {
                self.record = saved;
            }
            if let Some(board) = detected {
                let id = board.id.to_string();
                self.record.board_id = Some(id.clone());
                if self.board_id.as_ref() != Some(&id) {
                    self.board_id = Some(id.clone());
                    self.picker
                        .update(cx, |picker, cx| picker.set_selected_value(&id, window, cx));
                }
            }
            if previous != self.record {
                owner.update(cx, |workspace, cx| workspace.save_panel_layout(cx));
            }
        }
        self.port = port;
        cx.notify();
    }

    fn board(&self) -> Option<&'static BoardDef> {
        self.board_id.as_deref().and_then(boards::board_by_id)
    }

    /// The runtime this panel's board runs, taken from the workspace when it
    /// is pointed at this same board and from the catalog otherwise.
    fn runtime(&self, cx: &App) -> Runtime {
        let board = self.board();
        self.owner
            .upgrade()
            .map(|owner| owner.read(cx))
            .filter(|workspace| {
                self.port
                    .as_ref()
                    .is_some_and(|port| workspace.selected_port == port.path)
            })
            .map(|workspace| workspace.active_runtime())
            .or_else(|| board.map(BoardDef::default_runtime))
            .unwrap_or(Runtime::MicroPython)
    }

    // -- Firmware -------------------------------------------------------------

    fn confirm_firmware(
        &mut self,
        source: FirmwareSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(board), Some(port)) = (self.board(), self.port.clone()) else {
            return;
        };
        if self.is_busy() {
            return;
        }
        // Capture what the user is confirming, so a board swap between the
        // dialog opening and the click cannot redirect the write.
        let target = (device_key(&port), port.path.clone(), board.id.to_string());
        let panel = cx.weak_entity();
        let title = format!("Flash {} to {}?", source.label(), board.name);
        let path = port.path.clone();
        let method = flash_method_label(board.flash);
        let preparation = board.flash.preparation();

        window.open_dialog(cx, move |dialog, _, _| {
            let panel = panel.clone();
            let target = target.clone();
            let source = source.clone();
            dialog
                .title(title.clone())
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!("Port: {path}"))
                        .child(format!("Method: {method}"))
                        .child(
                            "Everything currently on the board, including any files you saved \
                             to it, is erased.",
                        )
                        .when_some(preparation, |this, preparation| this.child(preparation)),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("cancel-flash")
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(Button::new("confirm-flash").label("Flash").danger().on_click(
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                _ = panel.update(cx, |this, cx| {
                                    let unchanged = this.port.as_ref().is_some_and(|port| {
                                        device_key(port) == target.0 && port.path == target.1
                                    }) && this.board_id.as_deref()
                                        == Some(target.2.as_str());
                                    if unchanged {
                                        this.start(Job::Firmware(source.clone()), cx);
                                    } else {
                                        this.result = Some(Err(
                                            "The board on this port changed. Check the model and try again."
                                                .into(),
                                        ));
                                        cx.notify();
                                    }
                                });
                            },
                        )),
                )
        });
    }

    // -- Jobs -----------------------------------------------------------------

    fn start(&mut self, job: Job, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let (Some(port), Some(board)) = (self.port.clone(), self.board()) else {
            self.result = Some(Err("Connect this board and choose its model first.".into()));
            cx.notify();
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let runtime = self.runtime(cx);
        let workspace = owner.read(cx);
        let files = workspace.project_files.clone();
        let project = workspace.project_path.clone();
        let fqbn = workspace.fqbn_for(board);

        if matches!(job, Job::Upload) && project.is_none() {
            self.result = Some(Err("Open a project folder before uploading.".into()));
            cx.notify();
            return;
        }

        let lease = match self.leases.acquire(&device_key(&port)) {
            Ok(lease) => lease,
            Err(error) => {
                self.result = Some(Err(error));
                cx.notify();
                return;
            }
        };
        // A bootloader volume has no serial identity, so only one such write
        // may run at a time even across different board panels.
        let volume_lease = match (&job, board.flash) {
            (Job::Firmware(_), FlashMethod::MassStorage { volume, .. }) => {
                let key = format!("anode:volume:{}", volume.unwrap_or("any"));
                match self.leases.acquire(&key) {
                    Ok(lease) => Some(lease),
                    Err(_) => {
                        self.result = Some(Err(
                            "Another board is using the bootloader volume right now.".into(),
                        ));
                        cx.notify();
                        return;
                    }
                }
            }
            _ => None,
        };

        let (sender, receiver) = std::sync::mpsc::channel();
        self.log.clear();
        self.log.push(format!("{} · {}", board.name, port.path));
        self.result = None;
        self.running = Some(job.description());

        let spawned = std::thread::Builder::new()
            .name(format!("anode-device-{}", self.record.key))
            .spawn(move || {
                let _lease = lease;
                let _volume_lease = volume_lease;
                let mut emit = |event: DeviceEvent| {
                    let DeviceEvent::Progress(message) = event;
                    let _ = sender.send(JobEvent::Progress(message));
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_job(
                        job, board, runtime, &port.path, project, files, fqbn, &mut emit,
                    )
                }))
                .unwrap_or_else(|_| {
                    Err(
                        "The device worker stopped unexpectedly. The port has been released."
                            .into(),
                    )
                });
                let _ = sender.send(JobEvent::Finished(result));
            });
        if let Err(error) = spawned {
            self.running = None;
            self.result = Some(Err(error.to_string()));
            cx.notify();
            return;
        }
        self._job_task = Some(cx.spawn(async move |this, cx| loop {
            let done = this
                .update(cx, |this, cx| this.drain_job(&receiver, cx))
                .unwrap_or(true);
            if done {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        }));
        cx.notify();
    }

    fn drain_job(&mut self, receiver: &Receiver<JobEvent>, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        loop {
            match receiver.try_recv() {
                Ok(JobEvent::Progress(message)) => {
                    self.log.push(message);
                    changed = true;
                }
                Ok(JobEvent::Finished(result)) => {
                    self.running = None;
                    self.result = Some(result);
                    changed = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.running = None;
                    self.result = Some(Err("The device worker stopped.".into()));
                    changed = true;
                    break;
                }
            }
        }
        if self.log.len() > 1200 {
            self.log.drain(..self.log.len() - 1200);
        }
        if changed {
            self.scroll.scroll_to_item(self.log.len().saturating_sub(1));
            cx.notify();
        }
        !self.is_busy()
    }
}

/// The work itself, on the worker thread. A free function so the closure that
/// crosses the thread boundary captures only plain data.
#[allow(clippy::too_many_arguments)]
fn run_job(
    job: Job,
    board: &'static BoardDef,
    runtime: Runtime,
    port_path: &str,
    project: Option<std::path::PathBuf>,
    files: Vec<crate::project::ProjectFile>,
    fqbn: Option<String>,
    emit: &mut dyn FnMut(DeviceEvent),
) -> Result<(), String> {
    match job {
        Job::Firmware(source) => {
            let outcome = device::flash_firmware(port_path, board, source, emit);
            if outcome.success {
                Ok(())
            } else {
                Err(outcome.message())
            }
        }
        Job::Upload if runtime == Runtime::Arduino => {
            let project = project.ok_or("Open a sketch folder before uploading.")?;
            let fqbn =
                fqbn.ok_or_else(|| format!("{} has no Arduino board name in Anode.", board.name))?;
            device::arduino_upload(port_path, &project, &fqbn, emit)
        }
        Job::Upload => {
            let python: Vec<_> = files
                .iter()
                .filter(|file| file.language.is_device_source())
                .collect();
            if python.is_empty() {
                return Err("This project has no Python files to upload.".into());
            }
            for file in &python {
                device::upload_code(port_path, &file.name, &file.content, emit)?;
            }
            emit(DeviceEvent::Progress(format!(
                "[OK] {} files are on the board",
                python.len()
            )));
            Ok(())
        }
    }
}

impl EventEmitter<PanelEvent> for FlashPanel {}

impl Focusable for FlashPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BasePanel for FlashPanel {
    fn panel_name(&self) -> &'static str {
        "anode.flash"
    }
    fn closable(&self, _: &App) -> bool {
        !self.is_busy()
    }
    fn dump(&self, _: &App) -> PanelState {
        let mut state = PanelState::new("anode.flash");
        let mut record = self.record.clone();
        record.board_id = self.board_id.clone();
        state.info = PanelInfo::panel(serde_json::to_value(record).unwrap_or_default());
        state
    }
}

impl Panel for FlashPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let name = self
            .board()
            .map(|board| board.name)
            .unwrap_or(&self.record.name)
            .to_string();
        match &self.running {
            Some(_) => format!("Working · {name}"),
            None => name,
        }
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(
            self.board()
                .map(|board| SharedString::from(board.name))
                .unwrap_or_else(|| self.record.name.clone().into()),
        )
    }
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Render for FlashPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.port.is_some();
        let busy_elsewhere = !self.is_busy() && self.leases.is_busy(&self.record.key);
        let board = self.board();
        let runtime = self.runtime(cx);
        let busy = self.is_busy();
        let path = self
            .port
            .as_ref()
            .map(|port| port.path.as_str())
            .unwrap_or(&self.record.path)
            .to_string();

        // A mass-storage bootloader mounts as a disk, so flashing it does not
        // need the serial port to be live.
        let needs_port = board.is_some_and(|board| {
            matches!(
                board.flash,
                FlashMethod::Esptool | FlashMethod::Bossac | FlashMethod::DfuUtil
            )
        });
        let can_flash = board.is_some_and(|board| board.flash != FlashMethod::Unsupported)
            && !busy
            && !busy_elsewhere
            && (connected || !needs_port);
        let can_upload = connected && board.is_some() && !busy && !busy_elsewhere;
        let upload_label = if runtime == Runtime::Arduino {
            "Upload sketch"
        } else {
            "Upload project"
        };
        let upload_supported = matches!(runtime, Runtime::Arduino | Runtime::MicroPython);

        v_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .p_4()
                    .gap_3()
                    .flex_shrink_0()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(crate::views::status_dot(connected, cx))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(
                                        board
                                            .map(|board| board.name)
                                            .unwrap_or(&self.record.name)
                                            .to_string(),
                                    ),
                            )
                            .when_some(board, |this, board| {
                                this.child(Tag::secondary().small().child(board.chip.label()))
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(path),
                    )
                    .child(
                        Select::new(&self.picker)
                            .small()
                            .disabled(busy)
                            .placeholder("Choose this board's model…")
                            .accessibility_label("Board model"),
                    )
                    .when(!connected, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Offline. Reconnect it over USB and this panel follows."),
                        )
                    })
                    .when(busy_elsewhere, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().warning)
                                .child("This board is in use by the REPL or another panel."),
                        )
                    })
                    .child(
                        v_flex()
                            .gap_1()
                            .pt_1()
                            .child(
                                Button::new("panel-flash-micropython")
                                    .label("Flash MicroPython…")
                                    .icon(IconName::Download)
                                    .primary()
                                    .small()
                                    .w_full()
                                    .justify_start()
                                    .disabled(
                                        !can_flash || board.is_none_or(|b| b.mpy_variant.is_none()),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_firmware(
                                            FirmwareSource::MicroPython,
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new("panel-flash-circuitpython")
                                    .label("Flash CircuitPython…")
                                    .icon(IconName::Download)
                                    .outline()
                                    .small()
                                    .w_full()
                                    .justify_start()
                                    .disabled(
                                        !can_flash || board.is_none_or(|b| b.cpy_board.is_none()),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_firmware(
                                            FirmwareSource::CircuitPython,
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new("panel-upload")
                                    .label(upload_label)
                                    .icon(IconName::Upload)
                                    .ghost()
                                    .small()
                                    .w_full()
                                    .justify_start()
                                    .disabled(!can_upload || !upload_supported)
                                    .tooltip(if upload_supported {
                                        SharedString::from(
                                            "Runs on this board only, alongside other panels",
                                        )
                                    } else {
                                        SharedString::from(format!(
                                            "{} uploads from the editor toolbar",
                                            runtime.label()
                                        ))
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.start(Job::Upload, cx)),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("panel-use-for-run")
                                    .label("Use for Run")
                                    .icon(IconName::Play)
                                    .ghost()
                                    .small()
                                    .disabled(!connected || busy || busy_elsewhere)
                                    .tooltip("Point the editor's commands at this board")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let Some(port) = &this.port else { return };
                                        let path = port.path.clone();
                                        _ = this.owner.update(cx, |workspace, cx| {
                                            workspace.select_port(path);
                                            cx.notify();
                                        });
                                    })),
                            )
                            .child(
                                Button::new("panel-connect-repl")
                                    .label("REPL")
                                    .icon(IconName::SquareTerminal)
                                    .ghost()
                                    .small()
                                    .disabled(!connected || busy || busy_elsewhere)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let Some(port) = &this.port else { return };
                                        let path = port.path.clone();
                                        _ = this.owner.update(cx, |workspace, cx| {
                                            workspace.select_port(path);
                                            workspace.repl_connect();
                                            workspace.show_tool(Tool::Repl, window, cx);
                                            cx.notify();
                                        });
                                    })),
                            ),
                    )
                    .when_some(self.running.clone(), |this, running| {
                        this.child(
                            h_flex()
                                .gap_1p5()
                                .text_xs()
                                .text_color(cx.theme().info)
                                .child(Icon::new(IconName::LoaderCircle).size_3())
                                .child(format!("{running}… other boards stay usable")),
                        )
                    })
                    .when_some(self.result.as_ref(), |this, result| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(match result {
                                    Ok(_) => cx.theme().success,
                                    Err(_) => cx.theme().danger,
                                })
                                .child(match result {
                                    Ok(_) => "Finished".to_string(),
                                    Err(error) => error.clone(),
                                }),
                        )
                    }),
            )
            .child(
                h_flex()
                    .h_9()
                    .px_3()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Output"),
                    )
                    .child(
                        h_flex()
                            .gap_0p5()
                            .child(
                                Button::new("panel-copy-output")
                                    .icon(IconName::Copy)
                                    .ghost()
                                    .small()
                                    .disabled(self.log.is_empty())
                                    .accessibility_label("Copy output")
                                    .tooltip("Copy output")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            this.log.join("\n"),
                                        ));
                                    })),
                            )
                            .child(
                                Button::new("panel-clear-output")
                                    .icon(IconName::Trash)
                                    .ghost()
                                    .small()
                                    .disabled(busy || self.log.is_empty())
                                    .accessibility_label("Clear output")
                                    .tooltip("Clear output")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.log.clear();
                                        this.result = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("flash-output")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px_3()
                    .py_2()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .when(self.log.is_empty(), |this| {
                        this.child("Flash and upload progress for this board appears here.")
                    })
                    .children(self.log.iter().map(|line| div().child(line.clone()))),
            )
    }
}
