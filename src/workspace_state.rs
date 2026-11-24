//! The main Anode workspace state.
//!
//! Domain state lives in named fields, editing state in child entities
//! created once in `new`, and everything asynchronous in explicit tasks and
//! subscription handlers. Device work runs on background threads and reports
//! back through channels drained on the UI thread.

use crate::boards::{self, BoardDef, BoardMatch, PortIdentity, Runtime};
use crate::device::{self, DeviceEvent, FirmwareSource};
use crate::esp;
use crate::profiles::{self, CircuitPythonDrive, Confidence, ExecutionProfile, WorkspaceLayout};
use crate::project::{self, Language, ProjectFile, ProjectKind};
use crate::repl::{self, ReplConnection, ReplEvent};
use crate::settings::{BottomMode, Settings};
use crate::toolchain::{self, Tool, ToolStatus};

use gpui_kit::base::input::EditorState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    AppContext as _, Context, Entity, ScrollHandle, SharedString, Subscription, Task, Window,
};

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Log entries
// ---------------------------------------------------------------------------

/// What asking a port's chip to identify itself produced.
///
/// A port is asked at most once per session unless the user asks again, so a
/// board that is not Espressif costs one short handshake and is then left
/// alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChipProbe {
    Running,
    Found(esp::ChipReport),
    /// Nothing answered the Espressif bootloader on this port.
    Silent,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Warn,
    Error,
    Ok,
    Install,
    Flash,
}

impl LogKind {
    /// True for the lines the Problems filter keeps.
    pub fn is_problem(self) -> bool {
        matches!(self, LogKind::Warn | LogKind::Error)
    }
}

/// What the device log is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFilter {
    #[default]
    All,
    Problems,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub id: u64,
    pub kind: LogKind,
    /// Local wall-clock time, so a long flash can be read against the board's
    /// own behaviour.
    pub time: SharedString,
    /// `SharedString` because the Output panel clones every visible entry on
    /// each frame; copying two `String`s per line per frame is what made a
    /// running flash feel heavy.
    pub message: SharedString,
    pub(crate) project: Option<PathBuf>,
}

/// Which way an Up/Down key moves through REPL history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryStep {
    /// Up: towards the commands typed earlier.
    Older,
    /// Down: back towards the line being typed.
    Newer,
}

/// One open editor tab.
#[derive(Debug, Clone)]
pub struct OpenTab {
    pub path: PathBuf,
    pub name: String,
    pub is_dirty: bool,
}

/// Events forwarded from background device work to the UI.
enum BackgroundOutcome {
    Progress(String),
    Finished(Result<(), String>, Option<crate::recovery::RecoveryContext>),
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

/// The full-window destination the user is looking at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Screen {
    #[default]
    Workspace,
    Settings,
}

/// What each dock group shows when the window first opens.
fn default_visible_panels(
) -> std::collections::BTreeMap<crate::panels::ToolGroup, crate::panels::PanelChoice> {
    use crate::panels::{PanelChoice, Tool};
    [Tool::Files, Tool::Editor, Tool::Device, Tool::Output]
        .into_iter()
        .map(|tool| (tool.group(), PanelChoice::Tool(tool)))
        .collect()
}

pub struct Workspace {
    pub(crate) agents: crate::agent::AgentService,
    pub(crate) _agent_poll: Option<Task<()>>,
    pub(crate) _agent_watch: Option<Task<()>>,
    // Persisted settings (recent projects, last port, board overrides).
    pub settings: Settings,

    // Project state.
    pub project_path: Option<PathBuf>,
    pub project_name: String,
    pub project_kind: ProjectKind,
    pub project_files: Vec<ProjectFile>,
    pub current_file: Option<ProjectFile>,
    pub is_dirty: bool,
    /// `[env:…]` names from a PlatformIO project, when one is open.
    pub platformio_environments: Vec<String>,

    // Multi-tab editor state.
    pub open_tabs: Vec<OpenTab>,
    pub active_tab: Option<PathBuf>,

    // Editor state (retained entity).
    pub editor: Entity<EditorState>,
    pub(crate) terminal: Entity<crate::terminal::TerminalView>,
    /// The file the editor currently shows, so changes land in the right slot.
    editor_file: Option<PathBuf>,
    /// Guard so a programmatic `set_value` doesn't mark the tab dirty.
    editor_syncing: bool,

    // Connection state.
    pub ports: Vec<PortIdentity>,
    pub selected_port: String,
    pub available_profiles: Vec<ExecutionProfile>,
    pub active_profile: Option<ExecutionProfile>,
    pub profile_manually_set: bool,
    pub circuit_python_drive: Option<CircuitPythonDrive>,
    pub connection_dropped: bool,
    pub(crate) device_recovery: Option<crate::recovery::RecoveryIssue>,
    /// What the chip on each physical board said about itself, keyed the same
    /// way the board history is. A USB bridge can only name a family and often
    /// names the wrong one, so this is what makes detection trustworthy.
    pub chip_probes: std::collections::HashMap<String, ChipProbe>,
    _chip_probe_task: Option<Task<()>>,
    /// Set when a flash took the REPL's port away from it, so the REPL can be
    /// put back once the board reboots.
    repl_resumes_after_work: bool,

    // Toolchain availability, refreshed off the UI thread.
    pub tool_statuses: Vec<ToolStatus>,
    pub tools_scanning: bool,
    _tools_task: Option<Task<()>>,

    // REPL state.
    pub repl_connection: Option<ReplConnection>,
    pub repl_connected: bool,
    pub repl_output: String,
    pub repl_input: Entity<InputState>,
    /// Edits the `arduino-cli` board name for the selected board. Retained so
    /// a half-typed FQBN survives a re-render and a USB poll tick.
    pub fqbn_input: Entity<InputState>,
    /// The board `fqbn_input` currently holds a value for, so switching board
    /// replaces it instead of carrying the previous board's name over.
    fqbn_input_board: Option<&'static str>,
    repl_events: Option<Receiver<ReplEvent>>,
    pub repl_history: Vec<String>,
    /// Where the Up/Down keys currently sit in `repl_history`.
    pub repl_history_cursor: Option<usize>,

    // Device log.
    pub log_entries: Vec<LogEntry>,
    pub log_filter: LogFilter,
    /// Whether the log keeps the newest line in view. Turning it off lets the
    /// user read back through a long flash while it is still running.
    pub log_follow: bool,
    pub log_scroll: ScrollHandle,
    next_log_id: u64,

    // Background device work: progress lines and completion.
    device_events: Option<Receiver<BackgroundOutcome>>,
    device_work_project: Option<PathBuf>,
    pub device_busy: bool,
    /// Where the running job has got to, read from its own output, so a long
    /// write shows a phase and a percentage rather than only a spinner.
    device_progress: Option<esp::Progress>,
    /// A heartbeat that repaints while a device job runs. Progress is drained
    /// in `render`, and the port poll now only repaints when discovery
    /// changed, so a long flash needs its own reason to draw frames.
    _device_poll: Option<Task<()>>,

    // Bottom panel.
    pub bottom_mode: BottomMode,
    pub(crate) workspace_mode: crate::panels::Tool,
    /// Which full-window screen the user is on. Settings is a destination of
    /// its own rather than a panel competing for dock space.
    pub(crate) screen: Screen,
    /// The one panel each dock group shows. Everything else in the group stays
    /// hidden, which is what keeps the docks free of a tab strip.
    pub(crate) visible_panels:
        std::collections::BTreeMap<crate::panels::ToolGroup, crate::panels::PanelChoice>,
    pub(crate) panels: Option<crate::panels::PanelWorkspace>,
    pub(crate) port_leases: crate::hardware::PortLeases,
    pub(crate) layout_save_task: Option<Task<()>>,

    // Retained Select entities (port, board, runtime profile).
    pub pickers: crate::views::Pickers,
    pub picker_caches: crate::views::PickerCaches,
    /// Set by the async folder picker; consumed on the next render pass where
    /// window access is available.
    pub pending_open_path: Option<PathBuf>,
    /// Set when a project was loaded without window access; the render pass
    /// then points the editor at the new current file.
    pub needs_editor_sync: bool,

    // Background port polling task.
    _poll_task: Option<Task<()>>,

