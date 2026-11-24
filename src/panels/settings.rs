//! Settings: where the agents Anode can launch are chosen, installed, signed
//! in to, and configured.
//!
//! The Providers page is a master–detail: every configured provider on the
//! left, the selected one's setup, runtime, and environment on the right.
//! Installing and signing in run as managed child processes whose output is
//! shown here, so a failure is visible instead of disappearing into a terminal.
use crate::agent::install::{self, JobEvent, JobHandle, Readiness};
use crate::agent::{
    diagnostic_text, probe_provider, test_connection, AuthState, ProviderId, ProviderKind,
    ProviderProfile, ProviderStatus, Registry,
};
use crate::workspace_state::Workspace;
use futures::StreamExt as _;
use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::link::Link;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::sidebar::{Sidebar, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled, Task,
    WeakEntity, Window,
};
use std::collections::{BTreeMap, HashMap};

/// Where usage for a Codex ChatGPT account is managed.
const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/#settings/Usage";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Providers,
    Agent,
    Diagnostics,
}

impl Page {
    const ALL: [Page; 3] = [Page::Providers, Page::Agent, Page::Diagnostics];

    fn label(self) -> &'static str {
        match self {
            Self::Providers => "Providers",
            Self::Agent => "Agent",
            Self::Diagnostics => "Diagnostics",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Providers => IconName::Plug,
            Self::Agent => IconName::Sparkles,
            Self::Diagnostics => IconName::ScanLine,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JobKind {
    Install,
    SignIn,
}

/// A managed `npm install` or provider sign-in, with the output it has printed.
struct Job {
    kind: JobKind,
    provider: ProviderId,
    lines: Vec<String>,
    outcome: Option<Result<String, String>>,
    handle: JobHandle,
    _task: Task<()>,
}

impl Job {
    fn is_running(&self) -> bool {
        self.outcome.is_none()
    }
}

/// One editable environment variable.
struct EnvRow {
    row: u64,
    name: Entity<InputState>,
    value: Entity<InputState>,
}

/// The unsaved form for the provider currently shown in the detail pane.
struct ProviderEditor {
    id: ProviderId,
    name: Entity<InputState>,
    command: Entity<InputState>,
    args: Entity<InputState>,
    env: Vec<EnvRow>,
    next_row: u64,
    error: Option<String>,
    notice: Option<String>,
    test: Option<Result<String, String>>,
    testing: bool,
    _test_task: Option<Task<()>>,
}

impl ProviderEditor {
    fn new(profile: &ProviderProfile, window: &mut Window, cx: &mut App) -> Self {
        let mut field = |value: &str, placeholder: &'static str, masked: bool, cx: &mut App| {
            let value = value.to_string();
            cx.new(|cx| {
                let mut state = InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(masked);
                state.set_value(value, window, cx);
                state
            })
        };
        let mut editor = Self {
            id: profile.id.clone(),
            name: field(&profile.name, "Display name", false, cx),
            command: field(&profile.command, "Executable name or path", false, cx),
            args: field(
                &serde_json::to_string(&profile.args).unwrap_or_else(|_| "[]".into()),
                r#"["acp"]"#,
                false,
                cx,
            ),
            env: Vec::new(),
            next_row: 0,
            error: None,
            notice: None,
            test: None,
            testing: false,
            _test_task: None,
        };
        for (name, value) in &profile.env {
            editor.push_env(name, value, window, cx);
        }
        editor
    }

    fn push_env(&mut self, name: &str, value: &str, window: &mut Window, cx: &mut App) {
        let row = self.next_row;
        self.next_row += 1;
        let masked = is_secret(name);
        let name_value = name.to_string();
        let value_value = value.to_string();
        let name = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("OPENAI_BASE_URL");
            state.set_value(name_value, window, cx);
            state
        });
        let value = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder("Value")
                .masked(masked);
            state.set_value(value_value, window, cx);
            state
        });
        self.env.push(EnvRow { row, name, value });
    }

    /// Read the form back into a profile, reporting what is wrong instead of
    /// saving something that cannot start.
    fn to_profile(&self, base: &ProviderProfile, cx: &App) -> Result<ProviderProfile, String> {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err("Give the provider a display name.".into());
        }
        let command = self.command.read(cx).value().trim().to_string();
        if command.is_empty() {
            return Err("Set the command that starts the agent.".into());
        }
        let args: Vec<String> = serde_json::from_str(self.args.read(cx).value().trim())
            .map_err(|error| format!("Arguments must be a JSON array of strings: {error}"))?;
        let mut env = BTreeMap::new();
        for row in &self.env {
            let key = row.name.read(cx).value().trim().to_string();
            if key.is_empty() {
                continue;
            }
            if key.contains(char::is_whitespace) || key.contains('=') {
                return Err(format!("`{key}` is not a usable environment name."));
            }
            env.insert(key, row.value.read(cx).value().to_string());
        }
        Ok(ProviderProfile {
            id: base.id.clone(),
            kind: base.kind,
            name,
            command,
            args,
            env,
            enabled: base.enabled,
        })
    }
}

