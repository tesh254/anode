//! The main Anode workspace state.
//!
//! Translated from `src/App.tsx` + `src/lib/store.ts` (the non-persisted
//! subset). What was a Zustand store plus a tree of React components is one
//! GPUI entity: domain state lives in named fields, editing state in child
//! entities created once in `new`, and the React effects became explicit
//! tasks and subscription handlers.

use crate::boards::{self, BoardType, PortIdentity};
use crate::device::{self, DeviceEvent};
use crate::profiles::{self, CircuitPythonDrive, Confidence, ExecutionProfile, WorkspaceLayout};
use crate::project::{self, Language, ProjectFile};
use crate::repl::{self, ReplConnection, ReplEvent};
use crate::settings::{BottomMode, Settings};

use gpui_kit::base::input::EditorState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, ScrollHandle, Subscription, Task, Window};

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Log entries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Warn,
    Error,
    Ok,
    Install,
    Flash,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub id: u64,
    pub kind: LogKind,
    pub message: String,
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
    Finished(Result<(), String>),
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

pub struct Workspace {
    // Persisted settings (recent projects, last port, board overrides).
    pub settings: Settings,

    // Project state.
    pub project_path: Option<PathBuf>,
    pub project_name: String,
    pub project_files: Vec<ProjectFile>,
    pub current_file: Option<ProjectFile>,
    pub is_dirty: bool,

    // Multi-tab editor state.
    pub open_tabs: Vec<OpenTab>,
    pub active_tab: Option<PathBuf>,

    // Editor state (retained entity).
    pub editor: Entity<EditorState>,
    /// The file the editor currently shows, so changes land in the right slot.
    editor_file: Option<PathBuf>,
    /// Guard so a programmatic `set_value` doesn't mark the tab dirty.
    editor_syncing: bool,

    // Connection state.
    pub ports: Vec<PortIdentity>,
    pub selected_port: String,
    pub detected_board: BoardType,
    pub available_profiles: Vec<ExecutionProfile>,
    pub active_profile: Option<ExecutionProfile>,
    pub profile_manually_set: bool,
    pub circuit_python_drive: Option<CircuitPythonDrive>,
    pub connection_dropped: bool,

    // REPL state.
    pub repl_connection: Option<ReplConnection>,
    pub repl_connected: bool,
    pub repl_output: String,
    pub repl_input: Entity<InputState>,
    repl_events: Option<Receiver<ReplEvent>>,
    pub repl_history: Vec<String>,

    // Shell log.
    pub log_entries: Vec<LogEntry>,
    pub log_scroll: ScrollHandle,
    next_log_id: u64,

    // Background device work: progress lines and completion.
    device_events: Option<Receiver<BackgroundOutcome>>,
    pub device_busy: bool,
    /// Flashes/runs started before the poll task can deliver their result;
    /// keyed by a monotonic token so a stale result is dropped.
    pub flash_board_choice: BoardType,

    // Bottom panel.
    pub bottom_mode: BottomMode,
    pub(crate) panels: Option<crate::panels::PanelWorkspace>,
    pub(crate) port_leases: crate::hardware::PortLeases,
    pub(crate) layout_save_task: Option<Task<()>>,