    pub(crate) _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();

        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("python")
                .placeholder("Open a file from the sidebar to start editing")
        });
        let terminal = cx.new(crate::terminal::TerminalView::new);
        let repl_input = cx.new(|cx| InputState::new(window, cx).placeholder(">>> "));
        let fqbn_input = cx.new(|cx| InputState::new(window, cx).placeholder("vendor:arch:board"));

        let editor_subscription =
            cx.subscribe_in(&editor, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change) && !this.editor_syncing {
                    if let Some(path) = this.editor_file.clone() {
                        let content = state.read(cx).value().to_string();
                        this.update_file_content(&path, content);
                        cx.notify();
                    }
                }
            });

        let repl_subscription =
            cx.subscribe_in(&repl_input, window, |this, state, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let line = state.read(cx).value().to_string();
                    this.send_repl_line(line);
                    state.update(cx, |state, cx| {
                        state.set_value("", window, cx);
                    });
                    cx.notify();
                }
            });

        let mut agents = crate::agent::AgentService::default();
        // Only a provider the user chose, and that is still installed, comes
        // back after a restart. Anode never picks one on its own.
        if let Some(id) = settings.agent_provider.clone() {
            agents.restore_selection(&crate::agent::ProviderId::from(id));
        }
        agents.restore_chats(settings.agent_chats.clone());

        let mut this = Self {
            agents,
            _agent_poll: None,
            _agent_watch: None,
            settings,
            project_path: None,
            project_name: String::new(),
            project_kind: ProjectKind::Unknown,
            project_files: Vec::new(),
            current_file: None,
            is_dirty: false,
            platformio_environments: Vec::new(),
            open_tabs: Vec::new(),
            active_tab: None,
            editor,
            terminal,
            editor_file: None,
            editor_syncing: false,
            ports: Vec::new(),
            selected_port: String::new(),
            available_profiles: Vec::new(),
            active_profile: None,
            profile_manually_set: false,
            circuit_python_drive: None,
            connection_dropped: false,
            device_recovery: None,
            chip_probes: std::collections::HashMap::new(),
            _chip_probe_task: None,
            repl_resumes_after_work: false,
            tool_statuses: Vec::new(),
            tools_scanning: false,
            _tools_task: None,
            repl_connection: None,
            repl_connected: false,
            repl_output: String::new(),
            repl_input,
            fqbn_input,
            fqbn_input_board: None,
            repl_events: None,
            repl_history: Vec::new(),
            repl_history_cursor: None,
            log_entries: Vec::new(),
            log_filter: LogFilter::default(),
            log_follow: true,
            log_scroll: ScrollHandle::new(),
            next_log_id: 1,
            device_events: None,
            device_work_project: None,
            device_busy: false,
            device_progress: None,
            _device_poll: None,
            bottom_mode: BottomMode::Shell,
            workspace_mode: crate::panels::Tool::Editor,
            screen: Screen::Workspace,
            visible_panels: default_visible_panels(),
            panels: None,
            port_leases: Default::default(),
            layout_save_task: None,
            pickers: Default::default(),
            picker_caches: Default::default(),
            pending_open_path: None,
            needs_editor_sync: false,
            _poll_task: None,
            _subscriptions: vec![editor_subscription, repl_subscription],
        };
        this.start_port_polling(cx);
        this.start_agent_poll(cx);
        this._subscriptions
            .push(cx.on_app_quit(|this, cx| this.agents.shutdown(cx)));
        this.refresh_tools(cx);
        this
    }

    // -- Port polling --------------------------------------------------------

    /// Poll serial ports every second, auto-detect the board, and keep the
    /// runtime profile in step.
    fn start_port_polling(&mut self, cx: &mut Context<Self>) {
        self._poll_task = Some(
            cx.spawn(async move |this, cx: &mut gpui_kit::AsyncApp| loop {
                let (ports, drive) = cx
                    .background_spawn(async {
                        (list_serial_ports(), device::detect_circuitpython())
                    })
                    .await;
                if this
                    .update(cx, |this, cx| {
                        // Only a snapshot that actually changed something
                        // visible is worth a frame. Notifying on every tick
                        // redrew the whole window once a second forever,
                        // which is what made typing elsewhere feel heavy.
                        let changed = match ports {
                            Ok(ports) => this.apply_port_snapshot(ports, drive, cx),
                            Err(_) => false,
                        };
                        if changed {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }),
        );
    }

    pub fn refresh_ports(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let (ports, drive) = cx
                .background_spawn(async { (list_serial_ports(), device::detect_circuitpython()) })
                .await;
            _ = this.update(cx, |this, cx| {
                match ports {
                    Ok(ports) => {
                        this.apply_port_snapshot(ports, drive, cx);
                    }
                    Err(error) => {
                        this.add_log(LogKind::Error, format!("Could not scan USB ports: {error}"))
                    }
                }
                // An explicit Scan always repaints, so the button visibly
                // does something even when nothing was found.
                cx.notify();
            });
        })
        .detach();
    }

    /// Apply a discovery snapshot. Returns whether anything the UI draws
    /// changed, so a quiet tick costs no frame.
    fn apply_port_snapshot(
        &mut self,
        ports: Vec<PortIdentity>,
        drive: CircuitPythonDrive,
        cx: &mut Context<Self>,
    ) -> bool {
        let ports_changed = ports != self.ports;
        let drive_changed = self.circuit_python_drive.as_ref() != Some(&drive);
        let before = self.detection_state();
        let previous_key = self
            .selected_port_identity()
            .as_ref()
            .map(crate::hardware::device_key)
            .or_else(|| {
                self.settings
                    .recent_boards
                    .iter()
                    .find(|record| record.path == self.selected_port)
                    .map(|record| record.key.clone())
            });
        let now = now_millis();
        if crate::hardware::remember_connections(
            &mut self.settings.recent_boards,
            &self.ports,
            &ports,
            &self.settings.board_overrides,
            now,
        ) {
            self.settings.save();
        }
        if let Some(key) = previous_key {
            if let Some(current) = ports
                .iter()
                .find(|port| crate::hardware::device_key(port) == key)
            {
                self.selected_port = current.path.clone();
            }
        }
        self.ports = ports;
        self.circuit_python_drive = Some(drive);
        self.auto_connect_if_needed();
        self.detect_board_for_selected_port();
        // A bridge descriptor cannot name the chip, so ask the chip. Only
        // a port that could still answer something new is asked, and only
        // once.
        if self
            .selected_port_identity()
            .is_some_and(|port| self.chip_probe_is_useful(&port))
        {
            self.probe_chip(false, cx);
        }
        ports_changed || drive_changed || before != self.detection_state()
    }

    /// The parts of detection the UI reads, as a comparable snapshot.
    fn detection_state(&self) -> (String, bool, Option<String>, usize, usize) {
        (
            self.selected_port.clone(),
            self.connection_dropped,
            self.active_profile
                .as_ref()
                .map(|profile| profile.id.clone()),
            self.available_profiles.len(),
            self.settings.recent_boards.len(),
        )
    }

    fn auto_connect_if_needed(&mut self) {
        if !self.settings.auto_connect
            || self.device_busy
            || self.repl_connected
            || self
                .ports
                .iter()
                .any(|port| port.path == self.selected_port)
        {
            return;
        }
        let candidate = if self.settings.last_port_path.is_empty() {
            None
        } else {
            self.ports
                .iter()
                .find(|port| {
                    port.path == self.settings.last_port_path
                        && profiles::is_likely_hardware_port(port)
                })
                .cloned()
        };
        let candidate = candidate.or_else(|| profiles::preferred_port(&self.ports).cloned());
        if let Some(port) = candidate {
            self.selected_port = port.path.clone();
            self.settings.last_port_path = port.path.clone();
            self.settings.save();
            self.rebuild_profiles_for(&port);
        }
    }

    fn detect_board_for_selected_port(&mut self) {
        if self.selected_port.is_empty() {
            return;
        }
        let current = self
            .ports
            .iter()
            .find(|port| port.path == self.selected_port)
            .cloned();
        let Some(current) = current else {
            // The selected port vanished — flag the drop and wait for it.
            if !self.connection_dropped {
                self.connection_dropped = true;
                self.add_log(
                    LogKind::Warn,
                    format!("Connection lost on {}", self.selected_port),
                );
            }
            return;
        };

        if self.connection_dropped {
            self.connection_dropped = false;
            self.add_log(LogKind::Ok, format!("Reconnected to {}", current.path));
        }

        // Keep the runtime in step with what is actually on this port: board
        // swaps, a CIRCUITPY drive mounting, a fresh-launch restore.
        self.available_profiles = self.detect_profiles_for(&current);
        let next = profiles::resolve_profile_on_detect(
            self.active_profile.as_ref(),
            &self.available_profiles,
            Some(&self.settings.last_profile_id),
            self.profile_manually_set,
        );
        if let Some(profile) = next {
            if self.active_profile.as_ref() != Some(&profile) {
                self.settings.last_profile_id = profile.id.clone();
                self.settings.save();
                self.active_profile = Some(profile);
            }
        }
    }

    fn detect_profiles_for(&self, port: &PortIdentity) -> Vec<ExecutionProfile> {
        let board = self.board_for_port(port);
        profiles::detect_profiles(port, &board, self.circuit_python_drive.as_ref())
    }

    fn rebuild_profiles_for(&mut self, port: &PortIdentity) {
        self.available_profiles = self.detect_profiles_for(port);
        if let Some(profile) = profiles::select_best_profile(&self.available_profiles) {
            self.settings.last_profile_id = profile.id.clone();
            self.settings.save();
            self.active_profile = Some(profile);
            self.profile_manually_set = false;
        }
    }

    /// The user picked a port in the UI.
    pub fn select_port(&mut self, path: String) {
        if path.is_empty() || path == self.selected_port {
            return;
        }
        if self.repl_connected {
            self.repl_disconnect();
        }
        self.selected_port = path.clone();
        self.connection_dropped = false;
        self.settings.last_port_path = path.clone();
        self.settings.save();
        if let Some(port) = self.ports.iter().find(|port| port.path == path).cloned() {
            self.rebuild_profiles_for(&port);
        }
    }

    /// The user pinned an exact board from the picker.
    pub fn set_board_override(&mut self, board_id: &str) {
        let Some(port) = self.selected_port_identity() else {
            return;
        };
        self.settings
            .board_overrides
            .insert(boards::port_key(&port), board_id.to_string());
        self.settings.save();
        // The board decides which runtimes exist, so the profile set has to
        // be rebuilt rather than merely relabelled.
        self.available_profiles = self.detect_profiles_for(&port);
        if let Some(profile) = profiles::select_best_profile(&self.available_profiles) {
            self.active_profile = Some(profile);
            self.profile_manually_set = false;
        }
        if let Some(board) = boards::board_by_id(board_id) {
            self.add_log(LogKind::Ok, format!("Board set to {}", board.name));
        }
    }

    /// The user chose a runtime explicitly.
    pub fn select_profile(&mut self, profile_id: &str) {
        if let Some(profile) = self
            .available_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
        {
            self.settings.last_profile_id = profile.id.clone();
            self.settings.save();
            self.active_profile = Some(profile);
            self.profile_manually_set = true;
        }
    }

    /// Replace the FQBN Anode uses for a board. An empty value restores the
    /// catalog default.
    pub fn set_fqbn_override(&mut self, board_id: &str, fqbn: &str) {
        let fqbn = fqbn.trim();
        let board = boards::board_by_id(board_id);
        if fqbn.is_empty() || board.and_then(|board| board.fqbn) == Some(fqbn) {
            if self.settings.fqbn_overrides.remove(board_id).is_some() {
                self.add_log(LogKind::Ok, "Restored the default Arduino board name");
            }
        } else {
            self.settings
                .fqbn_overrides
                .insert(board_id.to_string(), fqbn.to_string());
            self.add_log(LogKind::Ok, format!("Arduino board name set to {fqbn}"));
        }
        self.settings.save();
        // Re-seed the field from settings on the next sync.
        self.fqbn_input_board = None;
    }

    /// Keep the FQBN field showing the selected board's value. Called from
    /// render, so it only writes when the board actually changed.
    pub fn sync_fqbn_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(board) = self.active_board() else {
            return;
        };
        if self.fqbn_input_board == Some(board.id) {
            return;
        }
        self.fqbn_input_board = Some(board.id);
        let value = self.fqbn_for(board).unwrap_or_default();
        self.fqbn_input.update(cx, |state, cx| {
            state.set_value(value, window, cx);
        });
    }

    /// Commit whatever the FQBN field currently holds.
    pub fn commit_fqbn_input(&mut self, cx: &mut Context<Self>) {
        let Some(board) = self.active_board() else {
            return;
        };
        let value = self.fqbn_input.read(cx).value().to_string();
        self.set_fqbn_override(board.id, &value);
    }

    // -- Board and runtime ---------------------------------------------------

    pub fn selected_port_identity(&self) -> Option<PortIdentity> {
        self.ports
            .iter()
            .find(|port| port.path == self.selected_port)
            .cloned()
    }

    pub fn resolved_board(&self) -> BoardMatch {
        self.selected_port_identity()
            .map(|port| self.board_for_port(&port))
            .unwrap_or_default()
    }

    /// The board on a port: what the USB descriptor suggests, corrected by
    /// what the chip itself reported when it was asked.
    pub fn board_for_port(&self, port: &PortIdentity) -> BoardMatch {
        let matched = boards::identify_board(Some(port), &self.settings.board_overrides);
        match self.chip_probes.get(&crate::hardware::device_key(port)) {
            Some(ChipProbe::Found(report)) => match report.chip {
                Some(chip) => matched.refined_by_chip(chip),
                None => matched,
            },
            _ => matched,
        }
    }

    /// What the chip on a port reported, when it has been asked and answered.
    pub fn chip_report_for(&self, port: &PortIdentity) -> Option<&esp::ChipReport> {
        match self.chip_probes.get(&crate::hardware::device_key(port)) {
            Some(ChipProbe::Found(report)) => Some(report),
            _ => None,
        }
    }

    /// How the probe of the selected port stands.
    pub fn chip_probe(&self) -> Option<&ChipProbe> {
        let port = self.selected_port_identity()?;
        self.chip_probes.get(&crate::hardware::device_key(&port))
    }

    /// True while the selected port's chip is being asked what it is.
    pub fn chip_probe_running(&self) -> bool {
        matches!(self.chip_probe(), Some(ChipProbe::Running))
    }

    /// Whether asking this port's chip could tell us anything new.
    ///
    /// Worth asking when the descriptor left a real question: an Espressif
    /// chip's own USB, a USB-UART bridge that fronts anything, or a board the
    /// catalog resolved only to a chip family. A board the user pinned, or one
    /// a product id identified outright, has nothing left to learn.
    pub fn chip_probe_is_useful(&self, port: &PortIdentity) -> bool {
        // Probing resets a real board. Tests run against whatever hardware
        // happens to be plugged into the developer's machine, so they opt out
        // the same way they redirect the config directory.
        if std::env::var_os("ANODE_DISABLE_HARDWARE_PROBE").is_some() {
            return false;
        }
        if toolchain::locate(Tool::Esptool) == toolchain::Location::Missing {
            return false;
        }
        let matched = boards::identify_board(Some(port), &self.settings.board_overrides);
        if matched.from_override || matched.exact {
            return false;
        }
        esp::is_native_usb(port)
            || boards::is_usb_uart_bridge(port)
            || matched.board.is_some_and(|board| board.chip.is_espressif())
    }

    /// Ask the chip on the selected port to identify itself, once.
    ///
    /// `deliberate` separates the user pressing the button from the automatic
    /// look on connect. The automatic one gives up after two handshakes, so a
    /// port that fronts something other than an Espressif chip costs a couple
    /// of seconds and is then left alone for the rest of the session.
    pub fn probe_chip(&mut self, deliberate: bool, cx: &mut Context<Self>) {
        let Some(port) = self.selected_port_identity() else {
            return;
        };
        self.probe_chip_on(&port, deliberate, cx);
    }

    /// Ask the chip on a particular port, so a per-board panel can identify
    /// its own board without first making it the workspace's selection.
    pub fn probe_chip_on(&mut self, port: &PortIdentity, deliberate: bool, cx: &mut Context<Self>) {
        let key = crate::hardware::device_key(port);
        if self.device_busy || matches!(self.chip_probes.get(&key), Some(ChipProbe::Running)) {
            return;
        }
        if !deliberate && self.chip_probes.contains_key(&key) {
            return;
        }
        // Probing resets the chip into its bootloader, so it needs the port to
        // itself exactly as a flash would.
        let was_repl_connected = self.repl_connected && self.selected_port == port.path;
        if was_repl_connected {
            if !deliberate {
                return;
            }
            self.repl_disconnect();
        }
        let lease = match self.port_leases.acquire(&key) {
            Ok(lease) => lease,
            Err(error) => {
                if deliberate {
                    self.add_log(LogKind::Warn, error);
                }
                return;
            }
        };
        let path = port.path.clone();
        let attempts = if deliberate { 7 } else { 2 };
        self.chip_probes.insert(key.clone(), ChipProbe::Running);
        if deliberate {
            self.add_log(LogKind::Info, format!("Identifying the chip on {path}"));
        }
        self._chip_probe_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let _lease = lease;
                    esp::probe(&path, attempts, &mut |_| {})
                })
                .await;
            _ = this.update(cx, |this, cx| {
                this.finish_chip_probe(key, result, deliberate, was_repl_connected, cx);
            });
        }));
        cx.notify();
    }

    fn finish_chip_probe(
        &mut self,
        key: String,
        result: Result<esp::ChipReport, esp::ProbeError>,
        deliberate: bool,
        resume_repl: bool,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(report) => {
                self.add_log(LogKind::Ok, format!("Detected {}", report.summary()));
                let selected = self
                    .selected_port_identity()
                    .filter(|port| crate::hardware::device_key(port) == key);
                self.chip_probes.insert(key, ChipProbe::Found(report));
                // The chip decides which runtimes exist, so the profile set
                // has to be rebuilt rather than merely relabelled.
                if let Some(port) = selected {
                    self.rebuild_profiles_for(&port);
                }
            }
            Err(esp::ProbeError::NoAnswer(_)) => {
                if deliberate {
                    self.add_log(
                        LogKind::Warn,
                        "No Espressif chip answered on this port. Choose the board model by hand.",
                    );
                }
                self.chip_probes.insert(key, ChipProbe::Silent);
            }
            Err(error) => {
                if deliberate {
                    self.add_log(LogKind::Warn, error.message());
                }
                self.chip_probes
                    .insert(key, ChipProbe::Failed(error.message()));
            }
        }
        if resume_repl {
            self.repl_connect();
        }
        cx.notify();
    }

    /// The board the active profile targets, falling back to USB detection.
    pub fn active_board(&self) -> Option<&'static BoardDef> {
        self.active_profile
            .as_ref()
            .and_then(ExecutionProfile::board)
            .or_else(|| self.resolved_board().board)
    }

    pub fn active_runtime(&self) -> Runtime {
        self.active_profile
            .as_ref()
            .map(|profile| profile.runtime)
            .or_else(|| self.project_kind.runtime())
            .unwrap_or(Runtime::MicroPython)
    }

    pub fn workspace_layout(&self) -> WorkspaceLayout {
        profiles::workspace_layout(self.active_runtime())
    }

    pub fn is_connected(&self) -> bool {
        !self.selected_port.is_empty()
            && !self.connection_dropped
            && self
                .ports
                .iter()
                .any(|port| port.path == self.selected_port)
    }

    pub fn detected_board_label(&self) -> Option<String> {
        self.active_board().map(|board| board.name.to_string())
    }

    /// The FQBN to compile with: the user's override, then the catalog's.
    pub fn fqbn_for(&self, board: &BoardDef) -> Option<String> {
        self.settings
            .fqbn_overrides
            .get(board.id)
            .cloned()
            .or_else(|| board.fqbn.map(str::to_string))
    }

    // -- Toolchain -----------------------------------------------------------

    /// Probe every tool off the UI thread and cache the result.
    pub fn refresh_tools(&mut self, cx: &mut Context<Self>) {
        if self.tools_scanning {
            return;
        }
        self.tools_scanning = true;
        self._tools_task = Some(cx.spawn(async move |this, cx| {
            let statuses = cx
                .background_spawn(async { toolchain::probe_all(Tool::ALL) })
                .await;
            _ = this.update(cx, |this, cx| {
                this.tool_statuses = statuses;
                this.tools_scanning = false;
                cx.notify();
            });
        }));
    }

    pub fn tool_status(&self, tool: Tool) -> Option<&ToolStatus> {
        self.tool_statuses.iter().find(|status| status.tool == tool)
    }

    pub fn is_tool_available(&self, tool: Tool) -> bool {
        self.tool_status(tool).is_some_and(ToolStatus::is_available)
    }

    /// The tools the current board and runtime need, and whether each is
    /// present. Empty when no board is selected.
    pub fn required_tools(&self) -> Vec<(Tool, bool)> {
        let Some(board) = self.active_board() else {
            return Vec::new();
        };
        toolchain::required_tools(board, self.active_runtime())
            .into_iter()
            .map(|tool| (tool, self.is_tool_available(tool)))
            .collect()
    }

    /// The tools that are needed but missing, which is what blocks a build.
    pub fn missing_tools(&self) -> Vec<Tool> {
        self.required_tools()
            .into_iter()
            .filter(|(_, available)| !available)
            .map(|(tool, _)| tool)
            .collect()
    }

    /// Install a tool. `drain_background_events` re-probes once the job ends,
    /// so the toolchain list reflects the new state without another command.
    pub fn install_tool(&mut self, tool: Tool) {
        self.add_log(LogKind::Install, format!("Installing {}…", tool.name()));
        self.spawn_device_work(move |emit| toolchain::install(tool, emit));
    }

    /// Install the `arduino-cli` core the active board compiles with.
    pub fn install_board_core(&mut self) {
        let Some(board) = self.active_board() else {
            self.add_log(LogKind::Warn, "Select a board first");
            return;
        };
        if board.core.is_none() {
            self.add_log(
                LogKind::Warn,
                format!("{} has no Arduino core in Anode", board.name),
            );
            return;
        }
        self.add_log(
            LogKind::Install,
            format!("Installing the Arduino core for {}…", board.name),
        );
        self.spawn_device_work(move |emit| device::arduino_core_install(board, emit));
    }

    // -- Logging -------------------------------------------------------------

    pub fn add_log(&mut self, kind: LogKind, message: impl Into<SharedString>) {
        self.log_entries.push(LogEntry {
            id: self.next_log_id,
            kind,
            time: chrono::Local::now().format("%H:%M:%S").to_string().into(),
            message: message.into(),
            project: self.project_path.clone(),
        });
        // Bound the log so a chatty flash session can't grow it forever.
        if self.log_entries.len() > 2000 {
            let excess = self.log_entries.len() - 2000;
            self.log_entries.drain(..excess);
        }
        self.next_log_id += 1;
    }

    pub fn clear_log(&mut self) {
        self.log_entries.clear();
        self.device_recovery = None;
    }

    pub fn set_log_filter(&mut self, filter: LogFilter) {
        self.log_filter = filter;
    }

    /// The entries the current filter shows.
    pub fn visible_log_entries(&self) -> Vec<LogEntry> {
        match self.log_filter {
            LogFilter::All => self.log_entries.clone(),
            LogFilter::Problems => self
                .log_entries
                .iter()
                .filter(|entry| entry.kind.is_problem())
                .cloned()
                .collect(),
        }
    }

    /// The whole log as text, for the Copy button.
    pub fn log_as_text(&self) -> String {
        self.visible_log_entries()
            .iter()
            .map(|entry| format!("{} {}", entry.time, entry.message))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Classify a progress line from a device worker. Workers mark their own
    /// lines with a prefix; anything else is ordinary tool output.
    fn log_from_progress(&mut self, message: &str) {
        let (kind, message) = if let Some(rest) = message.strip_prefix("[ERROR]") {
            (LogKind::Error, rest.trim())
        } else if let Some(rest) = message.strip_prefix("[WARN]") {
            (LogKind::Warn, rest.trim())
        } else if let Some(rest) = message.strip_prefix("[OK]") {
            (LogKind::Ok, rest.trim())
        } else if looks_like_an_error(message) {
            (LogKind::Error, message)
        } else {
            (LogKind::Flash, message)
        };
        self.add_log(kind, message.to_string());
    }

    // -- Project -------------------------------------------------------------

    pub fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_project_from_drain(path, cx);
        self.sync_editor_to_current_file(window, cx);
    }

    // -- Screens and panel visibility ---------------------------------------

    /// Open Settings as a full-window destination, re-checking the agents it
    /// shows as it opens.
    pub(crate) fn show_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Settings;
        if let Some(screen) = self.settings_screen() {
            screen.update(cx, |screen, cx| screen.show_providers(window, cx));
        }
        cx.notify();
    }

    /// Leave Settings and return to the workspace.
    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Workspace;
        cx.notify();
    }

    /// Whether this tool is the one its dock region currently shows.
    pub fn is_tool_visible(&self, tool: crate::panels::Tool) -> bool {
        self.visible_panels
            .get(&tool.group())
            .is_some_and(|choice| *choice == crate::panels::PanelChoice::Tool(tool))
    }

    pub(crate) fn is_board_panel_visible(&self, key: &str) -> bool {
        self.visible_panels
            .get(&crate::panels::ToolGroup::Inspector)
            .is_some_and(|choice| match choice {
                crate::panels::PanelChoice::Board(visible) => visible == key,
                _ => false,
            })
    }

    /// Show one panel in its group, hiding whatever that group showed before.
    pub(crate) fn select_panel(&mut self, choice: crate::panels::PanelChoice) {
        self.visible_panels.insert(choice.group(), choice);
    }

    /// The display name of the agent chat is using, if the user chose one.
    /// `None` means chat is still waiting for a choice.
    pub fn selected_agent(&self) -> Option<String> {
        self.agents
            .selected_id()
            .map(|id| self.agents.provider_name(id))
    }

    /// Every configured agent's display name, in the order Settings lists them.
    pub fn configured_agents(&self) -> Vec<String> {
        self.agents
            .registry()
            .all()
            .iter()
            .map(|profile| profile.name.clone())
            .collect()
    }

    pub(crate) fn open_project_preserving_agent(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_project_inner(path, true, cx);
    }

    /// Load a project without window access; the editor syncs on the next
    /// render via `needs_editor_sync`.
    fn open_project_from_drain(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_project_inner(path, false, cx);
    }

    fn open_project_inner(&mut self, path: PathBuf, preserve_agent: bool, cx: &mut Context<Self>) {
        match project::load_project(&path) {
            Ok(files) => {
                if preserve_agent {
                    if let Ok(canonical) = path.canonicalize() {
                        self.agents.bind_preserving_active(canonical);
                    }
                } else {
                    self.agents.bind(path.canonicalize().ok());
                }
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Project".into());
                let kind = project::detect_project_kind(&path);
                self.add_log(LogKind::Ok, format!("Opened {name} · {}", kind.label()));
                self.settings
                    .add_recent_project(path.to_string_lossy().to_string(), name.clone());
                self.settings.save();
                self.terminal
                    .update(cx, |terminal, cx| terminal.open_directory(Some(&path), cx));
                self.platformio_environments = if kind == ProjectKind::PlatformIo {
                    device::platformio_environments(&path)
                } else {
                    Vec::new()
                };
                self.project_path = Some(path);
                self.project_name = name;
                self.project_kind = kind;
                self.project_files = files;
                self.reset_agent_file_watch(cx);
                self.open_tabs.clear();
                self.active_tab = None;
                // Open the file the project is about, not merely the first one
                // alphabetically.
                self.current_file = pick_entry_file(&self.project_files, kind);
                self.is_dirty = false;
                if let Some(file) = self.current_file.clone() {
                    self.open_tab_for(file);
                }
                // A project's own kind is a stronger signal than a USB guess.
                self.adopt_project_runtime();
                self.needs_editor_sync = true;
            }
            Err(error) => self.add_log(
                LogKind::Error,
                format!("Could not open that folder: {error}"),
            ),
        }
    }

    /// Switch the active runtime to the one the opened project is written
    /// for, when the board supports it and the user has not pinned a choice.
    fn adopt_project_runtime(&mut self) {
        let Some(runtime) = self.project_kind.runtime() else {
            return;
        };
        if self.profile_manually_set {
            return;
        }
        if let Some(profile) = self
            .available_profiles
            .iter()
            .find(|profile| profile.runtime == runtime)
            .cloned()
        {
            self.settings.last_profile_id = profile.id.clone();
            self.settings.save();
            self.active_profile = Some(profile);
        }
    }

    /// Open a project from a recent-projects entry.
    pub fn open_recent_project(&mut self, path: &str) {
        let path = PathBuf::from(path);
        if path.is_dir() {
            self.pending_open_path = Some(path);
        } else {
            self.add_log(
                LogKind::Error,
                format!("That folder no longer exists: {}", path.display()),
            );
        }
    }

    pub fn reload_project(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.project_path.clone() {
            self.pending_open_path = Some(path);
            cx.notify();
        }
    }

    fn update_file_content(&mut self, path: &PathBuf, content: String) {
        for file in &mut self.project_files {
            if &file.path == path {
                file.content = content.clone();
                break;
            }
        }
        if let Some(current) = self.current_file.as_mut() {
            if &current.path == path {
                current.content = content;
            }
        }
        self.is_dirty = true;
        for tab in &mut self.open_tabs {
            if &tab.path == path {
                tab.is_dirty = true;
            }
        }
    }

    fn open_tab_for(&mut self, file: ProjectFile) {
        if !self.open_tabs.iter().any(|tab| tab.path == file.path) {
            self.open_tabs.push(OpenTab {
                path: file.path.clone(),
                name: file.name.clone(),
                is_dirty: false,
            });
        }
        self.active_tab = Some(file.path.clone());
        self.current_file = Some(file);
    }

    pub fn select_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(file) = self
            .project_files
            .iter()
            .find(|file| file.path == path)
            .cloned()
        {
            self.open_tab_for(file);
            self.sync_editor_to_current_file(window, cx);
        }
    }

    pub fn select_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.open_tabs.get(index) {
            let path = tab.path.clone();
            self.select_file(path, window, cx);
        }
    }

    pub fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.open_tabs.get(index) else {
            return;
        };
        let path = tab.path.clone();
        self.open_tabs.retain(|tab| tab.path != path);
        if self.active_tab.as_ref() == Some(&path) {
            self.active_tab = self.open_tabs.last().map(|tab| tab.path.clone());
            self.current_file = self.active_tab.as_ref().and_then(|path| {
                self.project_files
                    .iter()
                    .find(|file| &file.path == path)
                    .cloned()
            });
            self.sync_editor_to_current_file(window, cx);
        }
    }

    /// Point the editor entity at the current file. Requires window access for
    /// `set_value`; callers pass the window they already hold.
    fn sync_editor_to_current_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_syncing = true;
        let content = self
            .current_file
            .as_ref()
            .map(|file| file.content.clone())
            .unwrap_or_default();
        let highlighter = self
            .current_file
            .as_ref()
            .map(|file| file.language)
            .unwrap_or(Language::Python)
            .highlighter();
        self.editor_file = self.current_file.as_ref().map(|file| file.path.clone());
        self.editor.update(cx, |state, cx| {
            state.set_value(content, window, cx);
            state.set_highlighter(highlighter, cx);
        });
        self.editor_syncing = false;
    }

    // -- Save ----------------------------------------------------------------

    pub fn save_current_file(&mut self) {
        let Some(file) = self.current_file.clone() else {
            self.add_log(LogKind::Warn, "No file is open");
            return;
        };
        if self.save_file(&file, false) {
            self.is_dirty = false;
            for tab in &mut self.open_tabs {
                if tab.path == file.path {
                    tab.is_dirty = false;
                }
            }
        }
    }

    fn save_file(&mut self, file: &ProjectFile, silent: bool) -> bool {
        match project::write_file(&file.path, &file.content) {
            Ok(()) => {
                if !silent {
                    self.add_log(LogKind::Ok, format!("Saved {}", file.name));
                }
                true
            }
            Err(error) => {
                self.add_log(
                    LogKind::Error,
                    format!("Could not save {}: {error}", file.name),
                );
                false
            }
        }
    }

    pub fn save_all_files(&mut self) {
        let dirty: Vec<ProjectFile> = self
            .project_files
            .iter()
            .filter(|file| {
                self.open_tabs
                    .iter()
                    .any(|tab| tab.path == file.path && tab.is_dirty)
            })
            .cloned()
            .collect();
        if dirty.is_empty() {
            return;
        }
        for file in &dirty {
            if !self.save_file(file, true) {
                return;
            }
        }
        self.is_dirty = false;
        for tab in &mut self.open_tabs {
            tab.is_dirty = false;
        }
        self.add_log(LogKind::Ok, format!("Saved {} files", dirty.len()));
    }

    // -- Run / build / upload ------------------------------------------------

    /// Run the open file on the board. Only the interpreted runtimes can do
    /// this; the compiled ones build instead.
    pub fn run_code(&mut self) {
        if !self.active_runtime().is_interpreted() {
            self.build_project();
            return;
        }
        let Some(file) = self.current_file.clone() else {
            self.add_log(LogKind::Warn, "No file is open");
            return;
        };
        if file.language != Language::Python {
            self.add_log(LogKind::Warn, format!("{} is not a Python file", file.name));
            return;
        }

        // CircuitPython deploys to the drive instead of running over serial.
        // The board auto-reloads and runs code.py; output reaches the REPL.
        if self.active_runtime() == Runtime::CircuitPython {
            self.deploy_circuit_python(vec![file]);
            return;
        }

        let Some(port) = self.require_port() else {
            return;
        };
        if !self.save_file(&file, true) {
            return;
        }
        self.add_log(LogKind::Info, format!("Running {} on {port}", file.name));
        self.spawn_device_work(move |emit| device::run_code(&port, &file.content, emit));
    }

    /// Compile the project without touching the board.
    pub fn build_project(&mut self) {
        let runtime = self.active_runtime();
        if runtime.is_interpreted() {
            self.add_log(
                LogKind::Warn,
                format!(
                    "{} runs source directly; there is nothing to build",
                    runtime.label()
                ),
            );
            return;
        }
        let Some(project_path) = self.project_path.clone() else {
            self.add_log(LogKind::Warn, "Open a project folder first");
            return;
        };
        if let Some(missing) = self.first_missing_tool() {
            return self.report_missing_tool(missing);
        }
        self.save_all_files();

        match (runtime, self.project_kind) {
            (Runtime::Arduino, _) => {
                let Some(fqbn) = self.require_fqbn() else {
                    return;
                };
                self.add_log(LogKind::Info, format!("Compiling for {fqbn}"));
                self.spawn_build_work(move |emit| {
                    device::arduino_compile(&project_path, &fqbn, emit)
                });
            }
            (Runtime::Native, ProjectKind::EspIdf) => {
                let Some(target) = self.require_idf_target() else {
                    return;
                };
                self.add_log(LogKind::Info, format!("Building with ESP-IDF for {target}"));
                self.spawn_build_work(move |emit| device::idf_build(&project_path, &target, emit));
            }
            (Runtime::Native, _) => {
                let environment = self.platformio_environments.first().cloned();
                self.add_log(LogKind::Info, "Building with PlatformIO");
                self.spawn_build_work(move |emit| {
                    device::platformio_build(&project_path, environment.as_deref(), emit)
                });
            }
            _ => {}
        }
    }

    /// Put the project on the board, whichever runtime it uses.
    pub fn upload_project(&mut self) {
        match self.active_runtime() {
            Runtime::CircuitPython => {
                let files: Vec<ProjectFile> = self
                    .project_files
                    .iter()
                    .filter(|file| file.language.is_device_source())
                    .cloned()
                    .collect();
                if files.is_empty() {
                    self.add_log(LogKind::Warn, "This project has no Python files");
                    return;
                }
                self.deploy_circuit_python(files);
            }
            Runtime::MicroPython => self.upload_micropython(),
            Runtime::Arduino => self.upload_arduino(),
            Runtime::Native => self.upload_native(),
        }
    }

    fn upload_micropython(&mut self) {
        let files: Vec<ProjectFile> = self
            .project_files
            .iter()
            .filter(|file| file.language.is_device_source())
            .cloned()
            .collect();
        if files.is_empty() {
            self.add_log(LogKind::Warn, "This project has no Python files");
            return;
        }
        let Some(port) = self.require_port() else {
            return;
        };
        if let Some(missing) = self.first_missing_tool() {
            return self.report_missing_tool(missing);
        }
        self.save_all_files();
        let total = files.len();
        self.add_log(LogKind::Info, format!("Uploading {total} files to {port}"));
        self.spawn_device_work(move |emit| {
            for file in &files {
                device::upload_code(&port, &file.name, &file.content, emit)?;
            }
            emit(DeviceEvent::Progress(format!(
                "[OK] {total} files are on the board"
            )));
            Ok(())
        });
    }

    fn upload_arduino(&mut self) {
        let Some(project_path) = self.project_path.clone() else {
            self.add_log(LogKind::Warn, "Open a sketch folder first");
            return;
        };
        let Some(port) = self.require_port() else {
            return;
        };
        if let Some(missing) = self.first_missing_tool() {
            return self.report_missing_tool(missing);
        }
        let Some(fqbn) = self.require_fqbn() else {
            return;
        };
        self.save_all_files();
        self.add_log(LogKind::Info, format!("Uploading to {port} for {fqbn}"));
        self.spawn_device_work(move |emit| {
            device::arduino_upload(&port, &project_path, &fqbn, emit)
        });
    }

    fn upload_native(&mut self) {
        let Some(project_path) = self.project_path.clone() else {
            self.add_log(LogKind::Warn, "Open a project folder first");
            return;
        };
        let Some(port) = self.require_port() else {
            return;
        };
        if let Some(missing) = self.first_missing_tool() {
            return self.report_missing_tool(missing);
        }
        self.save_all_files();
        if self.project_kind == ProjectKind::EspIdf {
            let Some(target) = self.require_idf_target() else {
                return;
            };
            self.add_log(LogKind::Info, format!("Flashing {target} with ESP-IDF"));
            self.spawn_device_work(move |emit| {
                device::idf_flash(&project_path, &target, &port, emit)
            });
            return;
        }
        let environment = self.platformio_environments.first().cloned();
        self.add_log(
            LogKind::Info,
            format!("Uploading to {port} with PlatformIO"),
        );
        self.spawn_device_work(move |emit| {
            device::platformio_upload(&project_path, environment.as_deref(), &port, emit)
        });
    }

    fn deploy_circuit_python(&mut self, files: Vec<ProjectFile>) {
        let drive = self.circuit_python_drive.clone().unwrap_or_default();
        if !drive.present {
            self.add_log(
                LogKind::Warn,
                "No CIRCUITPY drive is mounted. Reconnect the board, or flash CircuitPython first.",
            );
            return;
        }
        self.save_all_files();
        let total = files.len();
        self.add_log(LogKind::Info, format!("Copying {total} files to CIRCUITPY"));
        self.spawn_device_work(move |emit| {
            for file in &files {
                let written = device::circuitpython_deploy(&drive, &file.name, &file.content)?;
                emit(DeviceEvent::Progress(format!("Copied {written}")));
            }
            emit(DeviceEvent::Progress(format!(
                "[OK] {total} files copied; the board restarts code.py on its own"
            )));
            Ok(())
        });
    }

    pub fn stop_code(&mut self) {
        let Some(port) = self.require_port() else {
            return;
        };
        self.add_log(LogKind::Warn, "Stopping the running program");
        self.spawn_device_work(move |emit| device::stop_code(&port, emit));
    }

    /// Where the running device job has got to, when its output said.
    pub fn flash_phase(&self) -> Option<esp::Progress> {
        self.device_progress.clone()
    }

    /// Everything a firmware write needs for the active board, including what
    /// the chip itself reported. `None` when no board is resolved yet.
    pub fn flash_request(&self, source: FirmwareSource) -> Option<device::FlashRequest> {
        let board = self.active_board()?;
        let port = self.selected_port_identity();
        Some(device::FlashRequest {
            port_path: self.selected_port.clone(),
            board,
            source,
            report: port
                .as_ref()
                .and_then(|port| self.chip_report_for(port))
                .cloned(),
            native_usb: port.as_ref().is_some_and(esp::is_native_usb),
        })
    }

    /// Flash firmware onto the active board.
    ///
    /// The REPL holds the same serial port, so a flash takes it away and puts
    /// it back rather than refusing until the user disconnects by hand.
    pub fn flash_firmware(&mut self, source: FirmwareSource) {
        let Some(request) = self.flash_request(source) else {
            self.add_log(LogKind::Warn, "Select a board first");
            return;
        };
        if self.repl_connected {
            self.repl_disconnect();
            self.repl_resumes_after_work = true;
        }
        let runtime = self.active_runtime();
        let port = request.port_path.clone();
        self.add_log(
            LogKind::Info,
            format!(
                "Flashing {} · {}",
                request.board.name,
                request.image_label()
            ),
        );
        self.spawn_device_work(move |emit| {
            let outcome = device::flash_firmware(request, emit);
            if !outcome.success {
                return Err(outcome.message());
            }
            // A board that writes correctly and then does not come back is
            // still a failed flash from where the user sits, so say what
            // happened next rather than stopping at "written".
            device::settle_after_flash(&port, runtime, emit);
            Ok(())
        });
    }

    // -- Preconditions -------------------------------------------------------

    /// The selected port, or a log line saying why there isn't one.
    fn require_port(&mut self) -> Option<String> {
        if self.selected_port.is_empty() {
            self.add_log(LogKind::Warn, "Connect a board over USB first");
            return None;
        }
        Some(self.selected_port.clone())
    }

    fn require_fqbn(&mut self) -> Option<String> {
        let board = self.active_board()?;
        match self.fqbn_for(board) {
            Some(fqbn) => Some(fqbn),
            None => {
                self.add_log(
                    LogKind::Error,
                    format!(
                        "{} has no Arduino board name in Anode. Set one in the board panel.",
                        board.name
                    ),
                );
                None
            }
        }
    }

    fn require_idf_target(&mut self) -> Option<String> {
        let board = self.active_board();
        match board.and_then(|board| board.idf_target()) {
            Some(target) => Some(target.to_string()),
            None => {
                self.add_log(
                    LogKind::Error,
                    "ESP-IDF only builds for Espressif chips. Select an Espressif board, or build with PlatformIO.",
                );
                None
            }
        }
    }

    fn first_missing_tool(&self) -> Option<Tool> {
        // ESP-IDF and PlatformIO are alternatives for a native build, so a
        // missing one is only fatal when the other is missing too.
        let missing = self.missing_tools();
        if self.active_runtime() == Runtime::Native {
            let has_either =
                self.is_tool_available(Tool::PlatformIo) || self.is_tool_available(Tool::EspIdf);
            if has_either {
                return None;
            }
            return missing.into_iter().next();
        }
        missing.into_iter().next()
    }

    fn report_missing_tool(&mut self, tool: Tool) {
        self.add_log(
            LogKind::Error,
            format!("{} is not installed. {}", tool.name(), tool.install_hint()),
        );
    }

    /// Run device I/O on a background thread; its progress lines and result
    /// are drained back onto the UI thread by `drain_background_events`.
    fn spawn_device_work(
        &mut self,
        work: impl FnOnce(&mut dyn FnMut(DeviceEvent)) -> Result<(), String> + Send + 'static,
    ) {
        self.spawn_work(true, work);
    }

    fn spawn_build_work(
        &mut self,
        work: impl FnOnce(&mut dyn FnMut(DeviceEvent)) -> Result<(), String> + Send + 'static,
    ) {
        // Compilation uses saved project files and must not take the REPL's serial lease.
        self.spawn_work(false, work);
    }

    fn spawn_work(
        &mut self,
        uses_port: bool,
        work: impl FnOnce(&mut dyn FnMut(DeviceEvent)) -> Result<(), String> + Send + 'static,
    ) {
        if self.device_busy {
            self.add_log(LogKind::Warn, "Another device operation is still running");
            return;
        }
        let key = self
            .selected_port_identity()
            .map(|port| crate::hardware::device_key(&port))
            .unwrap_or_else(|| self.selected_port.replace("/dev/tty.", "/dev/cu."));
        let lease = if !uses_port || key.is_empty() {
            None
        } else {
            match self.port_leases.acquire(&key) {
                Ok(lease) => Some(lease),
                Err(error) => {
                    self.device_recovery =
                        crate::recovery::RecoveryIssue::from_error(self.recovery_context(), &error);
                    self.add_log(LogKind::Warn, error);
                    return;
                }
            }
        };
        let (sender, receiver) = std::sync::mpsc::channel::<BackgroundOutcome>();
        self.device_events = Some(receiver);
        self.device_work_project = self.project_path.clone();
        self.device_busy = true;
        let recovery_context = uses_port.then(|| self.recovery_context());
        std::thread::spawn(move || {
            let _lease = lease;
            let mut sink = |event: DeviceEvent| {
                let DeviceEvent::Progress(message) = event;
                let _ = sender.send(BackgroundOutcome::Progress(message));
            };
            let result = work(&mut sink);
            let _ = sender.send(BackgroundOutcome::Finished(result, recovery_context));
        });
    }

    /// Deliver background progress and results to the log. Called from render
    /// and from the poll tick, so results arrive even between user actions.
    pub fn drain_background_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(path) = self.pending_open_path.take() {
            self.open_project_from_drain(path, cx);
            self.sync_editor_to_current_file(window, cx);
            changed = true;
        }
        if self.needs_editor_sync {
            self.needs_editor_sync = false;
            self.sync_editor_to_current_file(window, cx);
            changed = true;
        }
        let mut finished = false;
        while self.device_events.is_some() {
            let Some(receiver) = &self.device_events else {
                break;
            };
            let Ok(event) = receiver.try_recv() else {
                break;
            };
            match event {
                BackgroundOutcome::Progress(message) => {
                    // A line that says nothing about where the work has got to
                    // leaves the indicator where it was, rather than flickering
                    // back to an unknown state.
                    if let Some(progress) = esp::progress_from_line(&message) {
                        self.device_progress = Some(progress);
                    }
                    self.log_from_progress(&message);
                    if let Some(entry) = self.log_entries.last_mut() {
                        entry.project = self.device_work_project.clone();
                    }
                    changed = true;
                }
                BackgroundOutcome::Finished(result, context) => {
                    self.agent_build_finished(result.clone());
                    if let Some(context) = context {
                        if let Err(error) = &result {
                            self.device_recovery =
                                crate::recovery::RecoveryIssue::from_error(context, error);
                        } else if self.device_recovery.as_ref().is_some_and(|issue| {
                            issue.context.port == context.port
                                && issue.context.runtime == context.runtime
                        }) {
                            self.device_recovery = None;
                        }
                    }
                    let first_error = self.next_log_id;
                    if let Err(error) = result {
                        for line in error.lines().filter(|line| !line.trim().is_empty()) {
                            self.add_log(LogKind::Error, line.to_string());
                        }
                    }
                    for entry in self
                        .log_entries
                        .iter_mut()
                        .filter(|entry| entry.id >= first_error)
                    {
                        entry.project = self.device_work_project.clone();
                    }
                    self.device_work_project = None;
                    self.device_busy = false;
                    self.device_progress = None;
                    self.device_events = None;
                    finished = true;
                    changed = true;
                    break;
                }
            }
        }
        if finished {
            // An install or a core download changes what is available, and a
            // re-probe is cheap next to the job that just ran.
            self.refresh_tools(cx);
            if std::mem::take(&mut self.repl_resumes_after_work) {
                self.repl_connect();
            }
        }
        if self.device_busy {
            if self._device_poll.is_none() {
                self._device_poll = Some(cx.spawn(async move |this, cx| loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(80))
                        .await;
                    let still_running = this
                        .update(cx, |this, cx| {
                            cx.notify();
                            this.device_busy
                        })
                        .unwrap_or(false);
                    if !still_running {
                        break;
                    }
                }));
            }
        } else {
            self._device_poll = None;
        }
        if changed {
            cx.notify();
        }
    }

    /// Open the native folder picker and load the chosen directory.
    pub fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn(async move |this, cx: &mut gpui_kit::AsyncApp| {
            let picked = paths
                .await
                .ok()
                .and_then(|result| result.ok())
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            if let Some(path) = picked {
                _ = this.update(cx, |this, cx| {
                    this.pending_open_path = Some(path);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Ask for a firmware image on disk and flash it to the active board.
    pub fn pick_firmware_file(&mut self, cx: &mut Context<Self>) {
        if self.active_board().is_none() {
            self.add_log(LogKind::Warn, "Select a board first");
            return;
        }
        let paths = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Flash".into()),
        });
        cx.spawn(async move |this, cx: &mut gpui_kit::AsyncApp| {
            let picked = paths
                .await
                .ok()
                .and_then(|result| result.ok())
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            if let Some(path) = picked {
                _ = this.update(cx, |this, cx| {
                    this.flash_firmware(FirmwareSource::LocalFile(path));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Create a project from a template under ~/Documents/Anode, then open it.
    pub fn create_from_template(&mut self, template_id: &str, cx: &mut Context<Self>) {
        let Some(template) = project::TEMPLATES
            .iter()
            .find(|template| template.id == template_id)
        else {
            return;
        };
        let base = dirs::document_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Anode");
        let project_dir = unique_directory(&base, &template.directory_name());

        match project::create_from_template(&project_dir, template) {
            Ok(_) => {
                self.add_log(
                    LogKind::Ok,
                    format!("Created {} from {}", project_dir.display(), template.name),
                );
                self.pending_open_path = Some(project_dir);
            }
            Err(error) => self.add_log(
                LogKind::Error,
                format!("Could not create the project: {error}"),
            ),
        }
        cx.notify();
    }

    // -- REPL ----------------------------------------------------------------

    pub fn repl_connect(&mut self) {
        let Some(port) = self.require_port() else {
            return;
        };
        if self.repl_connected {
            return;
        }
        let key = self
            .selected_port_identity()
            .map(|port| crate::hardware::device_key(&port))
            .unwrap_or_else(|| port.replace("/dev/tty.", "/dev/cu."));
        let lease = match self.port_leases.acquire(&key) {
            Ok(lease) => lease,
            Err(error) => {
                self.device_recovery =
                    crate::recovery::RecoveryIssue::from_error(self.recovery_context(), &error);
                self.add_log(LogKind::Warn, error);
                return;
            }
        };
        self.repl_output.clear();
        match ReplConnection::open_exclusive(&port, lease) {
            Ok((connection, events)) => {
                self.repl_connection = Some(connection);
                self.repl_events = Some(events);
                self.repl_connected = true;
                self.repl_output
                    .push_str(&format!("--- Connected to {port} ---\n"));
                self.bottom_mode = BottomMode::Repl;
            }
            Err(error) => {
                self.device_recovery =
                    crate::recovery::RecoveryIssue::from_error(self.recovery_context(), &error);
                self.repl_output
                    .push_str(&format!("[ERROR] Could not connect: {error}\n"));
                self.add_log(LogKind::Error, format!("REPL: {error}"));
            }
        }
    }

    pub fn repl_disconnect(&mut self) {
        self.repl_connection = None;
        self.repl_events = None;
        self.repl_connected = false;
        self.repl_output.push_str("\n--- Disconnected ---\n");
    }

    fn send_repl_line(&mut self, line: String) {
        if !self.repl_connected {
            return;
        }
        if !line.trim().is_empty() {
            // Keep one copy of a repeated command, at the end where Up finds
            // it first.
            self.repl_history.retain(|entry| entry != &line);
            self.repl_history.push(line.clone());
            self.repl_history.truncate(200);
        }
        self.repl_history_cursor = None;
        if let Some(connection) = &self.repl_connection {
            connection.send(format!("{line}\r\n").as_bytes());
        }
    }

    /// Step through REPL history, as a terminal's Up and Down keys do.
    ///
    /// Returns the line the input should now show, or `None` when the cursor
    /// cannot move — already at the oldest entry, or already back at the line
    /// being typed. Stepping past the newest entry restores the empty line.
    pub fn repl_history_step(&mut self, direction: HistoryStep) -> Option<String> {
        let next = step_history(self.repl_history.len(), self.repl_history_cursor, direction)?;
        self.repl_history_cursor = next;
        Some(match next {
            Some(index) => self.repl_history[index].clone(),
            // Past the newest entry: back to the empty line.
            None => String::new(),
        })
    }

    pub fn repl_break(&mut self) {
        if let Some(connection) = &self.repl_connection {
            connection.send(&[repl::controls::BREAK]);
            self.repl_output.push_str("^C");
        }
    }

    pub fn repl_soft_reset(&mut self) {
        if let Some(connection) = &self.repl_connection {
            connection.send(&[repl::controls::SOFT_RESET]);
            self.repl_output.push_str("\n[Soft reset]\n");
        }
    }

    pub fn repl_send_raw(&mut self, data: String) {
        self.send_repl_line(data);
    }

    /// Forward any REPL bytes that arrived since the last frame into the
    /// terminal buffer. Called from render.
    pub fn drain_repl_events(&mut self) {
        while let Some(receiver) = &self.repl_events {
            match receiver.try_recv() {
                Ok(ReplEvent::Data(text)) => self.repl_output.push_str(&text),
                Ok(ReplEvent::Disconnected) => {
                    self.repl_connection = None;
                    self.repl_events = None;
                    self.repl_connected = false;
                    self.repl_output.push_str("\n--- Disconnected ---\n");
                    break;
                }
                Ok(ReplEvent::Error(error)) => {
                    if let Some(issue) =
                        crate::recovery::RecoveryIssue::from_error(self.recovery_context(), &error)
                    {
                        self.device_recovery = Some(issue);
                    }
                    self.repl_output.push_str(&format!("\n[ERROR] {error}\n"));
                }
                Err(_) => break,
            }
        }
        // The REPL view scrolls its own text; keep the buffer bounded so a
        // chatty board cannot grow it without limit.
        const MAX_REPL_BYTES: usize = 256 * 1024;
        if self.repl_output.len() > MAX_REPL_BYTES {
            let mut start = self.repl_output.len() - MAX_REPL_BYTES;
            while !self.repl_output.is_char_boundary(start) {
                start += 1;
            }
            self.repl_output.drain(..start);
        }
    }

    // -- Recent projects -----------------------------------------------------

    pub fn recent_projects(&self) -> &[crate::settings::RecentProject] {
        &self.settings.recent_projects
    }

    pub fn clear_recent_projects(&mut self) {
        self.settings.recent_projects.clear();
        self.settings.save();
    }

    pub fn is_profile_pinned(&self, profile: &ExecutionProfile) -> bool {
        profile.confidence == Confidence::High && !profile.is_generic
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Where the history cursor lands after one key press.
///
/// The outer `Option` says whether the cursor moved at all; the inner one is
/// the new position, where `None` means the line being typed.
fn step_history(
    len: usize,
    cursor: Option<usize>,
    direction: HistoryStep,
) -> Option<Option<usize>> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    Some(match (cursor, direction) {
        (None, HistoryStep::Older) => Some(last),
        // Down from the line being typed has nowhere to go.
        (None, HistoryStep::Newer) => return None,
        // Up at the oldest entry stays there rather than clearing the input.
        (Some(0), HistoryStep::Older) => return None,
        (Some(index), HistoryStep::Older) => Some(index - 1),
        (Some(index), HistoryStep::Newer) if index >= last => None,
        (Some(index), HistoryStep::Newer) => Some(index + 1),
    })
}

/// Tool output rarely marks its own severity, so recognise the shapes that
/// always mean a failure. Anything less certain stays ordinary output.
fn looks_like_an_error(message: &str) -> bool {
    const MARKERS: &[&str] = &[
        "error:",
        "fatal error",
        "Traceback (most recent call last)",
        "command not found",
        "Permission denied",
    ];
    MARKERS.iter().any(|marker| message.contains(marker))
}

/// The file to open first: the entry point for the project's kind, else the
/// first source file, else the first file at all.
fn pick_entry_file(files: &[ProjectFile], kind: ProjectKind) -> Option<ProjectFile> {
    let preferred: &[&str] = match kind {
        ProjectKind::CircuitPython => &["code.py", "main.py"],
        ProjectKind::MicroPython => &["main.py", "boot.py"],
        ProjectKind::EspIdf => &["main/main.c", "main/main.cpp"],
        ProjectKind::PlatformIo => &["src/main.cpp", "src/main.c", "src/main.ino"],
        ProjectKind::ArduinoSketch | ProjectKind::Unknown => &[],
    };
    for name in preferred {
        if let Some(file) = files.iter().find(|file| file.relative == *name) {
            return Some(file.clone());
        }
    }
    files
        .iter()
        .find(|file| {
            matches!(
                file.language,
                Language::Python | Language::Arduino | Language::C | Language::Cpp
            )
        })
        .or_else(|| files.first())
        .cloned()
}

/// `base/name`, or `base/name-2` when that is taken, so creating the same
/// template twice does not overwrite the first project.
fn unique_directory(base: &std::path::Path, name: &str) -> PathBuf {
    let first = base.join(name);
    if !first.exists() {
        return first;
    }
    (2..100)
        .map(|suffix| base.join(format!("{name}-{suffix}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

/// List serial ports as `PortIdentity`s. Runs on the calling thread; the
/// caller is the poll task, so the macOS IOKit enumeration cost lands off the
/// render path.
fn list_serial_ports() -> Result<Vec<PortIdentity>, String> {
    Ok(crate::hardware::canonical_ports(
        serialport::available_ports()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|info| match info.port_type {
                serialport::SerialPortType::UsbPort(usb) => PortIdentity {
                    path: info.port_name.clone(),
                    vendor_id: Some(format!("{:04x}", usb.vid)),
                    product_id: Some(format!("{:04x}", usb.pid)),
                    manufacturer: usb.manufacturer.clone(),
                    serial_number: usb.serial_number.clone(),
                },
                _ => PortIdentity {
                    path: info.port_name.clone(),
                    ..Default::default()
                },
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(relative: &str, language: Language) -> ProjectFile {
        ProjectFile {
            name: relative.rsplit('/').next().unwrap().to_string(),
            relative: relative.to_string(),
            path: PathBuf::from(relative),
            content: String::new(),
            language,
        }
    }

    #[test]
    fn opens_the_entry_point_for_the_project_kind() {
        let files = vec![
            file("README.md", Language::Markdown),
            file("platformio.ini", Language::Ini),
            file("src/main.cpp", Language::Cpp),
        ];
        assert_eq!(
            pick_entry_file(&files, ProjectKind::PlatformIo)
                .unwrap()
                .relative,
            "src/main.cpp"
        );

        let micropython = vec![
            file("helpers.py", Language::Python),
            file("main.py", Language::Python),
        ];
        assert_eq!(
            pick_entry_file(&micropython, ProjectKind::MicroPython)
                .unwrap()
                .relative,
            "main.py"
        );
    }

    #[test]
    fn falls_back_to_source_over_configuration() {
        let files = vec![
            file("platformio.ini", Language::Ini),
            file("sketch.ino", Language::Arduino),
        ];
        assert_eq!(
            pick_entry_file(&files, ProjectKind::Unknown)
                .unwrap()
                .relative,
            "sketch.ino"
        );
        assert!(pick_entry_file(&[], ProjectKind::Unknown).is_none());
    }

    #[test]
    fn error_shaped_output_is_logged_as_an_error() {
        assert!(looks_like_an_error("main.cpp:4:1: error: expected ';'"));
        assert!(looks_like_an_error("Traceback (most recent call last):"));
        assert!(!looks_like_an_error("Writing at 0x00010000... (54 %)"));
        assert!(!looks_like_an_error(
            "Compiling .pio/build/esp32dev/src/main.cpp.o"
        ));
    }

    #[test]
    fn history_walks_back_then_returns_to_the_typed_line() {
        use HistoryStep::{Newer, Older};
        // Three entries, oldest first.
        assert_eq!(step_history(3, None, Older), Some(Some(2)));
        assert_eq!(step_history(3, Some(2), Older), Some(Some(1)));
        assert_eq!(step_history(3, Some(0), Older), None, "stops at the oldest");
        assert_eq!(step_history(3, Some(1), Newer), Some(Some(2)));
        assert_eq!(
            step_history(3, Some(2), Newer),
            Some(None),
            "past the newest is the line being typed"
        );
        assert_eq!(step_history(3, None, Newer), None);
        assert_eq!(step_history(0, None, Older), None, "no history, no move");
    }

    #[test]
    fn a_second_project_from_one_template_gets_its_own_folder() {
        let base = tempfile::tempdir().unwrap();
        let first = unique_directory(base.path(), "arduino-blink");
        std::fs::create_dir_all(&first).unwrap();
        let second = unique_directory(base.path(), "arduino-blink");
        assert_ne!(first, second);
        assert!(second.ends_with("arduino-blink-2"));
    }
}