/// Environment names whose values are hidden until the user reveals them.
fn is_secret(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL"]
        .iter()
        .any(|needle| name.contains(needle))
}

pub(crate) struct SettingsScreen {
    owner: WeakEntity<Workspace>,
    focus: FocusHandle,
    page: Page,
    selected: Option<ProviderId>,
    editor: Option<ProviderEditor>,
    statuses: HashMap<ProviderId, ProviderStatus>,
    probing: bool,
    _probe_task: Option<Task<()>>,
    job: Option<Job>,
}

impl SettingsScreen {
    pub(super) fn new(
        owner: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            owner,
            focus: cx.focus_handle(),
            page: Page::Providers,
            selected: None,
            editor: None,
            statuses: HashMap::new(),
            probing: false,
            _probe_task: None,
            job: None,
        };
        this.prepare(window, cx);
        this
    }

    /// Open the screen on a provider, used when chat sends the user here.
    pub(crate) fn show_providers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = Page::Providers;
        self.prepare(window, cx);
        cx.notify();
    }

    /// Pick a provider to show and probe them all, after the current update.
    ///
    /// Both callers are reached from inside a `Workspace` update — panel
    /// construction and `show_tool` — and reading the workspace there would
    /// re-enter an entity that is already borrowed.
    fn prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.defer_in(window, |this, window, cx| {
            if this.selected.is_none() {
                if let Some(first) = this.registry(cx).and_then(|registry| registry.first_id()) {
                    this.select_provider(first, window, cx);
                }
            }
            this.refresh(cx);
        });
    }

    fn registry(&self, cx: &App) -> Option<Registry> {
        Some(self.owner.upgrade()?.read(cx).agents.registry().clone())
    }

    fn profile(&self, cx: &App) -> Option<ProviderProfile> {
        let id = self.selected.clone()?;
        self.registry(cx)?.get(&id).cloned()
    }

    fn select_provider(&mut self, id: ProviderId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = self
            .registry(cx)
            .and_then(|registry| registry.get(&id).cloned())
        else {
            return;
        };
        self.selected = Some(id);
        self.editor = Some(ProviderEditor::new(&profile, window, cx));
        cx.notify();
    }

    /// Re-read every provider's install and sign-in state from disk and from
    /// the providers' own commands.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.probing {
            return;
        }
        let Some(registry) = self.registry(cx) else {
            return;
        };
        self.probing = true;
        self._probe_task = Some(cx.spawn(async move |this, cx| {
            let profiles: Vec<ProviderProfile> = registry.all().to_vec();
            let results = cx
                .background_spawn(async move {
                    profiles
                        .into_iter()
                        .map(|profile| (profile.id.clone(), probe_provider(&profile)))
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.statuses = results.into_iter().collect();
                this.probing = false;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn update_registry(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Registry),
    ) -> Result<(), String> {
        let Some(owner) = self.owner.upgrade() else {
            return Err("Workspace closed.".into());
        };
        owner.update(cx, |workspace, cx| {
            let mut registry = workspace.agents.registry().clone();
            edit(&mut registry);
            let result = workspace.agents.set_registry(registry);
            workspace.settings.agent_provider = workspace
                .agents
                .selected_id()
                .map(|id| id.as_str().to_string());
            workspace.settings.save();
            cx.notify();
            result
        })
    }

    fn save_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(base) = self.profile(cx) else { return };
        // Never write a form that belongs to a provider other than the one on
        // screen, for instance after the selection changed mid-edit.
        let Some(editor) = self.editor.as_ref().filter(|editor| editor.id == base.id) else {
            return;
        };
        let profile = match editor.to_profile(&base, cx) {
            Ok(profile) => profile,
            Err(error) => {
                if let Some(editor) = &mut self.editor {
                    editor.error = Some(error);
                    editor.notice = None;
                }
                cx.notify();
                return;
            }
        };
        let id = profile.id.clone();
        match self.update_registry(cx, move |registry| registry.upsert(profile)) {
            Ok(()) => {
                if let Some(editor) = &mut self.editor {
                    editor.error = None;
                    editor.notice = Some("Saved".into());
                    editor.test = None;
                }
                self.select_provider(id, window, cx);
                self.refresh(cx);
            }
            Err(error) => {
                if let Some(editor) = &mut self.editor {
                    editor.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    fn reset_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let removed = id.clone();
        let _ = self.update_registry(cx, move |registry| {
            registry.remove(&removed);
        });
        let next = if id.is_builtin() {
            Some(id)
        } else {
            self.registry(cx).and_then(|registry| registry.first_id())
        };
        self.selected = None;
        self.editor = None;
        if let Some(next) = next {
            self.select_provider(next, window, cx);
        }
        self.refresh(cx);
        cx.notify();
    }

    fn add_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile = ProviderProfile::custom("New agent");
        let id = profile.id.clone();
        if self
            .update_registry(cx, move |registry| registry.upsert(profile))
            .is_ok()
        {
            self.page = Page::Providers;
            self.select_provider(id, window, cx);
            self.refresh(cx);
        }
    }

    fn set_enabled(&mut self, id: ProviderId, enabled: bool, cx: &mut Context<Self>) {
        let _ = self.update_registry(cx, move |registry| {
            if let Some(mut profile) = registry.get(&id).cloned() {
                profile.enabled = enabled;
                registry.upsert(profile);
            }
        });
        cx.notify();
    }

    /// Make this provider the one chat talks to.
    fn use_in_chat(&mut self, id: ProviderId, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        owner.update(cx, |workspace, cx| {
            if workspace.agents.select(&id) {
                workspace.settings.agent_provider = Some(id.as_str().to_string());
                workspace.settings.save();
            }
            cx.notify();
        });
        cx.notify();
    }

    fn start_job(&mut self, kind: JobKind, cx: &mut Context<Self>) {
        if self.job.as_ref().is_some_and(Job::is_running) {
            return;
        }
        let Some(profile) = self.profile(cx) else {
            return;
        };
        let Some(status) = self.statuses.get(&profile.id) else {
            return;
        };
        let steps = status.missing.clone();
        let handle = JobHandle::default();
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<JobEvent>();
        let worker_handle = handle.clone();
        let worker_profile = profile.clone();
        cx.background_spawn(async move {
            match kind {
                JobKind::Install => install::install_steps(steps, worker_handle, sender),
                JobKind::SignIn => install::sign_in(&worker_profile, worker_handle, sender),
            }
        })
        .detach();
        let task = cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                let finished = matches!(event, JobEvent::Finished(_));
                let updated = this.update(cx, |this, cx| this.on_job_event(event, cx));
                if updated.is_err() || finished {
                    break;
                }
            }
        });
        self.job = Some(Job {
            kind,
            provider: profile.id,
            lines: Vec::new(),
            outcome: None,
            handle,
            _task: task,
        });
        cx.notify();
    }

    fn on_job_event(&mut self, event: JobEvent, cx: &mut Context<Self>) {
        let Some(job) = &mut self.job else { return };
        match event {
            JobEvent::Line(line) => {
                job.lines.push(line);
                // Keep the tail: an npm install prints far more than fits.
                if job.lines.len() > 200 {
                    job.lines.remove(0);
                }
            }
            JobEvent::Finished(outcome) => {
                job.outcome = Some(outcome);
                self.refresh(cx);
            }
        }
        cx.notify();
    }

    fn test_connection(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile(cx) else {
            return;
        };
        let root = self
            .owner
            .upgrade()
            .map(|owner| owner.read(cx).agents.effective_root())
            .unwrap_or_else(std::env::temp_dir);
        let Some(editor) = &mut self.editor else {
            return;
        };
        if editor.testing {
            return;
        }
        editor.testing = true;
        editor.test = None;
        let task = cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move { test_connection(&profile, root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(editor) = &mut this.editor {
                    editor.testing = false;
                    editor.test = Some(outcome);
                }
                cx.notify();
            });
        });
        if let Some(editor) = &mut self.editor {
            editor._test_task = Some(task);
        }
        cx.notify();
    }

    // -- Rendering -----------------------------------------------------------

    fn render_navigation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        Sidebar::new("settings-navigation")
            .collapsible(false)
            .w(gpui_kit::rems(13.))
            .header(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_3()
                    .gap_2()
                    .child(Icon::new(IconName::Settings).size_4())
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Settings"),
                    ),
            )
            .child(
                SidebarMenu::new().children(Page::ALL.into_iter().map(|item| {
                    SidebarMenuItem::new(item.label())
                        .icon(Icon::new(item.icon()))
                        .active(item == page)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.page = item;
                            cx.notify();
                        }))
                })),
            )
            .footer(
                h_flex().w_full().child(
                    Button::new("settings-back")
                        .icon(IconName::ArrowLeft)
                        .label("Back")
                        .ghost()
                        .small()
                        .w_full()
                        .justify_start()
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                ),
            )
    }

    /// Leave Settings and return to the workspace.
    fn close(&mut self, cx: &mut Context<Self>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |workspace, cx| workspace.close_settings(cx));
        }
        cx.notify();
    }

    fn render_provider_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(registry) = self.registry(cx) else {
            return v_flex().into_any_element();
        };
        let selected = self.selected.clone();
        let in_chat = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).agents.selected_id().cloned());
        let mut list = v_flex()
            .w(gpui_kit::rems(15.))
            .flex_shrink_0()
            .min_h_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .p_2()
            .gap_1()
            .overflow_y_scrollbar();
        for profile in registry.all() {
            let id = profile.id.clone();
            let status = self.statuses.get(&id);
            let (label, tone) = status_summary(profile, status, self.probing);
            let is_selected = selected.as_ref() == Some(&id);
            let is_in_chat = in_chat.as_ref() == Some(&id);
            let row_id = SharedString::from(format!("provider-{id}"));
            let toggle_id = SharedString::from(format!("provider-toggle-{id}"));
            let enabled = profile.enabled;
            let toggle_target = id.clone();
            list = list.child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new(row_id)
                            .ghost()
                            .flex_1()
                            .min_w_0()
                            .selected(is_selected)
                            .toggled(is_selected)
                            .accessibility_label(profile.name.clone())
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .min_w_0()
                                    .child(Icon::new(kind_icon(profile.kind)).size_4())
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .items_start()
                                            .gap_0p5()
                                            .child(
                                                h_flex()
                                                    .gap_1()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_sm()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .child(profile.name.clone()),
                                                    )
                                                    .when_some(
                                                        status.and_then(|s| s.version.clone()),
                                                        |row, version| {
                                                            row.child(
                                                                div()
                                                                    .truncate()
                                                                    .text_xs()
                                                                    .font_family(
                                                                        cx.theme()
                                                                            .mono_font_family
                                                                            .clone(),
                                                                    )
                                                                    .text_color(
                                                                        cx.theme().muted_foreground,
                                                                    )
                                                                    .child(version),
                                                            )
                                                        },
                                                    ),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap_1()
                                                    .min_w_0()
                                                    .child(status_dot(tone, cx))
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_xs()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child(label),
                                                    )
                                                    .when(is_in_chat, |row| {
                                                        row.child(
                                                            Tag::info().small().child("In chat"),
                                                        )
                                                    }),
                                            ),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_provider(id.clone(), window, cx);
                            })),
                    )
                    .child(
                        Switch::new(toggle_id)
                            .checked(enabled)
                            .accessibility_label("Show this provider in chat")
                            .tooltip(if enabled {
                                "Shown in chat"
                            } else {
                                "Hidden from chat"
                            })
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                this.set_enabled(toggle_target.clone(), *checked, cx);
                            })),
                    ),
            );
        }
        list.into_any_element()
    }

    fn render_provider_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(profile) = self.profile(cx) else {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Select a provider"),
                )
                .into_any_element();
        };
        let status = self.statuses.get(&profile.id);
        let Some(editor) = &self.editor else {
            return v_flex().flex_1().into_any_element();
        };
        let id = profile.id.clone();
        let in_chat = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).agents.selected_id().cloned())
            == Some(id.clone());
        let available = status.is_some_and(ProviderStatus::is_available);

        let mut detail = v_flex().p_4().gap_4();

        // Header: who this is, and the one action that makes it the chat agent.
        detail = detail.child(
            h_flex()
                .justify_between()
                .gap_3()
                .child(
                    v_flex()
                        .gap_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(Icon::new(kind_icon(profile.kind)).size_4())
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(profile.name.clone()),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(profile.kind.description()),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("provider-use")
                                .label(if in_chat { "In chat" } else { "Use in chat" })
                                .outline()
                                .small()
                                .disabled(in_chat || !available)
                                .tooltip(if available {
                                    "Make this the agent the chat panel talks to"
                                } else {
                                    "Install the agent first"
                                })
                                .on_click(cx.listener({
                                    let id = id.clone();
                                    move |this, _, _, cx| this.use_in_chat(id.clone(), cx)
                                })),
                        )
                        .child(
                            Button::new("provider-refresh")
                                .icon(IconName::RefreshCw)
                                .ghost()
                                .small()
                                .disabled(self.probing)
                                .accessibility_label("Check again")
                                .tooltip("Check again")
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        ),
                ),
        );

        detail = detail.child(self.render_setup(&profile, status, cx));
        detail = detail.child(self.render_runtime(&profile, status, editor, cx));
        detail = detail.child(self.render_environment(editor, cx));

        if let Some(error) = &editor.error {
            detail = detail.child(Alert::error("provider-error", error.clone()).small());
        } else if let Some(notice) = &editor.notice {
            detail = detail.child(Alert::success("provider-notice", notice.clone()).small());
        }
        if let Some(test) = &editor.test {
            detail = detail.child(match test {
                Ok(message) => Alert::success("provider-test", message.clone()).small(),
                Err(error) => Alert::error("provider-test", diagnostic_text(error)).small(),
            });
        }

        // The commit row stays out of the scroll region: Save must be reachable
        // however long the environment list grows.
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            // The scroll owner resolves its height against this box. Scrolling
            // the column directly would make it 100% of the whole detail pane
            // and push the commit row past the bottom edge.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(detail.size_full().overflow_y_scrollbar()),
            )
            .child(
            h_flex()
                .flex_shrink_0()
                // A narrow dock still has to show every action, so the row
                // wraps rather than pushing Save past the panel edge.
                .flex_wrap()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .justify_between()
                .gap_2()
                .child(
                    Button::new("provider-remove")
                        .label(if profile.id.is_builtin() {
                            "Reset to defaults"
                        } else {
                            "Remove provider…"
                        })
                        .disabled(profile.id.is_builtin() && profile.is_default())
                        .ghost()
                        .small()
                        .when(!profile.id.is_builtin(), Button::danger)
                        .on_click(cx.listener({
                            let name = profile.name.clone();
                            let builtin = profile.id.is_builtin();
                            move |this, _, window, cx| {
                                if builtin {
                                    this.reset_provider(window, cx);
                                    return;
                                }
                                let panel = cx.weak_entity();
                                let name = name.clone();
                                window.open_alert_dialog(cx, move |alert, _, _| {
                                    let panel = panel.clone();
                                    alert
                                        .title(format!("Remove “{name}”?"))
                                        .description(
                                            "Its command and environment are deleted. Saved chats stay.",
                                        )
                                        .ok_text("Remove")
                                        .show_cancel(true)
                                        .on_ok(move |_, window, cx| {
                                            let _ = panel.update(cx, |this, cx| {
                                                this.reset_provider(window, cx)
                                            });
                                            true
                                        })
                                });
                            }
                        })),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .when(editor.testing, |row| row.child(Spinner::new().small()))
                        .child(
                            Button::new("provider-test-connection")
                                .label(if editor.testing {
                                    "Testing…"
                                } else {
                                    "Test connection"
                                })
                                .outline()
                                .small()
                                .disabled(editor.testing || !available)
                                .on_click(cx.listener(|this, _, _, cx| this.test_connection(cx))),
                        )
                        .child(
                            Button::new("provider-save")
                                .label("Save")
                                .primary()
                                .small()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save_provider(window, cx)
                                })),
                        ),
                ),
        )
        .into_any_element()
    }

    /// Install and sign-in: everything that has to be true before the agent can
    /// answer a prompt.
    fn render_setup(
        &self,
        profile: &ProviderProfile,
        status: Option<&ProviderStatus>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let missing = status
            .map(|status| status.missing.clone())
            .unwrap_or_default();
        let readiness = install::readiness(profile, &missing);
        let job = self
            .job
            .as_ref()
            .filter(|job| job.provider == profile.id)
            .filter(|job| job.kind == JobKind::Install || job.kind == JobKind::SignIn);
        let mut group = v_flex().gap_3();

        match &readiness {
            Readiness::Installable(steps) if !steps.is_empty() => {
                let packages: Vec<String> = steps
                    .iter()
                    .map(|step| format!("{} — {}", step.package, step.purpose))
                    .collect();
                group = group
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().child(format!(
                                "{} {} not installed.",
                                packages.len(),
                                if packages.len() == 1 {
                                    "package is"
                                } else {
                                    "packages are"
                                }
                            )))
                            .children(packages.into_iter().map(|package| {
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(cx.theme().muted_foreground)
                                    .child(package)
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("provider-install")
                                    .icon(IconName::Download)
                                    .label(if job.is_some_and(Job::is_running) {
                                        "Installing…"
                                    } else {
                                        "Install with npm"
                                    })
                                    .small()
                                    .disabled(job.is_some_and(Job::is_running))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.start_job(JobKind::Install, cx)
                                    })),
                            )
                            .when(job.is_some_and(Job::is_running), |row| {
                                row.child(
                                    Button::new("provider-install-stop")
                                        .label("Stop")
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(job) = &this.job {
                                                job.handle.cancel();
                                            }
                                            cx.notify();
                                        })),
                                )
                            }),
                    );
            }
            Readiness::Manual => {
                group = group.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Install this agent yourself, then point the command below at it."),
                );
            }
            _ => {}
        }

        if let Some(error) = status.and_then(|status| status.error.clone()) {
            group = group.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().warning_foreground)
                    .child(error),
            );
        }

        group = group.child(self.render_account(profile, status, cx));

        if let Some(job) = job {
            group = group.child(self.render_job(job, cx));
        }

        GroupBox::new().outline().title("Setup").child(group)
    }

    /// What the provider's own commands say about its account.
    fn render_account(
        &self,
        profile: &ProviderProfile,
        status: Option<&ProviderStatus>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let auth = status.map(|status| status.auth.clone()).unwrap_or_default();
        if auth == AuthState::NotApplicable {
            return v_flex().child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(profile.kind.guidance()),
            );
        }
        let detail = status.and_then(|status| status.auth_detail.clone());
        let (title, line) = match (&auth, profile.kind) {
            (AuthState::SignedIn(account), ProviderKind::Codex) => (
                "ChatGPT account".to_string(),
                format!("Signed in · {account}"),
            ),
            (AuthState::SignedIn(account), _) => {
                ("Credentials".to_string(), format!("{account} stored"))
            }
            (AuthState::SignedOut, ProviderKind::Codex) => (
                "ChatGPT account".to_string(),
                "Not signed in. Anode never stores the credential; the Codex CLI owns it."
                    .to_string(),
            ),
            (AuthState::SignedOut, _) => (
                "Credentials".to_string(),
                "No credentials stored yet.".to_string(),
            ),
            _ => (
                "Account".to_string(),
                "Install the agent to read its sign-in state.".to_string(),
            ),
        };

        let signed_in = auth.is_signed_in();
        let can_sign_in = status.is_some_and(|status| {
            profile
                .kind
                .auth_binary()
                .is_some_and(|binary| crate::toolchain::locate_executable(binary).is_some())
                && status.missing.is_empty()
        });
        let running = self
            .job
            .as_ref()
            .is_some_and(|job| job.kind == JobKind::SignIn && job.is_running());

        let mut actions = h_flex().gap_2();
        match profile.kind {
            ProviderKind::Codex => {
                actions = actions.child(
                    Button::new("codex-sign-in")
                        .icon(IconName::LogIn)
                        .label(if running {
                            "Waiting for browser…"
                        } else if signed_in {
                            "Change account"
                        } else {
                            "Sign in with ChatGPT"
                        })
                        .small()
                        .disabled(running || !can_sign_in)
                        .on_click(
                            cx.listener(|this, _, _, cx| this.start_job(JobKind::SignIn, cx)),
                        ),
                );
                if signed_in {
                    actions = actions
                        .child(
                            Button::new("codex-sign-out")
                                .label("Sign out")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.sign_out(cx))),
                        )
                        .child(
                            Link::new("codex-usage").href(CHATGPT_USAGE_URL).child(
                                h_flex()
                                    .gap_1()
                                    .text_xs()
                                    .child("Manage usage")
                                    .child(Icon::new(IconName::ExternalLink).size_3()),
                            ),
                        );
                }
            }
            ProviderKind::OpenCode => {
                // `opencode auth login` is an interactive picker, so it belongs
                // in a real terminal rather than a captured child process.
                actions = actions.child(
                    Button::new("opencode-sign-in")
                        .icon(IconName::Terminal)
                        .label(if signed_in {
                            "Manage credentials…"
                        } else {
                            "Sign in in Terminal…"
                        })
                        .small()
                        .disabled(!can_sign_in)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_in_terminal("opencode auth login", window, cx)
                        })),
                );
            }
            ProviderKind::Custom => {}
        }

        v_flex()
            .gap_2()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(status_dot(
                                if signed_in {
                                    Tone::Ready
                                } else {
                                    Tone::Attention
                                },
                                cx,
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(line),
                            ),
                    )
                    .when_some(detail.filter(|_| signed_in), |column, detail| {
                        column.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(detail),
                        )
                    }),
            )
            .child(actions)
    }

    fn render_job(&self, job: &Job, cx: &mut Context<Self>) -> impl IntoElement {
        let url = (job.kind == JobKind::SignIn)
            .then(|| install::sign_in_url(&job.lines))
            .flatten();
        v_flex()
            .gap_2()
            .child(
                div()
                    .max_h_40()
                    .overflow_y_scrollbar()
                    .p_2()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().muted)
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child(if job.lines.is_empty() {
                        "Starting…".to_string()
                    } else {
                        job.lines.join("\n")
                    }),
            )
            .when_some(url, |column, url| {
                column.child(
                    h_flex().gap_2().child(
                        Link::new("job-open-url").href(url.clone()).child(
                            h_flex()
                                .gap_1()
                                .text_xs()
                                .child("Open the sign-in page")
                                .child(Icon::new(IconName::ExternalLink).size_3()),
                        ),
                    ),
                )
            })
            .when_some(job.outcome.clone(), |column, outcome| {
                column.child(match outcome {
                    Ok(message) => Alert::success("job-outcome", message).small(),
                    Err(error) => Alert::error("job-outcome", diagnostic_text(&error)).small(),
                })
            })
    }

    fn render_runtime(
        &self,
        profile: &ProviderProfile,
        status: Option<&ProviderStatus>,
        editor: &ProviderEditor,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let resolved = status
            .and_then(|status| status.resolved_path.clone())
            .map(|path| path.display().to_string());
        GroupBox::new().outline().title("Runtime").child(
            v_flex()
                .gap_3()
                .when(profile.kind == ProviderKind::Custom, |column| {
                    column.child(field(
                        "Display name",
                        "The name chat and this list show.",
                        Input::new(&editor.name)
                            .id("provider-name")
                            .aria_label("Display name")
                            .small(),
                        cx,
                    ))
                })
                .child(field(
                    "Command",
                    "Executable name on PATH, or an absolute path.",
                    Input::new(&editor.command)
                        .id("provider-command")
                        .aria_label("Command")
                        .small(),
                    cx,
                ))
                .when_some(resolved, |column, resolved| {
                    column.child(
                        div()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(resolved),
                    )
                })
                .child(field(
                    "Arguments",
                    "A JSON array, passed to the command when ACP starts.",
                    Input::new(&editor.args)
                        .id("provider-arguments")
                        .aria_label("Arguments")
                        .small(),
                    cx,
                )),
        )
    }

    fn render_environment(
        &self,
        editor: &ProviderEditor,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut rows = v_flex().gap_2();
        for row in &editor.env {
            let key = row.row;
            rows = rows.child(
                h_flex()
                    .gap_2()
                    .child(
                        div().w(gpui_kit::rems(13.)).child(
                            Input::new(&row.name)
                                .id(SharedString::from(format!("env-name-{key}")))
                                .aria_label("Variable name")
                                .small()
                                .cleanable(false),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&row.value)
                                .id(SharedString::from(format!("env-value-{key}")))
                                .aria_label("Variable value")
                                .small()
                                .mask_toggle(),
                        ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("env-remove-{key}")))
                            .icon(IconName::Trash)
                            .ghost()
                            .small()
                            .accessibility_label("Remove variable")
                            .tooltip("Remove variable")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(editor) = &mut this.editor {
                                    editor.env.retain(|row| row.row != key);
                                    editor.notice = None;
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        GroupBox::new()
            .outline()
            .title("Environment")
            .child(
                v_flex()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Passed to the agent process. A base URL and API key for an OpenAI- or Anthropic-compatible endpoint go here; values are stored in plain text on disk.",
                            ),
                    )
                    .when(!editor.env.is_empty(), |column| column.child(rows))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("env-add")
                                    .icon(IconName::Plus)
                                    .label("Add variable")
                                    .outline()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Some(editor) = &mut this.editor {
                                            editor.push_env("", "", window, cx);
                                            editor.notice = None;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("env-add-openai")
                                    .label("Add OpenAI-compatible endpoint")
                                    .ghost()
                                    .small()
                                    .tooltip("Adds OPENAI_BASE_URL and OPENAI_API_KEY")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Some(editor) = &mut this.editor {
                                            editor.push_env(
                                                "OPENAI_BASE_URL",
                                                "https://openrouter.ai/api/v1",
                                                window,
                                                cx,
                                            );
                                            editor.push_env("OPENAI_API_KEY", "", window, cx);
                                            editor.notice = None;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn render_agent_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return v_flex().into_any_element();
        };
        let workspace = owner.read(cx);
        let auto_connect = workspace.settings.agent_auto_connect;
        let chats = workspace.agents.saved_chats().len();
        let selected = workspace
            .agents
            .selected_id()
            .map(|id| workspace.agents.provider_name(id));
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .p_4()
            .gap_4()
            .overflow_y_scrollbar()
            .child(
                GroupBox::new().outline().title("Chat").child(
                    v_flex()
                        .gap_3()
                        .child(
                            h_flex()
                                .justify_between()
                                .gap_3()
                                .child(
                                    v_flex()
                                        .gap_0p5()
                                        .child(div().text_sm().child("Current provider"))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(selected.unwrap_or_else(|| {
                                                    "None. Chat stays empty until you choose one."
                                                        .into()
                                                })),
                                        ),
                                )
                                .child(
                                    Button::new("agent-page-providers")
                                        .label("Providers")
                                        .outline()
                                        .small()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.page = Page::Providers;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            h_flex()
                                .justify_between()
                                .gap_3()
                                .child(
                                    v_flex()
                                        .gap_0p5()
                                        .child(div().text_sm().child("Connect automatically"))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(
                                                    "Start the chosen agent when a project opens.",
                                                ),
                                        ),
                                )
                                .child(
                                    Switch::new("agent-auto-connect")
                                        .checked(auto_connect)
                                        .accessibility_label("Connect automatically")
                                        .on_click(cx.listener(
                                            |this, checked: &bool, _, cx| {
                                                let checked = *checked;
                                                if let Some(owner) = this.owner.upgrade() {
                                                    owner.update(cx, |workspace, cx| {
                                                        workspace.settings.agent_auto_connect =
                                                            checked;
                                                        workspace.settings.save();
                                                        cx.notify();
                                                    });
                                                }
                                                cx.notify();
                                            },
                                        )),
                                ),
                        ),
                ),
            )
            .child(
                GroupBox::new().outline().title("History").child(
                    h_flex()
                        .justify_between()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_0p5()
                                .child(div().text_sm().child(format!(
                                    "{chats} saved chat{} in this project",
                                    if chats == 1 { "" } else { "s" }
                                )))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            "Transcripts are kept on this machine. Provider sessions, credentials, and tool grants are not.",
                                        ),
                                ),
                        )
                        .child(
                            Button::new("agent-clear-chats")
                                .label("Clear history…")
                                .ghost()
                                .danger()
                                .small()
                                .disabled(chats == 0)
                                .on_click(cx.listener(|_, _, window, cx| {
                                    let panel = cx.weak_entity();
                                    window.open_alert_dialog(cx, move |alert, _, _| {
                                        let panel = panel.clone();
                                        alert
                                            .title("Clear saved chats?")
                                            .description(
                                                "Every saved transcript in this project is deleted. This cannot be undone.",
                                            )
                                            .ok_text("Clear")
                                            .show_cancel(true)
                                            .on_ok(move |_, _, cx| {
                                                let _ = panel.update(cx, |this, cx| {
                                                    this.clear_chats(cx)
                                                });
                                                true
                                            })
                                    });
                                })),
                        ),
                ),
            )
            .into_any_element()
    }

    fn render_diagnostics_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let diagnostics = self
            .owner
            .upgrade()
            .and_then(|owner| {
                owner
                    .read(cx)
                    .agents
                    .current()
                    .map(|conversation| conversation.diagnostics.join("\n"))
            })
            .unwrap_or_default();
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .p_4()
            .gap_4()
            .overflow_y_scrollbar()
            .child(
                GroupBox::new()
                    .outline()
                    .title(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(div().text_sm().child("Session diagnostics"))
                            .when(!diagnostics.is_empty(), |row| {
                                row.child(
                                    Clipboard::new("copy-diagnostics")
                                        .value(diagnostics.clone())
                                        .tooltip("Copy diagnostics"),
                                )
                            }),
                    )
                    .child(if diagnostics.is_empty() {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Connect an agent to collect transport diagnostics.")
                    } else {
                        div()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(diagnostics)
                    }),
            )
    }

    fn clear_chats(&mut self, cx: &mut Context<Self>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |workspace, cx| {
                workspace.agents.restore_chats(Vec::new());
                workspace.persist_agent_chats();
                cx.notify();
            });
        }
        cx.notify();
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile(cx) else {
            return;
        };
        let outcome = install::sign_out(&profile);
        if let Some(editor) = &mut self.editor {
            match outcome {
                Ok(message) => {
                    editor.notice = Some(message);
                    editor.error = None;
                }
                Err(error) => editor.error = Some(error),
            }
        }
        self.refresh(cx);
        cx.notify();
    }

    fn run_in_terminal(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let command = command.to_string();
        owner.update(cx, |workspace, cx| {
            workspace.show_tool(super::Tool::Terminal, window, cx);
            workspace
                .terminal
                .update(cx, |terminal, cx| terminal.run_command(&command, cx));
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Ready,
    Attention,
    Unavailable,
    Pending,
}

fn status_dot(tone: Tone, cx: &App) -> impl IntoElement {
    let color = match tone {
        Tone::Ready => cx.theme().success,
        Tone::Attention => cx.theme().warning,
        Tone::Unavailable => cx.theme().danger,
        Tone::Pending => cx.theme().muted_foreground,
    };
    div().flex_shrink_0().size_1p5().rounded_full().bg(color)
}

/// One short line describing where a provider stands. The words carry the
/// meaning; the dot only repeats it.
fn status_summary(
    profile: &ProviderProfile,
    status: Option<&ProviderStatus>,
    probing: bool,
) -> (String, Tone) {
    let Some(status) = status else {
        return if probing {
            ("Checking…".into(), Tone::Pending)
        } else {
            ("Not checked".into(), Tone::Pending)
        };
    };
    if !status.is_available() {
        return if status.missing.is_empty() {
            ("Command not found".into(), Tone::Unavailable)
        } else {
            ("Not installed".into(), Tone::Unavailable)
        };
    }
    if !profile.enabled {
        return ("Hidden from chat".into(), Tone::Pending);
    }
    match &status.auth {
        AuthState::SignedIn(account) => (format!("Ready · {account}"), Tone::Ready),
        AuthState::NotApplicable => ("Ready".into(), Tone::Ready),
        AuthState::SignedOut => ("Needs sign-in".into(), Tone::Attention),
        AuthState::Unknown => ("Installed".into(), Tone::Attention),
    }
}

fn kind_icon(kind: ProviderKind) -> IconName {
    match kind {
        ProviderKind::Codex => IconName::Bot,
        ProviderKind::OpenCode => IconName::SquareTerminal,
        ProviderKind::Custom => IconName::Plug,
    }
}

/// A labelled form row: label, control, then the help that explains it.
fn field(
    label: &'static str,
    description: &'static str,
    control: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_xs().font_weight(FontWeight::MEDIUM).child(label))
        .child(control)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description),
        )
}