    // Retained Select entities (port, board, runtime profile, flash dialog).
    pub pickers: crate::views::Pickers,
    pub picker_caches: crate::views::PickerCaches,
    pub pending_flash_tool: Option<String>,
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
                .placeholder("// Open a file from the sidebar to start editing")
        });
        let repl_input = cx.new(|cx| InputState::new(window, cx).placeholder(">>> …"));

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
                }
            });

        let mut this = Self {
            settings,
            project_path: None,
            project_name: String::new(),
            project_files: Vec::new(),
            current_file: None,
            is_dirty: false,
            open_tabs: Vec::new(),
            active_tab: None,
            editor,
            editor_file: None,
            editor_syncing: false,
            ports: Vec::new(),
            selected_port: String::new(),
            detected_board: BoardType::Unknown,
            available_profiles: Vec::new(),
            active_profile: None,
            profile_manually_set: false,
            circuit_python_drive: None,
            connection_dropped: false,
            repl_connection: None,
            repl_connected: false,
            repl_output: String::new(),
            repl_input,
            repl_events: None,
            repl_history: Vec::new(),
            log_entries: Vec::new(),
            log_scroll: ScrollHandle::new(),
            next_log_id: 1,
            device_events: None,
            device_busy: false,
            flash_board_choice: BoardType::Unknown,
            bottom_mode: BottomMode::Shell,
            panels: None,
            port_leases: Default::default(),
            layout_save_task: None,
            pickers: Default::default(),
            picker_caches: Default::default(),
            pending_flash_tool: None,
            pending_open_path: None,
            needs_editor_sync: false,
            _poll_task: None,
            _subscriptions: vec![editor_subscription, repl_subscription],
        };
        this.start_port_polling(cx);
        this
    }

    // -- Port polling --------------------------------------------------------

    /// Poll serial ports every second, auto-detect the board, and keep the
    /// execution profile in step (translated from `refreshPorts` +
    /// `useAutoConnect`).
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
                        if let Ok(ports) = ports {
                            this.apply_port_snapshot(ports, drive);
                        }
                        // Discovery runs off the UI thread; notify after the coherent snapshot.
                        // Active serial sessions also drain their queues in the ensuing render.
                        cx.notify();
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
                    Ok(ports) => this.apply_port_snapshot(ports, drive),
                    Err(error) => {
                        this.add_log(LogKind::Error, format!("Could not scan USB ports: {error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_port_snapshot(&mut self, ports: Vec<PortIdentity>, drive: CircuitPythonDrive) {
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
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
        let candidate = if !self.settings.last_port_path.is_empty() {
            self.ports
                .iter()
                .find(|p| {
                    p.path == self.settings.last_port_path && profiles::is_likely_hardware_port(p)
                })
                .cloned()
        } else {
            None
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
            .find(|p| p.path == self.selected_port)
            .cloned();
        let Some(current) = current else {
            // The selected port vanished — flag the drop and wait for it.
            if !self.connection_dropped {
                self.connection_dropped = true;
                self.add_log(
                    crate::LogKind::Warn,
                    format!(
                        "Connection lost on {}. Waiting for reconnect...",
                        self.selected_port
                    ),
                );
            }
            return;
        };

        if self.connection_dropped {
            self.connection_dropped = false;
            self.add_log(
                crate::LogKind::Ok,
                format!("Reconnected to {}", current.path),
            );
        }

        self.detected_board = profiles::detect_board_type(&current);

        // Keep the runtime/workspace in step with what's actually on this port
        // (board swaps, a CIRCUITPY drive mounting, fresh-launch restore).
        let profiles_list = self.detect_profiles_for(&current);
        let next = profiles::resolve_profile_on_detect(
            self.active_profile.as_ref(),
            &profiles_list,
            Some(&self.settings.last_profile_id),
            self.profile_manually_set,
        );
        if let Some(profile) = next {
            if self
                .active_profile
                .as_ref()
                .is_none_or(|p| p.id != profile.id)
            {
                self.settings.last_profile_id = profile.id.clone();
                self.settings.save();
                self.active_profile = Some(profile);
            }
        }
    }

    fn detect_profiles_for(&self, port: &PortIdentity) -> Vec<ExecutionProfile> {
        profiles::detect_profiles(port, self.circuit_python_drive.as_ref())
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
        if let Some(port) = self.ports.iter().find(|p| p.path == path).cloned() {
            self.detected_board = profiles::detect_board_type(&port);
            self.rebuild_profiles_for(&port);
        }
    }

    /// The user pinned an exact board from the picker.
    pub fn set_board_override(&mut self, board_id: &str) {
        let Some(port) = self.selected_port_identity() else {
            return;
        };
        let key = boards::port_key(&port);
        self.settings
            .board_overrides
            .insert(key, board_id.to_string());
        self.settings.save();
        if let Some(board) = boards::board_by_id(board_id) {
            self.detected_board = board.flash_board_type;
        }
    }

    /// The user chose an execution profile explicitly.
    pub fn select_profile(&mut self, profile_id: &str) {
        if let Some(profile) = self
            .available_profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
        {
            self.settings.last_profile_id = profile.id.clone();
            self.settings.save();
            self.active_profile = Some(profile);
            self.profile_manually_set = true;
        }
    }

    // -- Logging -------------------------------------------------------------

    pub fn add_log(&mut self, kind: crate::LogKind, message: impl Into<String>) {
        self.log_entries.push(LogEntry {
            id: self.next_log_id,
            kind,
            message: message.into(),
        });
        // Bound the log so a chatty flash session can't grow it forever.
        if self.log_entries.len() > 1000 {
            let drop = self.log_entries.len() - 1000;
            self.log_entries.drain(..drop);
        }
        self.next_log_id += 1;
    }

    fn log_from_progress(&mut self, message: &str) {
        let (kind, message) = if let Some(rest) = message.strip_prefix("[ERROR]") {
            (crate::LogKind::Error, rest.trim())
        } else if let Some(rest) = message.strip_prefix("[WARN]") {
            (crate::LogKind::Warn, rest.trim())
        } else if let Some(rest) = message.strip_prefix("[OK]") {
            (crate::LogKind::Ok, rest.trim())
        } else {
            (crate::LogKind::Flash, message)
        };
        self.add_log(kind, message.to_string());
    }

    // -- Project -------------------------------------------------------------

    pub fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_project_from_drain(path);
        self.sync_editor_to_current_file(window, cx);
    }

    /// Load a project without window access; the editor syncs on the next
    /// render via `needs_editor_sync`.
    fn open_project_from_drain(&mut self, path: PathBuf) {
        self.needs_editor_sync = true;
        match project::load_project(&path) {
            Ok(files) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Project".into());
                self.add_log(crate::LogKind::Ok, format!("Opened {name}"));
                self.settings
                    .add_recent_project(path.to_string_lossy().to_string(), name.clone());
                self.settings.save();
                self.project_path = Some(path);
                self.project_name = name;
                self.project_files = files;
                self.open_tabs.clear();
                self.active_tab = None;
                self.current_file = self.project_files.first().cloned();
                self.is_dirty = false;
                if let Some(file) = self.current_file.clone() {
                    self.open_tab_for(file);
                }
                self.needs_editor_sync = true;
            }
            Err(e) => self.add_log(
                crate::LogKind::Error,
                format!("Failed to open project folder: {e}"),
            ),
        }
    }

    /// Open a project from a recent-projects entry.
    pub fn open_recent_project(&mut self, path: &str) {
        let path = PathBuf::from(path);
        if path.is_dir() {
            self.pending_open_path = Some(path);
        } else {
            self.add_log(
                crate::LogKind::Error,
                format!("Project folder no longer exists: {}", path.display()),
            );
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
        if !self.open_tabs.iter().any(|t| t.path == file.path) {
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
        if let Some(file) = self.project_files.iter().find(|f| f.path == path).cloned() {
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

    pub fn close_tab(&mut self, index: usize) {
        let Some(tab) = self.open_tabs.get(index) else {
            return;
        };
        let path = tab.path.clone();
        self.open_tabs.retain(|t| t.path != path);
        if self.active_tab.as_ref() == Some(&path) {
            // Activate the last remaining tab, if any.
            self.active_tab = self.open_tabs.last().map(|t| t.path.clone());
            self.current_file = self
                .active_tab
                .as_ref()
                .and_then(|p| self.project_files.iter().find(|f| &f.path == p).cloned());
        }
    }

    /// Point the editor entity at the current file. Requires window access for
    /// `set_value`; callers pass the window they already hold.
    fn sync_editor_to_current_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_syncing = true;
        let content = self
            .current_file
            .as_ref()
            .map(|f| f.content.clone())
            .unwrap_or_default();
        let language_name = match self
            .current_file
            .as_ref()
            .map(|f| f.language)
            .unwrap_or(Language::Python)
        {
            Language::Python => "python",
            Language::Arduino => "cpp",
        };
        self.editor_file = self.current_file.as_ref().map(|f| f.path.clone());
        self.editor.update(cx, |state, cx| {
            state.set_value(content, window, cx);
            state.set_highlighter(language_name, cx);
        });
        self.editor_syncing = false;
    }

    // -- Save ----------------------------------------------------------------

    pub fn save_current_file(&mut self) {
        let Some(file) = self.current_file.clone() else {
            self.add_log(crate::LogKind::Warn, "No file open");
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
                    self.add_log(crate::LogKind::Ok, format!("{} saved", file.name));
                }
                true
            }
            Err(e) => {
                self.add_log(
                    crate::LogKind::Error,
                    format!("Failed to save {}: {e}", file.name),
                );
                false
            }
        }
    }

    pub fn save_all_files(&mut self) {
        if self.project_files.is_empty() {
            return;
        }
        let files = self.project_files.clone();
        self.add_log(
            crate::LogKind::Info,
            format!("Saving {} project files locally...", files.len()),
        );
        for file in &files {
            if !self.save_file(file, true) {
                return;
            }
        }
        self.is_dirty = false;
        for tab in &mut self.open_tabs {
            tab.is_dirty = false;
        }
        self.add_log(
            crate::LogKind::Ok,
            format!("Saved {} files locally.", files.len()),
        );
    }

    // -- Run / upload / stop -------------------------------------------------

    pub fn selected_port_identity(&self) -> Option<PortIdentity> {
        self.ports
            .iter()
            .find(|p| p.path == self.selected_port)
            .cloned()
    }

    pub fn is_circuit_python(&self) -> bool {
        self.active_profile
            .as_ref()
            .is_some_and(|p| p.runtime == boards::Runtime::CircuitPython)
    }

    pub fn is_arduino(&self) -> bool {
        self.active_profile
            .as_ref()
            .is_some_and(|p| p.runtime == boards::Runtime::Arduino)
    }

    pub fn workspace_layout(&self) -> WorkspaceLayout {
        profiles::workspace_layout(self.is_arduino())
    }

    pub fn run_code(&mut self) {
        let Some(file) = self.current_file.clone() else {
            self.add_log(crate::LogKind::Warn, "No file open");
            return;
        };

        // CircuitPython deploys to the drive instead of running over serial.
        // The board auto-reloads and runs code.py; output streams to the REPL.
        if self.is_circuit_python() {
            self.deploy_circuit_python(vec![file]);
            return;
        }

        if self.selected_port.is_empty() {
            self.add_log(crate::LogKind::Warn, "No board connected");
            return;
        }
        let port = self.selected_port.clone();

        if !self.save_file(&file, true) {
            return;
        }
        self.add_log(
            crate::LogKind::Info,
            format!("Running {} on {}...", file.name, port),
        );
        self.spawn_device_work(move |emit| device::run_code(&port, &file.content, emit));
    }

    pub fn upload_project(&mut self) {
        let files: Vec<ProjectFile> = self
            .project_files
            .iter()
            .filter(|f| matches!(f.language, Language::Python | Language::Arduino))
            .cloned()
            .collect();
        if files.is_empty() {
            self.add_log(crate::LogKind::Warn, "No files to upload");
            return;
        }

        if self.is_circuit_python() {
            self.deploy_circuit_python(files);
            return;
        }

        if self.selected_port.is_empty() {
            self.add_log(crate::LogKind::Warn, "No board connected");
            return;
        }
        let port = self.selected_port.clone();
        self.save_all_files();
        let total = files.len();
        self.add_log(
            crate::LogKind::Info,
            format!("Deploying {total} files to {port}..."),
        );
        self.spawn_device_work(move |emit| {
            for file in &files {
                emit(DeviceEvent::Progress(format!(
                    "[FLASH] Uploading {} …",
                    file.name
                )));
                device::upload_code(&port, &file.name, &file.content, emit)?;
            }
            emit(DeviceEvent::Progress(format!(
                "[OK] Deployment complete. {total} files synced to board."
            )));
            Ok(())
        });
    }

    fn deploy_circuit_python(&mut self, files: Vec<ProjectFile>) {
        let drive = self.circuit_python_drive.clone().unwrap_or_default();
        let total = files.len();
        self.add_log(
            crate::LogKind::Info,
            format!("Copying {total} files to CIRCUITPY…"),
        );
        self.spawn_device_work(move |emit| {
            for file in &files {
                emit(DeviceEvent::Progress(format!(
                    "[FLASH] Copying {} to CIRCUITPY…",
                    file.name
                )));
                device::circuitpython_deploy(&drive, &file.name, &file.content)?;
            }
            emit(DeviceEvent::Progress(format!(
                "[OK] Deployment complete. {total} files copied to CIRCUITPY."
            )));
            Ok(())
        });
    }

    pub fn stop_code(&mut self) {
        if self.selected_port.is_empty() {
            self.add_log(crate::LogKind::Warn, "No board connected");
            return;
        }
        let port = self.selected_port.clone();
        self.add_log(crate::LogKind::Warn, "Stopping execution...");
        self.spawn_device_work(move |emit| device::stop_code(&port, emit));
    }

    pub fn arduino_compile(&mut self) {
        let Some(project_path) = self.project_path.clone() else {
            self.add_log(crate::LogKind::Warn, "No project open");
            return;
        };
        let fqbn = self
            .active_profile
            .as_ref()
            .and_then(|p| p.fqbn.clone())
            .unwrap_or_else(|| "arduino:avr:uno".into());
        self.add_log(
            crate::LogKind::Info,
            format!("Compiling with arduino-cli ({fqbn})..."),
        );
        self.spawn_device_work(move |emit| device::arduino_compile(&project_path, &fqbn, emit));
    }

    pub fn arduino_upload(&mut self) {
        let Some(project_path) = self.project_path.clone() else {
            self.add_log(crate::LogKind::Warn, "No project open");
            return;
        };
        if self.selected_port.is_empty() {
            self.add_log(crate::LogKind::Warn, "No board connected");
            return;
        }
        let port = self.selected_port.clone();
        let fqbn = self
            .active_profile
            .as_ref()
            .and_then(|p| p.fqbn.clone())
            .unwrap_or_else(|| "arduino:avr:uno".into());
        self.add_log(
            crate::LogKind::Info,
            format!("Uploading to {port} ({fqbn})..."),
        );
        self.spawn_device_work(move |emit| {
            device::arduino_upload(&port, &project_path, &fqbn, emit)
        });
    }

    /// Run device I/O on a background thread; its progress lines and result
    /// are drained back onto the UI thread by `drain_background_events`.
    fn spawn_device_work(
        &mut self,
        work: impl FnOnce(&mut dyn FnMut(DeviceEvent)) -> Result<(), String> + Send + 'static,
    ) {
        if self.device_busy {
            self.add_log(crate::LogKind::Warn, "Another device operation is running");
            return;
        }
        let key = self
            .selected_port_identity()
            .map(|port| crate::hardware::device_key(&port))
            .unwrap_or_else(|| self.selected_port.replace("/dev/tty.", "/dev/cu."));
        let lease = if key.is_empty() {
            None
        } else {
            match self.port_leases.acquire(&key) {
                Ok(lease) => Some(lease),
                Err(error) => {
                    self.add_log(LogKind::Warn, error);
                    return;
                }
            }
        };
        let (tx, rx) = std::sync::mpsc::channel::<BackgroundOutcome>();
        self.device_events = Some(rx);
        self.device_busy = true;
        std::thread::spawn(move || {
            let _lease = lease;
            let mut sink = |event: DeviceEvent| {
                let DeviceEvent::Progress(message) = event;
                let _ = tx.send(BackgroundOutcome::Progress(message));
            };
            let result = work(&mut sink);
            let _ = tx.send(BackgroundOutcome::Finished(result));
        });
    }

    /// Deliver background progress/results to the log. Called from render and
    /// from the poll tick, so results arrive even between user actions.
    pub fn drain_background_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(path) = self.pending_open_path.take() {
            self.open_project_from_drain(path);
            self.sync_editor_to_current_file(window, cx);
            changed = true;
        }
        if self.needs_editor_sync {
            self.needs_editor_sync = false;
            self.sync_editor_to_current_file(window, cx);
            changed = true;
        }
        while self.device_events.is_some() {
            let Some(rx) = &self.device_events else { break };
            let event = match rx.try_recv() {
                Ok(event) => event,
                Err(_) => break,
            };
            match event {
                BackgroundOutcome::Progress(message) => {
                    self.log_from_progress(&message);
                    changed = true;
                }
                BackgroundOutcome::Finished(result) => {
                    if let Err(e) = result {
                        if !e.is_empty() {
                            self.add_log(crate::LogKind::Error, e);
                        }
                    }
                    self.device_busy = false;
                    self.device_events = None;
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    // -- Flash ---------------------------------------------------------------

    /// The flash button: detect the board kind and open the flash dialog.
    pub fn begin_flash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_port.is_empty() {
            self.add_log(crate::LogKind::Warn, "Please select a board first.");
            return;
        }
        // Detect from the selected port (falling back to its path alone).
        let detected = self
            .resolved_board()
            .board
            .map(|board| board.flash_board_type)
            .or_else(|| {
                self.selected_port_identity()
                    .map(|p| profiles::detect_board_type(&p))
            })
            .unwrap_or_else(|| {
                profiles::detect_board_type(&PortIdentity {
                    path: self.selected_port.clone(),
                    ..Default::default()
                })
            });
        self.flash_board_choice = detected;
        if detected == BoardType::Unknown {
            self.add_log(
                crate::LogKind::Warn,
                format!(
                    "Could not auto-detect board on {}. Select manually in the dialog.",
                    self.selected_port
                ),
            );
        } else {
            self.add_log(
                crate::LogKind::Info,
                format!("Detected {} on {}", detected.label(), self.selected_port),
            );
        }
        cx.notify();
        self.open_flash_dialog(window, cx);
    }

    /// Confirm the flash dialog: verify the tool, then flash on a background
    /// thread.
    pub fn confirm_flash(&mut self, board_type: BoardType) {
        if self.selected_port.is_empty() {
            self.add_log(crate::LogKind::Warn, "Please select a board first.");
            return;
        }
        let port_path = self.selected_port.clone();
        if let Some(tool) = board_type.tool() {
            let tool_name = tool.to_string();
            let available = std::thread::spawn(move || device::tool_available(&tool_name))
                .join()
                .unwrap_or(false);
            if !available {
                self.add_log(
                    crate::LogKind::Error,
                    format!("{tool} not found. Install it to flash this board."),
                );
                return;
            }
        }
        self.add_log(
            crate::LogKind::Info,
            format!("Flashing {} firmware on {port_path}...", board_type.label()),
        );
        let variant = self
            .resolved_board()
            .board
            .filter(|board| board.flash_board_type == board_type)
            .and_then(|board| board.mpy_variant);
        self.spawn_device_work(move |emit| {
            let outcome = device::flash_firmware(&port_path, board_type, variant, emit);
            if outcome.success {
                Ok(())
            } else {
                Err(outcome.error.unwrap_or_else(|| "Flash failed.".to_string()))
            }
        });
    }

    /// Install a missing flashing tool (esptool / arduino-cli).
    pub fn install_tool(&mut self, tool: &str) {
        let tool = tool.to_string();
        self.add_log(crate::LogKind::Install, format!("Installing {tool}…"));
        self.spawn_device_work(move |emit| {
            device::install_tool(&tool, emit)?;
            emit(DeviceEvent::Progress(
                "[OK] Tool installed successfully".into(),
            ));
            Ok(())
        });
    }

    /// Open the native folder picker and load the chosen directory.
    pub fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project Folder".into()),
        });
        cx.spawn(async move |this, cx: &mut gpui_kit::AsyncApp| {
            let paths = rx.await;
            let picked = paths
                .ok()
                .and_then(|result| result.ok())
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            if let Some(path) = picked {
                this.update(cx, |this, cx| {
                    this.pending_open_path = Some(path);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Create a project from a welcome-screen template in a new sibling
    /// directory of the last-used project, then open it.
    pub fn create_from_template(&mut self, template_id: &str, cx: &mut Context<Self>) {
        let Some(template) = crate::project::TEMPLATES
            .iter()
            .find(|t| t.id == template_id)
        else {
            return;
        };
        // Create the project under ~/Documents/Anode/<template-id>.
        let base = dirs::document_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Anode");
        let project_dir = base.join(template.name.to_lowercase().replace(' ', "-"));
        let main_file = crate::project::template_main_file(template);

        let dir = project_dir.clone();
        let file_name = main_file.to_string();
        let code = template.code.to_string();
        let result =
            std::thread::spawn(move || crate::project::create_file(&dir, &file_name, &code)).join();
        match result {
            Ok(Ok(_path)) => {
                let name = project_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Project".into());
                self.add_log(
                    crate::LogKind::Ok,
                    format!("Created {} from {}", name, template.name),
                );
                self.pending_open_path = Some(project_dir);
            }
            Ok(Err(e)) => {
                self.add_log(
                    crate::LogKind::Error,
                    format!("Failed to create project: {e}"),
                );
            }
            Err(_) => {
                self.add_log(
                    crate::LogKind::Error,
                    "Failed to create project".to_string(),
                );
            }
        }
        cx.notify();
    }

    // -- REPL ----------------------------------------------------------------

    pub fn repl_connect(&mut self) {
        if self.selected_port.is_empty() {
            self.add_log(
                crate::LogKind::Warn,
                "Select a port before opening the REPL",
            );
            return;
        }
        let port = self.selected_port.clone();
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
            Err(e) => {
                self.repl_output
                    .push_str(&format!("[ERROR] Connection failed: {e}\n"));
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
            self.repl_history.push(line.clone());
        }
        if let Some(connection) = &self.repl_connection {
            connection.send(format!("{line}\r\n").as_bytes());
        }
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
            self.repl_output.push_str("\n[Soft Reset]\n");
        }
    }

    pub fn repl_send_raw(&mut self, data: String) {
        self.send_repl_line(data);
    }

    /// Forward any REPL bytes that arrived since the last frame into the
    /// terminal buffer. Called from render.
    pub fn drain_repl_events(&mut self) {
        while let Some(rx) = &self.repl_events {
            match rx.try_recv() {
                Ok(ReplEvent::Data(text)) => self.repl_output.push_str(&text),
                Ok(ReplEvent::Disconnected) => {
                    self.repl_connection = None;
                    self.repl_events = None;
                    self.repl_connected = false;
                    self.repl_output.push_str("\n--- Disconnected ---\n");
                    break;
                }
                Ok(ReplEvent::Error(err)) => {
                    self.repl_output.push_str(&format!("\n[ERROR] {err}\n"));
                }
                Err(_) => break,
            }
        }
    }

    // -- Bottom panel --------------------------------------------------------

    pub fn set_bottom_mode(&mut self, mode: BottomMode) {
        self.bottom_mode = mode;
        self.settings.bottom_mode = mode;
        self.settings.save();
    }

    // -- Recent projects -----------------------------------------------------

    pub fn recent_projects(&self) -> &[crate::settings::RecentProject] {
        &self.settings.recent_projects
    }

    pub fn clear_recent_projects(&mut self) {
        self.settings.recent_projects.clear();
        self.settings.save();
    }

    /// Detect profiles for a port, for the runtime-profile picker items.
    pub fn profile_items(&self) -> Vec<ExecutionProfile> {
        self.available_profiles.clone()
    }

    pub fn board_candidates(&self) -> Vec<&'static str> {
        let port = self.selected_port_identity();
        let overrides = &self.settings.board_overrides;
        boards::identify_board(port.as_ref(), overrides).candidates
    }

    pub fn resolved_board(&self) -> boards::BoardMatch {
        let port = self.selected_port_identity();
        boards::identify_board(port.as_ref(), &self.settings.board_overrides)
    }

    /// Profile detection input for the currently-selected port, exposed for
    /// tests and the welcome screen.
    pub fn detected_board_label(&self) -> Option<String> {
        self.resolved_board()
            .board
            .map(|b| b.name.to_string())
            .or_else(|| {
                (self.detected_board != BoardType::Unknown)
                    .then(|| self.detected_board.label().to_string())
            })
    }

    pub fn confidence_label(&self) -> Option<(String, String)> {
        let profile = self.active_profile.as_ref()?;
        Some((profile.runtime.label().to_string(), profile.variant.clone()))
    }

    pub fn is_profile_pinned(&self, profile: &ExecutionProfile) -> bool {
        profile.confidence == Confidence::High && !profile.is_generic
    }
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
