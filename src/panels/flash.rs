use super::Tool;
use crate::boards::{self, BoardDef, BoardType, PortIdentity, Runtime};
use crate::device::{self, DeviceEvent};
use crate::hardware::{device_key, PortLeases, RecentBoard};
use crate::workspace_state::Workspace;
use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent, PanelInfo, PanelState};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, WeakEntity, Window,
};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct BoardChoice {
    id: String,
    name: &'static str,
}
impl SelectItem for BoardChoice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.name.into()
    }
    fn value(&self) -> &String {
        &self.id
    }
}
#[derive(Clone, Copy)]
enum Job {
    Firmware,
    Upload,
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
    picker: Entity<SelectState<SearchableVec<BoardChoice>>>,
    busy: bool,
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
        let choices: Vec<_> = boards::BOARD_CATALOG
            .iter()
            .map(|board| BoardChoice {
                id: board.id.into(),
                name: board.name,
            })
            .collect();
        let index = choices
            .iter()
            .position(|choice| Some(&choice.id) == selected.as_ref());
        let picker = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(choices),
                index.map(|index| gpui_kit::base::IndexPath::default().row(index)),
                window,
                cx,
            )
        });
        let subscription = cx.subscribe_in(
            &picker,
            window,
            |this, _, event: &SelectEvent<SearchableVec<BoardChoice>>, _, cx| {
                if let SelectEvent::Confirm(Some(id)) = event {
                    if this.busy {
                        return;
                    }
                    this.board_id = Some(id.clone());
                    this.record.board_id = Some(id.clone());
                    if let Some(board) = boards::board_by_id(id) {
                        this.record.name = board.name.into();
                    }
                    let identity = this.port.clone().unwrap_or_else(|| PortIdentity {
                        path: this.record.path.clone(),
                        vendor_id: this.record.vendor_id.clone(),
                        product_id: this.record.product_id.clone(),
                        serial_number: this.record.serial_number.clone(),
                        manufacturer: None,
                    });
                    let record = this.record.clone();
                    _ = this.owner.update(cx, |workspace, cx| {
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
                            saved.board_id = record.board_id;
                            saved.name = record.name;
                        }
                        workspace.settings.save();
                        workspace.save_panel_layout(cx);
                        cx.notify();
                    });
                    cx.notify();
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
            busy: false,
            log: Vec::new(),
            result: None,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
            _job_task: None,
        }
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
        if !self.busy {
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

    fn confirm_firmware(&self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(board), Some(port)) = (self.board(), &self.port) else {
            return;
        };
        if self.busy {
            return;
        }
        let target = (device_key(port), port.path.clone(), board.id.to_string());
        let panel = cx.weak_entity();
        let name = board.name;
        let firmware = board.mpy_variant.unwrap_or("Unknown");
        let path = port.path.clone();
        let is_pico = board.flash_board_type == BoardType::RaspberryPi;
        window.open_dialog(cx, move |dialog, _, _| {
            let panel = panel.clone();
            let target = target.clone();
            dialog.title(format!("Flash {name}?"))
                .child(v_flex().gap_3().child(format!("Target: {path}"))
                    .child(format!("Firmware: MicroPython · {firmware}"))
                    .child("This replaces the board's firmware. Back up any files you need first.")
                    .when(is_pico, |this| this.child("For Pico, the target is the RPI-RP2 boot volume. Connect only the intended Pico in BOOTSEL mode when prompted.")))
                .footer(DialogFooter::new()
                    .child(Button::new("cancel-flash").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("confirm-flash").label("Flash firmware").primary().on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                        _ = panel.update(cx, |this, cx| {
                            let matches = this.port.as_ref().is_some_and(|port| device_key(port) == target.0 && port.path == target.1)
                                && this.board_id.as_deref() == Some(target.2.as_str());
                            if matches { this.start_job(Job::Firmware, cx); }
                            else { this.result = Some(Err("The target changed. Review the board and try again.".into())); cx.notify(); }
                        });
                    })))
        });
    }

    fn start_job(&mut self, job: Job, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(port), Some(board)) = (self.port.clone(), self.board()) else {
            self.result = Some(Err(
                "Connect this board and select its exact model first.".into()
            ));
            cx.notify();
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let workspace = owner.read(cx);
        let files = workspace.project_files.clone();
        let project = workspace.project_path.clone();
        let fqbn = workspace
            .active_profile
            .as_ref()
            .filter(|profile| profile.port_path == port.path)
            .and_then(|profile| profile.fqbn.clone())
            .unwrap_or_else(|| "arduino:avr:uno".into());
        if matches!(job, Job::Upload) && project.is_none() {
            self.result = Some(Err("Open a project before uploading.".into()));
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
        // BOOTSEL volumes lack the selected serial identity, so only one UF2
        // operation may run at a time even across different board panels.
        let boot_lease =
            if matches!(job, Job::Firmware) && board.flash_board_type == BoardType::RaspberryPi {
                match self.leases.acquire("anode:rp2040-boot-volume") {
                    Ok(lease) => Some(lease),
                    Err(_) => {
                        self.result = Some(Err(
                            "Another Pico firmware operation is using the boot volume.".into(),
                        ));
                        cx.notify();
                        return;
                    }
                }
            } else {
                None
            };
        let (sender, receiver) = std::sync::mpsc::channel();
        self.log.clear();
        self.log.push(format!("{} · {}", board.name, port.path));
        self.result = None;
        self.busy = true;
        let spawned = std::thread::Builder::new()
            .name(format!("anode-device-{}", self.record.key))
            .spawn(move || {
                let _lease = lease;
                let _boot_lease = boot_lease;
                let mut emit = |event: DeviceEvent| {
                    let DeviceEvent::Progress(message) = event;
                    let _ = sender.send(JobEvent::Progress(message));
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match job {
                    Job::Firmware => {
                        if board.runtime == Runtime::Arduino {
                            return Err("Use Upload sketch for Arduino boards.".into());
                        }
                        let result = device::flash_firmware(
                            &port.path,
                            board.flash_board_type,
                            board.mpy_variant,
                            &mut emit,
                        );
                        if result.success {
                            Ok(())
                        } else {
                            Err(format!(
                                "{}{}",
                                result.error.unwrap_or_else(|| "Flashing failed".into()),
                                result
                                    .hint
                                    .map(|hint| format!("\n{hint}"))
                                    .unwrap_or_default()
                            ))
                        }
                    }
                    Job::Upload if board.runtime == Runtime::Arduino => device::arduino_upload(
                        &port.path,
                        project.as_deref().expect("validated project"),
                        &fqbn,
                        &mut emit,
                    ),
                    Job::Upload => {
                        if !files
                            .iter()
                            .any(|file| file.language == crate::project::Language::Python)
                        {
                            return Err("The project has no Python files to upload.".into());
                        }
                        for file in files
                            .iter()
                            .filter(|file| file.language == crate::project::Language::Python)
                        {
                            emit(DeviceEvent::Progress(format!("Uploading {}…", file.name)));
                            device::upload_code(&port.path, &file.name, &file.content, &mut emit)?;
                        }
                        Ok(())
                    }
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
            self.busy = false;
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
                    self.log.push(if result.is_ok() {
                        "Operation complete".into()
                    } else {
                        "Operation failed".into()
                    });
                    self.busy = false;
                    self.result = Some(result);
                    changed = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.result = Some(Err("Device worker disconnected.".into()));
                    changed = true;
                    break;
                }
            }
        }
        if self.log.len() > 1200 {
            self.log.drain(..self.log.len() - 1200);
        }
        if changed {
            cx.notify();
        }
        !self.busy
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
        !self.busy
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
        format!(
            "{}{}",
            if self.busy {
                "Working · "
            } else {
                "Board · "
            },
            self.board()
                .map(|board| board.name)
                .unwrap_or(&self.record.name)
        )
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(
            format!(
                "{} · {}",
                self.record.name,
                self.record
                    .serial_number
                    .as_deref()
                    .unwrap_or(&self.record.path)
            )
            .into(),
        )
    }
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}
impl Render for FlashPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.port.is_some();
        let busy_elsewhere = !self.busy && self.leases.is_busy(&self.record.key);
        let board = self.board();
        let can_flash = connected
            && !self.busy
            && !busy_elsewhere
            && board.is_some_and(|board| board.mpy_variant.is_some());
        let can_upload = connected && board.is_some() && !self.busy && !busy_elsewhere;
        v_flex().size_full().min_h_0().track_focus(&self.focus).bg(cx.theme().background)
            .child(v_flex().p_4().gap_3()
                .child(h_flex().gap_2()
                    .child(Icon::new(IconName::Cpu).size_5().text_color(cx.theme().info))
                    .child(div().flex_1().min_w_0().text_base().font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(board.map(|board| board.name).unwrap_or(&self.record.name).to_string()))
                    .child(div().text_xs().text_color(if connected { cx.theme().success } else { cx.theme().muted_foreground })
                        .child(if connected { "Connected" } else { "Offline" })))
                .child(div().text_xs().font_family(cx.theme().mono_font_family.clone()).text_color(cx.theme().muted_foreground)
                    .child(self.port.as_ref().map(|port| port.path.as_str()).unwrap_or(&self.record.path).to_string()))
                .child(v_flex().gap_1().child(div().text_xs().child("Board model"))
                    .child(Select::new(&self.picker).small().disabled(self.busy).placeholder("Identify board…").accessibility_label("Board model")))
                .when_some(board, |this, board| this.child(div().text_xs().text_color(cx.theme().muted_foreground)
                    .child(format!("{} · {}{}", board.chip.label(), board.runtime.label(), if board.generic { " · confirm your module" } else { "" }))))
                .when(!connected, |this| this.child(div().text_xs().text_color(cx.theme().muted_foreground)
                    .child("Reconnect this board over USB. This panel will detect it automatically.")))
                .when(busy_elsewhere, |this| this.child(div().text_xs().text_color(cx.theme().warning)
                    .child("This board is in use by another panel or the REPL.")))
                .child(h_flex().gap_2()
                    .child(Button::new("panel-flash-firmware").label("Flash firmware…").icon(IconName::Download).primary().small()
                        .disabled(!can_flash).on_click(cx.listener(|this, _, window, cx| this.confirm_firmware(window, cx))))
                    .child(Button::new("panel-upload-project").label(if board.is_some_and(|board| board.runtime == Runtime::Arduino) { "Upload sketch" } else { "Upload project" })
                        .icon(IconName::Upload).outline().small().disabled(!can_upload)
                        .on_click(cx.listener(|this, _, _, cx| this.start_job(Job::Upload, cx)))))
                .child(Button::new("board-use-for-run").label("Use for Run").icon(IconName::Play).ghost().small()
                    .disabled(!connected || self.busy || busy_elsewhere)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let Some(port) = &this.port else { return; };
                        let path = port.path.clone();
                        _ = this.owner.update(cx, |workspace, cx| { workspace.select_port(path); cx.notify(); });
                    })))
                .child(Button::new("board-use-repl").label("Connect REPL").icon(IconName::SquareTerminal).ghost().small()
                    .disabled(!connected || self.busy || busy_elsewhere)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let Some(port) = &this.port else { return; };
                        let path = port.path.clone();
                        _ = this.owner.update(cx, |workspace, cx| {
                            workspace.select_port(path); workspace.repl_connect();
                            workspace.show_tool(Tool::Repl, window, cx); cx.notify();
                        });
                    })))
                .when(self.busy, |this| this.child(div().text_xs().text_color(cx.theme().info).child("Working… You can use other device panels.")))
                .when_some(self.result.as_ref(), |this, result| this.child(div().text_xs()
                    .text_color(if result.is_ok() { cx.theme().success } else { cx.theme().danger })
                    .child(result.as_ref().map(|_| "Operation complete".to_string()).unwrap_or_else(|error| error.clone())))))
            .child(h_flex().px_4().py_2().border_t_1().border_color(cx.theme().border).justify_between()
                .child(div().text_xs().child("Device output"))
                .child(Button::new("clear-device-output").icon(IconName::Trash).ghost().small().tooltip("Clear this panel’s output")
                    .disabled(self.busy).on_click(cx.listener(|this, _, _, cx| { this.log.clear(); this.result = None; cx.notify(); }))))
            .child(div().id("flash-output").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll)
                .child(v_flex().p_4().gap_1().text_xs().font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .when(self.log.is_empty(), |this| this.child("Flash and upload progress appears here."))
                    .children(self.log.iter().map(|line| div().child(line.clone())))))
    }
}