impl Focusable for SettingsScreen {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.page {
            Page::Providers => h_flex()
                .items_stretch()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(self.render_provider_list(cx))
                .child(self.render_provider_detail(cx))
                .into_any_element(),
            Page::Agent => self.render_agent_page(cx).into_any_element(),
            Page::Diagnostics => self.render_diagnostics_page(cx).into_any_element(),
        };
        let checking = self.probing;
        h_flex()
            .size_full()
            .min_h_0()
            .items_stretch()
            .track_focus(&self.focus)
            .bg(cx.theme().background)
            .child(self.render_navigation(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .justify_between()
                            .gap_2()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .min_w_0()
                                    // The trail names where the user is, the
                                    // way the sidebar names where they can go.
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Settings"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("/"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(self.page.label()),
                                    )
                                    .when(checking && self.page == Page::Providers, |row| {
                                        row.child(Spinner::new().xsmall()).child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Checking agents…"),
                                        )
                                    }),
                            )
                            .when(self.page == Page::Providers, |row| {
                                row.child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Button::new("settings-refresh")
                                                .icon(IconName::RefreshCw)
                                                .label("Check again")
                                                .ghost()
                                                .small()
                                                .disabled(checking)
                                                .on_click(
                                                    cx.listener(|this, _, _, cx| this.refresh(cx)),
                                                ),
                                        )
                                        .child(
                                            Button::new("settings-add-provider")
                                                .icon(IconName::Plus)
                                                .label("Add provider")
                                                .outline()
                                                .small()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.add_provider(window, cx)
                                                })),
                                        ),
                                )
                            }),
                    )
                    .child(content),
            )
    }
}
