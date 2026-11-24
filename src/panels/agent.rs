//! The side chat.
//!
//! The workspace owns connections, transcripts, and grants; this panel is the
//! presentation. The agent and the model are both chosen in the composer — the
//! agent from a short menu, the model from a searchable palette grouped by the
//! upstream provider each model comes from — so there is no separate place to
//! pick an ACP agent.
use crate::{
    agent::{diagnostic_text, MessageKind, ProviderId, ProviderKind, State},
    workspace_state::Workspace,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, IndexPath, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, ParentElement as _, Render, SharedString, Styled, Subscription, WeakEntity,
    Window,
};
use serde_json::Value;

#[derive(Clone, Copy)]
struct SlashCommand {
    command: &'static str,
    description: &'static str,
    prompt: Option<&'static str>,
    attach: Option<AttachSource>,
    clear: bool,
}

const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        command: "/new-project",
        description: "Create a new board project and open it",
        prompt: Some("List available Anode templates, create a new project for my board (with git initialized), and open it."),
        attach: None,
        clear: false,
    },
    SlashCommand {
        command: "/debug-board",
        description: "Inspect Output, REPL, and board status",
        prompt: Some("Inspect my connected board status, recent Output diagnostics, and REPL logs, and help me troubleshoot."),
        attach: None,
        clear: false,
    },
    SlashCommand {
        command: "/output",
        description: "Attach Output log and diagnose errors",
        prompt: Some("Analyze the attached Output log for compiler or device errors and suggest fixes."),
        attach: Some(AttachSource::Output),
        clear: false,
    },
    SlashCommand {
        command: "/repl",
        description: "Attach REPL output and analyze tracebacks",
        prompt: Some("Analyze the attached REPL output and tracebacks and suggest fixes."),
        attach: Some(AttachSource::Repl),
        clear: false,
    },
    SlashCommand {
        command: "/terminal",
        description: "Attach Terminal output and diagnose issues",
        prompt: Some("Analyze the attached Terminal output and suggest next steps."),
        attach: Some(AttachSource::Terminal),
        clear: false,
    },
    SlashCommand {
        command: "/clear",
        description: "Start a new chat session",
        prompt: None,
        attach: None,
        clear: true,
    },
];

#[derive(Clone, Copy)]
enum AttachSource {
    Selection,
    Output,
    Repl,
    Terminal,
}

impl AttachSource {
    fn label(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Output => "Output",
            Self::Repl => "REPL",
            Self::Terminal => "Terminal",
        }
    }
}

// -- Model options ----------------------------------------------------------

/// One model an agent offers, as the palette shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelChoice {
    value: String,
    label: String,
}

/// Models that come from the same upstream provider. OpenCode, for example,
/// serves both `opencode` (Zen) and `opencode-go` models, and those are
/// different products that should not sit in one undifferentiated list.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelGroup {
    label: String,
    choices: Vec<ModelChoice>,
}

/// Turn an ACP `model` option into groups. OpenCode Go leads, followed by Zen,
/// then the current provider and the remaining providers alphabetically.
///
/// A value carries its upstream provider as a `provider/model` prefix. Codex
/// leaves its own models bare (`gpt-6.1-sol`, `autorouter`) and prefixes the
/// rest (`anthropic/claude-opus-5-5`), so bare values are grouped under
/// `agent`, the name of the agent serving them.
fn group_models(choices: &[Value], current: &str, agent: &str) -> Vec<ModelGroup> {
    let mut flat: Vec<(String, ModelChoice)> = Vec::new();
    let push = |entry: &Value, flat: &mut Vec<(String, ModelChoice)>| {
        let Some(value) = entry["value"].as_str() else {
            return;
        };
        let (slug, rest) = match value.split_once('/') {
            Some((slug, rest)) => (slug.to_string(), rest.to_string()),
            None => (String::new(), value.to_string()),
        };
        let name = entry["name"].as_str().unwrap_or(value);
        // Agents commonly repeat the provider slug in the display name.
        let label = name
            .strip_prefix(&format!("{slug}/"))
            .unwrap_or(name)
            .to_string();
        let label = if label.trim().is_empty() { rest } else { label };
        flat.push((
            slug,
            ModelChoice {
                value: value.to_string(),
                label,
            },
        ));
    };
    for entry in choices {
        if let Some(nested) = entry["options"].as_array() {
            for entry in nested {
                push(entry, &mut flat);
            }
        } else {
            push(entry, &mut flat);
        }
    }

    let current_slug = current.split_once('/').map(|(slug, _)| slug).unwrap_or("");
    let mut keys: Vec<String> = Vec::new();
    for (slug, _) in &flat {
        if !keys.contains(slug) {
            keys.push(slug.clone());
        }
    }
    keys.sort_by_key(|slug| {
        let priority = match slug.as_str() {
            "opencode-go" => 0,
            "opencode" => 1,
            _ if slug == current_slug => 2,
            _ => 3,
        };
        (priority, slug.to_lowercase())
    });
    keys.into_iter()
        .map(|slug| ModelGroup {
            label: group_label(&slug, agent),
            choices: flat
                .iter()
                .filter(|(key, _)| *key == slug)
                .map(|(_, choice)| choice.clone())
                .collect(),
        })
        .collect()
}

/// Keep the billing provider visible even when Go and Zen offer the same model.
fn selected_model_label(groups: &[ModelGroup], current: &str) -> String {
    groups
        .iter()
        .find_map(|group| {
            group
                .choices
                .iter()
                .find(|choice| choice.value == current)
                .map(|choice| {
                    if current.starts_with("opencode-go/") || current.starts_with("opencode/") {
                        format!("{} · {}", group.label, choice.label)
                    } else {
                        choice.label.clone()
                    }
                })
        })
        .unwrap_or_else(|| {
            if current.is_empty() {
                "Model".into()
            } else {
                current.into()
            }
        })
}

/// A readable heading for an upstream provider slug.
fn group_label(slug: &str, agent: &str) -> String {
    match slug {
        "" if agent.trim().is_empty() => "Models".into(),
        "" => agent.trim().to_string(),
        "opencode" => "OpenCode Zen".into(),
        "opencode-go" => "OpenCode Go".into(),
        "openrouter" => "OpenRouter".into(),
        "amazon-bedrock" => "Amazon Bedrock".into(),
        "openai" => "OpenAI".into(),
        "anthropic" => "Anthropic".into(),
        "google" => "Google".into(),
        other => other
            .split(['-', '_'])
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// The session's `model` option: its id, current value, and raw choices.
fn model_option(options: &Value) -> Option<(String, String, Vec<Value>)> {
    let list = options["configOptions"].as_array()?;
    let option = list
        .iter()
        .find(|option| option["id"] == "model" || option["category"] == "model")?;
    Some((
        option["id"].as_str()?.to_string(),
        option["currentValue"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        option["options"].as_array().cloned().unwrap_or_default(),
    ))
}

/// Session options other than the model, which are short enough for a menu.
fn other_options(options: &Value) -> Vec<Value> {
    options["configOptions"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|option| option["type"] == "select")
                .filter(|option| option["id"] != "model" && option["category"] != "model")
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn relative_time(millis: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let seconds = now.saturating_sub(millis) / 1000;
    match seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}

// -- Panel ------------------------------------------------------------------

pub(super) struct AgentPanel {
    owner: WeakEntity<Workspace>,
    draft: Entity<TextareaState>,
    scroller: Entity<MessageScrollerState>,
    models: Entity<CommandState>,
    model_picker_open: bool,
    history_open: bool,
    binding: String,
    syncing: bool,
    subscriptions: Vec<Subscription>,
    expanded_tools: std::collections::HashSet<u64>,
}

impl AgentPanel {
    pub(super) fn new(
        owner: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(2)
                .submit_on_enter(true)
                .placeholder("Message your agent…")
        });
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        let models = cx.new(|cx| CommandState::new(window, cx));
        let workspace = owner.upgrade().expect("workspace exists");
        let observation = cx.observe_in(&workspace, window, |this, owner, window, cx| {
            let workspace = owner.read(cx);
            let key = format!(
                "{:?}:{:?}",
                workspace.agents.selected_id(),
                workspace.project_path
            );
            let draft = workspace
                .agents
                .current()
                .map(|c| c.draft.clone())
                .unwrap_or_default();
            let count = workspace
                .agents
                .current()
                .map(|c| c.messages.len())
                .unwrap_or(0);
            let draft_changed = this.draft.read(cx).value().as_ref() != draft;
            if this.binding != key || draft_changed {
                this.expanded_tools.clear();
                this.binding = key;
                this.syncing = true;
                this.draft
                    .update(cx, |state, cx| state.set_value(draft, window, cx));
                this.syncing = false;
                this.scroller.update(cx, |state, cx| state.reset(count, cx));
            } else {
                this.scroller.update(cx, |state, cx| {
                    let old = state.item_count();
                    if count > old {
                        state.append(count - old, cx);
                    } else if count < old {
                        state.reset(count, cx);
                    } else if count > 0 {
                        state.remeasure(cx);
                    }
                });
            }
            cx.notify();
        });
        let changes = cx.subscribe_in(&draft, window, |this, input, event, window, cx| {
            if this.syncing {
                return;
            }
            if matches!(event, InputEvent::Change) {
                let value = input.read(cx).value().to_string();
                if let Some(owner) = this.owner.upgrade() {
                    owner.update(cx, |workspace, _| {
                        if let Some(c) = workspace.agents.current_mut() {
                            c.draft = value;
                        }
                    });
                }
                cx.notify();
            } else if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                this.send(window, cx);
            }
        });
        Self {
            owner,
            draft,
            scroller,
            models,
            model_picker_open: false,
            history_open: false,
            binding: String::new(),
            syncing: false,
            subscriptions: vec![observation, changes],
            expanded_tools: Default::default(),
        }
    }

    fn attach(&mut self, source: AttachSource, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let workspace = owner.read(cx);
        let text = match source {
            AttachSource::Selection => workspace.editor.read(cx).selected_value().to_string(),
            AttachSource::Output => workspace.log_as_text(),
            AttachSource::Repl => workspace.repl_output.clone(),
            AttachSource::Terminal => workspace
                .project_path
                .as_deref()
                .and_then(|root| workspace.terminal.read(cx).agent_output(root))
                .unwrap_or_default(),
        };
        if text.trim().is_empty() {
            return;
        }
        let text = diagnostic_text(&text);
        let current = self.draft.read(cx).value().to_string();
        let value = if current.trim().is_empty() {
            format!(
                "Attached {} (workspace data):\n```\n{text}\n```",
                source.label()
            )
        } else {
            format!(
                "{current}\n\nAttached {} (workspace data):\n```\n{text}\n```",
                source.label()
            )
        };
        self.draft
            .update(cx, |state, cx| state.set_value(value.clone(), window, cx));
        owner.update(cx, |workspace, cx| {
            if let Some(c) = workspace.agents.current_mut() {
                c.draft = value;
            }
            cx.notify();
        });
    }

    fn execute_slash_command(
        &mut self,
        cmd: SlashCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.draft
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.syncing = false;
        if cmd.clear {
            self.new_chat(cx);
            return;
        }
        if let Some(source) = cmd.attach {
            self.attach(source, window, cx);
        }
        if let Some(prompt) = cmd.prompt {
            let current = self.draft.read(cx).value().to_string();
            let next = if current.trim().is_empty() {
                prompt.to_string()
            } else {
                format!("{prompt}\n\n{current}")
            };
            self.apply_starter_prompt(&next, window, cx);
        }
    }

    fn apply_starter_prompt(&mut self, prompt: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing = true;
        self.draft.update(cx, |state, cx| {
            state.set_value(prompt.to_string(), window, cx)
        });
        self.syncing = false;
        let ready = self.owner.upgrade().is_some_and(|owner| {
            owner
                .read(cx)
                .agents
                .current()
                .is_some_and(|c| c.state == State::Ready)
        });
        if ready {
            self.send(window, cx);
        } else if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |workspace, cx| {
                if let Some(c) = workspace.agents.current_mut() {
                    c.draft = prompt.to_string();
                }
                cx.notify();
            });
            self.draft.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.draft.read(cx).value().to_string();
        let sent = self.owner.upgrade().is_some_and(|owner| {
            owner.update(cx, |workspace, cx| {
                let context = workspace.agent_context();
                let sent = workspace.agents.current_mut().is_some_and(|c| {
                    c.draft = text;
                    c.send(context)
                });
                if sent {
                    workspace.persist_agent_chats();
                }
                cx.notify();
                sent
            })
        });
        if sent {
            self.syncing = true;
            self.draft
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.syncing = false;
        }
    }

    fn new_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |workspace, cx| {
                if workspace.agents.new_chat() {
                    workspace.persist_agent_chats();
                }
                cx.notify();
            });
        }
        cx.notify();
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |workspace, cx| {
                workspace.show_settings(window, cx);
            });
        }
    }

    fn select_provider(&mut self, id: ProviderId, cx: &mut Context<Self>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |workspace, cx| {
                if workspace.agents.select(&id) {
                    workspace.settings.agent_provider = Some(id.as_str().to_string());
                    workspace.settings.save();
                }
                cx.notify();
            });
        }
        cx.notify();
    }

    fn set_option(&mut self, id: String, value: String, cx: &mut Context<Self>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |workspace, cx| {
                if let Some(conversation) = workspace.agents.current_mut() {
                    conversation.set_option(id, value);
                }
                cx.notify();
            });
        }
        cx.notify();
    }

    // -- Header --------------------------------------------------------------

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return h_flex();
        };
        let workspace = owner.read(cx);
        let name = workspace
            .agents
            .selected_id()
            .map(|id| workspace.agents.provider_name(id));
        let conversation = workspace.agents.current();
        let state = conversation.map(|c| c.state).unwrap_or(State::Disconnected);
        let busy = conversation.is_some_and(|c| c.is_busy());
        let messages = conversation.map(|c| c.messages.len()).unwrap_or(0);
        let connected = !matches!(state, State::Disconnected | State::Failed);
        let has_provider = name.is_some();

        let status = match state {
            State::Ready => Tag::success().small().child("Ready"),
            State::Responding | State::Cancelling => Tag::info().small().child(state.label()),
            State::Starting | State::Initializing | State::Opening => {
                Tag::secondary().small().child(state.label())
            }
            State::Failed => Tag::danger().small().child("Failed"),
            State::Disconnected => Tag::secondary().small().child("Not connected"),
        };

        h_flex()
            .flex_shrink_0()
            .px_3()
            .py_2()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(name.clone().unwrap_or_else(|| "Chat".into())),
                    )
                    .when(has_provider, |row| row.child(status))
                    .when(
                        matches!(
                            state,
                            State::Starting | State::Initializing | State::Opening
                        ),
                        |row| row.child(Spinner::new().xsmall()),
                    ),
            )
            .when(has_provider && state == State::Disconnected, |header| {
                header.child(
                    Button::new("agent-connect")
                        .label("Connect")
                        .outline()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(owner) = this.owner.upgrade() {
                                owner.update(cx, |workspace, cx| {
                                    workspace.agents.connect();
                                    cx.notify();
                                });
                            }
                        })),
                )
            })
            .child(self.render_history(cx))
            .child(
                Button::new("agent-new")
                    .icon(IconName::SquarePen)
                    .ghost()
                    .xsmall()
                    .accessibility_label("New chat")
                    .tooltip("New chat")
                    .disabled(busy || messages == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.new_chat(cx))),
            )
            .child(
                Button::new("agent-menu")
                    .icon(IconName::Ellipsis)
                    .ghost()
                    .xsmall()
                    .accessibility_label("Chat actions")
                    .dropdown_menu({
                        let panel = cx.weak_entity();
                        move |menu, _, _| {
                            let connection_panel = panel.clone();
                            let settings_panel = panel.clone();
                            menu.item(
                                PopupMenuItem::new(if connected {
                                    "Disconnect"
                                } else {
                                    "Connect"
                                })
                                .disabled(!has_provider)
                                .on_click(move |_, _, cx| {
                                    let _ = connection_panel.update(cx, |this, cx| {
                                        if let Some(owner) = this.owner.upgrade() {
                                            owner.update(cx, |workspace, cx| {
                                                if connected {
                                                    workspace.agents.disconnect();
                                                } else {
                                                    workspace.agents.connect();
                                                }
                                                cx.notify();
                                            });
                                        }
                                        cx.notify();
                                    });
                                }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Settings").on_click(move |_, window, cx| {
                                    let _ = settings_panel
                                        .update(cx, |this, cx| this.open_settings(window, cx));
                                }),
                            )
                        }
                    }),
            )
    }

    fn render_history(&self, cx: &Context<Self>) -> impl IntoElement {
        let chats = self
            .owner
            .upgrade()
            .map(|owner| owner.read(cx).agents.saved_chats())
            .unwrap_or_default();
        let names: Vec<String> = self
            .owner
            .upgrade()
            .map(|owner| {
                let workspace = owner.read(cx);
                chats
                    .iter()
                    .map(|chat| workspace.agents.provider_name(&chat.provider))
                    .collect()
            })
            .unwrap_or_default();
        let panel = cx.weak_entity();
        Popover::new("agent-history")
            .open(self.history_open)
            .on_open_change(cx.listener(|this, open: &bool, _, cx| {
                this.history_open = *open;
                cx.notify();
            }))
            .trigger(
                Button::new("agent-history-trigger")
                    .icon(IconName::MessagesSquare)
                    .ghost()
                    .xsmall()
                    .accessibility_label("Chat history")
                    .tooltip("Chat history"),
            )
            .content(move |_, _, cx| {
                let mut list = v_flex()
                    .w(gpui_kit::rems(20.))
                    .max_h(gpui_kit::rems(22.))
                    .gap_1()
                    .overflow_y_scrollbar();
                if chats.is_empty() {
                    return list.child(
                        div()
                            .p_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("No saved chats in this project yet."),
                    );
                }
                list = list.child(
                    div()
                        .flex_shrink_0()
                        .px_2()
                        .pt_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Saved chats"),
                );
                for (chat, name) in chats.iter().zip(names.iter()) {
                    let open_id = chat.id.clone();
                    let delete_id = chat.id.clone();
                    let open_panel = panel.clone();
                    let delete_panel = panel.clone();
                    let subtitle = format!("{name} · {}", relative_time(chat.updated_at));
                    list = list.child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_1()
                            .child(
                                Button::new(SharedString::from(format!("chat-{}", chat.id)))
                                    .ghost()
                                    .small()
                                    .h_auto()
                                    .py_2()
                                    .flex_1()
                                    .min_w_0()
                                    .accessibility_label(format!("{} · {subtitle}", chat.title))
                                    .child(
                                        v_flex()
                                            .w_full()
                                            .min_w_0()
                                            .items_start()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .w_full()
                                                    .truncate()
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(chat.title.clone()),
                                            )
                                            .child(
                                                div()
                                                    .w_full()
                                                    .truncate()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(subtitle),
                                            ),
                                    )
                                    .on_click(move |_, _, cx| {
                                        let _ = open_panel.update(cx, |this, cx| {
                                            this.history_open = false;
                                            if let Some(owner) = this.owner.upgrade() {
                                                owner.update(cx, |workspace, cx| {
                                                    if workspace.agents.restore_chat(&open_id) {
                                                        workspace.persist_agent_chats();
                                                    }
                                                    cx.notify();
                                                });
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Button::new(SharedString::from(format!("chat-delete-{}", chat.id)))
                                    .icon(IconName::Trash)
                                    .ghost()
                                    .xsmall()
                                    .accessibility_label("Delete chat")
                                    .tooltip("Delete chat")
                                    .on_click(move |_, _, cx| {
                                        let _ = delete_panel.update(cx, |this, cx| {
                                            if let Some(owner) = this.owner.upgrade() {
                                                owner.update(cx, |workspace, cx| {
                                                    if workspace.agents.delete_chat(&delete_id) {
                                                        workspace.persist_agent_chats();
                                                    }
                                                    cx.notify();
                                                });
                                            }
                                            cx.notify();
                                        });
                                    }),
                            ),
                    );
                }
                list
            })
    }

    // -- Composer ------------------------------------------------------------

    fn render_agent_picker(&self, cx: &Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div();
        };
        let workspace = owner.read(cx);
        let selected = workspace.agents.selected_id().cloned();
        let label = selected
            .as_ref()
            .map(|id| workspace.agents.provider_name(id))
            .unwrap_or_else(|| "Choose agent".into());
        let kind = selected
            .as_ref()
            .and_then(|id| workspace.agents.registry().get(id))
            .map(|profile| profile.kind)
            .unwrap_or(ProviderKind::Custom);
        let registry = workspace.agents.registry().clone();
        let panel = cx.weak_entity();
        div().child(
            Button::new("agent-provider")
                .icon(match kind {
                    ProviderKind::Codex => IconName::Bot,
                    ProviderKind::OpenCode => IconName::SquareTerminal,
                    ProviderKind::Custom => IconName::Plug,
                })
                .label(label)
                .outline()
                .rounded_full()
                .xsmall()
                .dropdown_menu(move |mut menu, _, _| {
                    let mut any = false;
                    for profile in registry.all() {
                        if !profile.enabled {
                            continue;
                        }
                        // Resolving here, rather than every frame, keeps the
                        // filesystem lookup to the moment the menu opens.
                        let installed = profile.resolve().is_ok();
                        let id = profile.id.clone();
                        let panel = panel.clone();
                        any = true;
                        menu = menu.item(
                            PopupMenuItem::new(if installed {
                                profile.name.clone()
                            } else {
                                format!("{} — not installed", profile.name)
                            })
                            .checked(selected.as_ref() == Some(&profile.id))
                            .disabled(!installed)
                            .on_click(move |_, _, cx| {
                                let _ = panel
                                    .update(cx, |this, cx| this.select_provider(id.clone(), cx));
                            }),
                        );
                    }
                    if !any {
                        menu = menu.item(PopupMenuItem::new("No providers").disabled(true));
                    }
                    let panel = panel.clone();
                    menu.separator()
                        .item(
                            PopupMenuItem::new("Settings").on_click(move |_, window, cx| {
                                let _ = panel.update(cx, |this, cx| this.open_settings(window, cx));
                            }),
                        )
                }),
        )
    }

    fn render_model_picker(&self, cx: &Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div();
        };
        let workspace = owner.read(cx);
        let conversation = workspace.agents.current();
        let ready = conversation.is_some_and(|c| c.state == State::Ready);
        let Some((option_id, current, choices)) =
            conversation.and_then(|c| model_option(&c.options))
        else {
            return div();
        };
        let agent = workspace
            .agents
            .selected_id()
            .map(|id| workspace.agents.provider_name(id))
            .unwrap_or_default();
        let groups = group_models(&choices, &current, &agent);
        let current_label = selected_model_label(&groups, &current);
        let state = self.models.clone();
        let panel = cx.weak_entity();
        let menu_groups = groups.clone();
        div().child(
            Popover::new("agent-model-picker")
                .open(self.model_picker_open)
                .on_open_change(cx.listener(|this, open: &bool, _, cx| {
                    this.model_picker_open = *open;
                    cx.notify();
                }))
                .trigger(
                    Button::new("agent-model-trigger")
                        .icon(IconName::Sparkles)
                        .label(current_label)
                        .outline()
                        .rounded_full()
                        .xsmall()
                        .disabled(!ready)
                        .accessibility_label("Model"),
                )
                .content(move |_, _, _| {
                    let groups = menu_groups.clone();
                    let confirm_groups = menu_groups.clone();
                    let option_id = option_id.clone();
                    let panel = panel.clone();
                    let mut command = Command::new(&state)
                        .placeholder("Search models…")
                        .max_h(gpui_kit::rems(22.))
                        .bordered(false)
                        .on_confirm(move |path: IndexPath, _, cx| {
                            let Some(choice) = confirm_groups
                                .get(path.section)
                                .and_then(|group| group.choices.get(path.row))
                            else {
                                return;
                            };
                            let value = choice.value.clone();
                            let option_id = option_id.clone();
                            let _ = panel.update(cx, |this, cx| {
                                this.model_picker_open = false;
                                this.set_option(option_id, value, cx);
                            });
                        });
                    for group in groups {
                        command =
                            command.group(CommandGroup::new().label(group.label.clone()).items(
                                group.choices.into_iter().map(|choice| {
                                    CommandItem::new()
                                        .label(choice.label.clone())
                                        .keywords([choice.value.clone()])
                                        .checked(choice.value == current)
                                }),
                            ));
                    }
                    div().w(gpui_kit::rems(22.)).child(command)
                }),
        )
    }

    fn render_option_buttons(&self, cx: &Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return h_flex();
        };
        let workspace = owner.read(cx);
        let conversation = workspace.agents.current();
        let ready = conversation.is_some_and(|c| c.state == State::Ready);
        let options = conversation
            .map(|c| other_options(&c.options))
            .unwrap_or_default();
        let mut row = h_flex().gap_1().flex_wrap();
        for option in options {
            let Some(id) = option["id"].as_str().map(str::to_string) else {
                continue;
            };
            let current = option["currentValue"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let mut choices = Vec::new();
            for entry in option["options"].as_array().into_iter().flatten() {
                if let Some(group) = entry["options"].as_array() {
                    choices.extend(group.iter().cloned());
                } else {
                    choices.push(entry.clone());
                }
            }
            let current_name = choices
                .iter()
                .find(|choice| choice["value"] == current)
                .and_then(|choice| choice["name"].as_str())
                .unwrap_or(&current)
                .to_string();
            let label = if option["category"] == "thought_level" {
                format!(
                    "{} · {current_name}",
                    option["name"].as_str().unwrap_or("Effort")
                )
            } else {
                current_name
            };
            let panel = cx.weak_entity();
            row = row.child(
                Button::new(SharedString::from(format!("agent-option-{id}")))
                    .label(label)
                    .outline()
                    .rounded_full()
                    .xsmall()
                    .disabled(!ready)
                    .accessibility_label(
                        option["name"]
                            .as_str()
                            .unwrap_or("Session option")
                            .to_string(),
                    )
                    .dropdown_menu(move |mut menu, _, _| {
                        menu = menu.scrollable(true);
                        for choice in &choices {
                            let Some(value) = choice["value"].as_str().map(str::to_string) else {
                                continue;
                            };
                            let label = choice["name"].as_str().unwrap_or(&value).to_string();
                            let panel = panel.clone();
                            let id = id.clone();
                            let selected = value == current;
                            menu = menu.item(PopupMenuItem::new(label).checked(selected).on_click(
                                move |_, _, cx| {
                                    let _ = panel.update(cx, |this, cx| {
                                        this.set_option(id.clone(), value.clone(), cx)
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            );
        }
        row
    }
}

impl EventEmitter<PanelEvent> for AgentPanel {}
impl Focusable for AgentPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.draft.read(cx).focus_handle(cx)
    }
}
impl BasePanel for AgentPanel {
    fn panel_name(&self) -> &'static str {
        super::Tool::Agent.registry_name()
    }
    fn visible(&self, cx: &App) -> bool {
        self.owner
            .upgrade()
            .is_none_or(|owner| owner.read(cx).is_tool_visible(super::Tool::Agent))
    }
}
impl Panel for AgentPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Agent"
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some("Agent".into())
    }
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
    /// The panel draws its own header, so a second one would be a duplicate
    /// boundary.
    fn title_bar(&self, _: &App) -> bool {
        false
    }
}

impl Render for AgentPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return v_flex();
        };
        let workspace = owner.read(cx);
        let provider_name = workspace
            .agents
            .selected_id()
            .map(|id| workspace.agents.provider_name(id));
        let auto_connect = workspace.settings.agent_auto_connect;
        let project = workspace.project_name.clone();
        let board = workspace
            .active_board()
            .map(|b| b.name)
            .unwrap_or("No board");
        let runtime = workspace.active_runtime().label();
        let conversation = workspace.agents.current();
        let state = conversation.map(|c| c.state).unwrap_or(State::Disconnected);
        let busy = conversation.is_some_and(|c| c.is_busy());
        let message_count = conversation.map(|c| c.messages.len()).unwrap_or(0);
        let should_auto_connect = auto_connect
            && provider_name.is_some()
            && state == State::Disconnected
            && conversation.is_none_or(|c| !c.user_disconnected && !c.auto_attempted);

        let mut header = v_flex().flex_shrink_0().child(self.render_header(cx));

        if state == State::Failed {
            header = header.child(
                h_flex()
                    .mx_3()
                    .my_2()
                    .gap_2()
                    .p_2()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().secondary)
                    .child(
                        div()
                            .text_xs()
                            .flex_1()
                            .child("Couldn’t connect. Check the agent's setup or sign-in."),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("agent-error-settings")
                                    .label("Settings")
                                    .ghost()
                                    .xsmall()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_settings(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("agent-retry")
                                    .label("Reconnect")
                                    .outline()
                                    .xsmall()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(owner) = this.owner.upgrade() {
                                            owner.update(cx, |workspace, cx| {
                                                workspace.agents.connect();
                                                cx.notify();
                                            });
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }

        // -- Conversation, or the state that stands in for it ----------------
        let chat_owner = self.owner.clone();
        let panel = cx.weak_entity();
        let agent_label = provider_name.clone().unwrap_or_else(|| "Agent".into());
        let conversation_area = if provider_name.is_none() {
            v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .p_6()
                .gap_2()
                .child(Icon::new(IconName::Plug).size_5())
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Choose an agent"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_center()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Anode talks to a local coding agent over ACP. Install and sign in to one, then pick it here.",
                        ),
                )
                .child(
                    Button::new("agent-empty-settings")
                        .label("Open settings")
                        .outline()
                        .small()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings(window, cx)
                        })),
                )
                .into_any_element()
        } else if message_count == 0 {
            v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .p_6()
                .gap_2()
                .child(Icon::new(IconName::Sparkles).size_5())
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("What would you like to build?"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Create a project, work on code, or debug your board."),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(
                            Button::new("starter-create")
                                .label("Create a project")
                                .outline()
                                .small()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_starter_prompt(
                                        "List available Anode templates, create a new project for my board (with git initialized), and open it.",
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("starter-debug")
                                .label("Debug my board")
                                .outline()
                                .small()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_starter_prompt(
                                        "Inspect recent Output diagnostics and REPL logs for any errors or tracebacks, and suggest a fix.",
                                        window,
                                        cx,
                                    );
                                })),
                        )
                )
                .into_any_element()
        } else {
            MessageScroller::new(
                "agent-conversation",
                self.scroller.clone(),
                move |index, _, cx| {
                    let mut row = v_flex().px_3().py_2().min_w_0();
                    let Some(owner) = chat_owner.upgrade() else {
                        return row;
                    };
                    let workspace = owner.read(cx);
                    let Some(conversation) = workspace.agents.current() else {
                        return row;
                    };
                    let Some(message) = conversation.messages.get(index) else {
                        return row;
                    };
                    let is_last_agent = index + 1 == conversation.messages.len()
                        && conversation.is_busy()
                        && message.kind == MessageKind::Agent;

                    match message.kind {
                        MessageKind::User => {
                            row = row
                                .items_end()
                                .gap_1()
                                .child(
                                    div()
                                        .max_w_full()
                                        .px_3()
                                        .py_2()
                                        .rounded(cx.theme().radius_lg)
                                        .bg(cx.theme().secondary)
                                        .text_sm()
                                        .child(message.text.clone()),
                                )
                                .child(
                                    h_flex().justify_end().gap_1().child(
                                        Clipboard::new(("copy-user", message.id))
                                            .value(message.text.clone())
                                            .tooltip("Copy message"),
                                    ),
                                );
                        }
                        MessageKind::Agent => {
                            row = row.gap_1().child(
                                h_flex()
                                    .items_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .p_1()
                                            .rounded_full()
                                            .bg(cx.theme().secondary)
                                            .child(Icon::new(IconName::Sparkles).size_3()),
                                    )
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_1()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child(agent_label.clone()),
                                                    )
                                                    .when(is_last_agent, |h| {
                                                        h.child(Spinner::new().xsmall())
                                                    }),
                                            )
                                            .child(TextView::markdown(
                                                ("agent-message", message.id),
                                                message.text.clone(),
                                            ))
                                            .child(
                                                h_flex().gap_1().child(
                                                    Clipboard::new(("copy-agent", message.id))
                                                        .value(message.text.clone())
                                                        .tooltip("Copy response"),
                                                ),
                                            ),
                                    ),
                            );
                        }
                        MessageKind::ToolCall => {
                            let id = message.id;
                            let expanded = panel
                                .upgrade()
                                .is_some_and(|p| p.read(cx).expanded_tools.contains(&id));
                            let toggle = panel.clone();
                            let status = message.status.as_str();
                            let icon = match status {
                                "in_progress" | "pending" => {
                                    Spinner::new().xsmall().into_any_element()
                                }
                                "completed" => {
                                    Icon::new(IconName::Check).size_3().into_any_element()
                                }
                                "failed" => Icon::new(IconName::TriangleAlert)
                                    .size_3()
                                    .into_any_element(),
                                _ => match message.tool_kind.as_str() {
                                    "execute" => {
                                        Icon::new(IconName::Terminal).size_3().into_any_element()
                                    }
                                    "read" | "edit" => {
                                        Icon::new(IconName::FileCode).size_3().into_any_element()
                                    }
                                    _ => Icon::new(IconName::Wrench).size_3().into_any_element(),
                                },
                            };
                            let status_badge = match status {
                                "completed" => Tag::success().small().child("Done"),
                                "failed" => Tag::danger().small().child("Failed"),
                                "in_progress" | "pending" => Tag::info().small().child("Running"),
                                other if !other.is_empty() => {
                                    Tag::secondary().small().child(other.to_string())
                                }
                                _ => Tag::secondary().small().child("Tool"),
                            };
                            let has_detail = !message.text.trim().is_empty();
                            row = row.gap_1().child(
                                v_flex()
                                    .p_2()
                                    .gap_1()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .bg(cx.theme().background)
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .child(icon)
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .truncate()
                                                            .child(message.title.clone()),
                                                    ),
                                            )
                                            .child(h_flex().gap_1().child(status_badge).when(
                                                has_detail,
                                                |h| {
                                                    h.child(
                                                        Button::new(("agent-tool", id))
                                                            .icon(if expanded {
                                                                IconName::ChevronDown
                                                            } else {
                                                                IconName::ChevronRight
                                                            })
                                                            .ghost()
                                                            .xsmall()
                                                            .accessibility_label(
                                                                "Toggle tool details",
                                                            )
                                                            .on_click(move |_, _, cx| {
                                                                let _ = toggle.update(
                                                                    cx,
                                                                    |panel, cx| {
                                                                        if !panel
                                                                            .expanded_tools
                                                                            .remove(&id)
                                                                        {
                                                                            panel
                                                                                .expanded_tools
                                                                                .insert(id);
                                                                        }
                                                                        panel.scroller.update(
                                                                            cx,
                                                                            |state, cx| {
                                                                                state.remeasure(cx)
                                                                            },
                                                                        );
                                                                        cx.notify();
                                                                    },
                                                                );
                                                            }),
                                                    )
                                                },
                                            )),
                                    )
                                    .when(has_detail, |card| {
                                        card.child(
                                            Collapsible::new().open(expanded).content(
                                                div()
                                                    .pt_1()
                                                    .border_t_1()
                                                    .border_color(cx.theme().border)
                                                    .max_h_48()
                                                    .overflow_y_scrollbar()
                                                    .text_xs()
                                                    .font_family(
                                                        cx.theme().mono_font_family.clone(),
                                                    )
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(message.text.clone()),
                                            ),
                                        )
                                    }),
                            );
                        }
                        MessageKind::Status => {
                            row = row.items_center().child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(message.text.clone()),
                            );
                        }
                        MessageKind::Error => {
                            row = row.child(
                                Alert::error(("agent-msg-err", message.id), message.text.clone())
                                    .small(),
                            );
                        }
                    }
                    row
                },
            )
            .flex_1()
            .min_h_0()
            .into_any_element()
        };

        // -- Pending approvals ------------------------------------------------
        let mut approvals = v_flex();
        let permission = conversation.and_then(|c| c.permissions.first());
        if let Some(permission) = permission {
            let title = permission
                .request
                .tool_call
                .fields
                .title
                .clone()
                .unwrap_or_else(|| "Permission requested".into());
            let detail = diagnostic_text(
                &serde_json::to_string_pretty(&permission.request.tool_call).unwrap_or_default(),
            );
            let mut buttons = h_flex().gap_1().flex_wrap();
            for option in &permission.request.options {
                let option_id = option.option_id.clone();
                let request_id = permission.request.tool_call.tool_call_id.clone();
                let label = option.name.clone();
                let owner = owner.clone();
                let focus = self.draft.read(cx).focus_handle(cx);
                buttons = buttons.child(
                    Button::new(SharedString::from(format!("agent-permission-{option_id}")))
                        .label(label)
                        .outline()
                        .xsmall()
                        .on_click(move |_, window, cx| {
                            owner.update(cx, |workspace, cx| {
                                if let Some(c) = workspace.agents.current_mut() {
                                    if c.permissions.first().is_some_and(|p| {
                                        p.request.tool_call.tool_call_id == request_id
                                    }) {
                                        let permission = c.permissions.remove(0);
                                        let _ = permission.reply.send(
                                            agent_client_protocol::RequestPermissionResponse::new(
                                                agent_client_protocol::RequestPermissionOutcome::Selected(
                                                    agent_client_protocol::SelectedPermissionOutcome::new(
                                                        option_id.clone(),
                                                    ),
                                                ),
                                            ),
                                        );
                                    }
                                }
                                cx.notify();
                            });
                            focus.focus(window, cx);
                        }),
                );
            }
            approvals = approvals.child(
                v_flex()
                    .mx_3()
                    .mb_2()
                    .p_2()
                    .gap_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Icon::new(IconName::TriangleAlert).size_3())
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(Tag::warning().small().child("Waiting for approval")),
                    )
                    .child(
                        div()
                            .max_h_24()
                            .overflow_y_scrollbar()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(detail),
                    )
                    .child(buttons),
            );
        } else if let Some(request) = conversation.and_then(|c| c.tool_requests.first()) {
            let terminal = request.name == "anode_terminal_exec";
            let mut buttons = h_flex().gap_2();
            for (label, allow) in [
                (if terminal { "Run command" } else { "Build" }, true),
                ("Decline", false),
            ] {
                let owner = owner.clone();
                let button = Button::new(label)
                    .label(label)
                    .xsmall()
                    .on_click(move |_, _, cx| {
                        owner.update(cx, |workspace, cx| {
                            workspace.agent_approve_operation(allow, cx)
                        });
                    });
                buttons = buttons.child(if allow {
                    button.primary()
                } else {
                    button.outline()
                });
            }
            approvals = approvals.child(
                v_flex()
                    .mx_3()
                    .mb_2()
                    .p_2()
                    .gap_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Icon::new(IconName::Terminal).size_3())
                            .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(
                                if terminal {
                                    "Run local command?"
                                } else {
                                    "Build saved project files?"
                                },
                            )),
                    )
                    .child(
                        div()
                            .max_h_24()
                            .overflow_y_scrollbar()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(diagnostic_text(&request.arguments.to_string())),
                    )
                    .child(buttons),
            );
        }

        // -- Slash commands ---------------------------------------------------
        let draft_text = self.draft.read(cx).value().to_string();
        let slash_query = draft_text
            .strip_prefix('/')
            .filter(|q| !q.contains('\n') && !q.contains(' '));
        let matching_commands: Vec<_> = if let Some(query) = slash_query {
            SLASH_COMMANDS
                .iter()
                .copied()
                .filter(|cmd| cmd.command.trim_start_matches('/').starts_with(query))
                .collect()
        } else {
            Vec::new()
        };

        let mut slash_menu = v_flex();
        if !matching_commands.is_empty() {
            let mut list = v_flex()
                .mx_3()
                .mb_2()
                .p_1()
                .gap_1()
                .rounded(cx.theme().radius_lg)
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().popover);
            for (ix, cmd) in matching_commands.into_iter().enumerate() {
                let icon = match cmd.command {
                    "/new-project" => IconName::Boxes,
                    "/debug-board" => IconName::Cpu,
                    "/output" => IconName::Terminal,
                    "/repl" => IconName::SquareTerminal,
                    "/terminal" => IconName::Terminal,
                    _ => IconName::Sparkles,
                };
                list = list.child(
                    Button::new(SharedString::from(format!("slash-cmd-{}", cmd.command)))
                        .ghost()
                        .small()
                        .w_full()
                        .child(
                            h_flex()
                                .w_full()
                                .justify_between()
                                .gap_2()
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .min_w_0()
                                        .child(Icon::new(icon).size_3())
                                        .child(
                                            v_flex()
                                                .items_start()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(cmd.command),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(cmd.description),
                                                ),
                                        ),
                                )
                                .when(ix == 0, |h| {
                                    h.child(
                                        div()
                                            .px_1()
                                            .rounded(cx.theme().radius)
                                            .border_1()
                                            .border_color(cx.theme().border)
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("↵"),
                                    )
                                }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.execute_slash_command(cmd, window, cx);
                        })),
                );
            }
            slash_menu = slash_menu.child(list);
        }

        let stop_owner = owner.clone();
        let panel_attach = cx.weak_entity();
        let attach_button = Button::new("agent-attach-menu")
            .icon(IconName::Plus)
            .outline()
            .rounded_full()
            .xsmall()
            .tooltip("Attach workspace context")
            .accessibility_label("Attach workspace context")
            .dropdown_menu(move |menu, _, _| {
                let sources = [
                    ("Editor selection", AttachSource::Selection),
                    ("Output log", AttachSource::Output),
                    ("REPL output", AttachSource::Repl),
                    ("Terminal output", AttachSource::Terminal),
                ];
                let mut menu = menu;
                for (label, source) in sources {
                    let panel = panel_attach.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = panel.update(cx, |this, cx| this.attach(source, window, cx));
                    }));
                }
                menu
            });

        let composer = v_flex()
            .px_3()
            .pt_2()
            .pb_2()
            .gap_2()
            .flex_shrink_0()
            .child(slash_menu)
            .child(
                v_flex()
                    .p_2()
                    .gap_2()
                    .rounded(cx.theme().radius_lg)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(Textarea::new(&self.draft).aria_label("Message to coding agent"))
                    .child(
                        h_flex()
                            .justify_between()
                            .gap_2()
                            .child(
                                h_flex()
                                    .gap_1()
                                    .flex_wrap()
                                    .min_w_0()
                                    .child(attach_button)
                                    .child(self.render_agent_picker(cx))
                                    .child(self.render_model_picker(cx))
                                    .child(self.render_option_buttons(cx)),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("{} / 64K", draft_text.len())),
                                    )
                                    .child(if busy {
                                        Button::new("agent-stop")
                                            .icon(IconName::Square)
                                            .danger()
                                            .rounded_full()
                                            .small()
                                            .tooltip("Stop response")
                                            .accessibility_label("Stop response")
                                            .disabled(state == State::Cancelling)
                                            .on_click(move |_, _, cx| {
                                                stop_owner.update(cx, |workspace, cx| {
                                                    if let Some(c) = workspace.agents.current_mut()
                                                    {
                                                        c.stop();
                                                    }
                                                    cx.notify();
                                                })
                                            })
                                    } else {
                                        Button::new("agent-send")
                                            .icon(IconName::ArrowUp)
                                            .primary()
                                            .rounded_full()
                                            .small()
                                            .tooltip("Send message")
                                            .accessibility_label("Send message")
                                            .disabled(
                                                state != State::Ready
                                                    || draft_text.trim().is_empty(),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.send(window, cx)
                                            }))
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .px_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(format!(
                        "{} · {board} · {runtime} · Type / for commands",
                        if project.is_empty() {
                            "No project open"
                        } else {
                            &project
                        }
                    )),
            );

        let _ = &self.subscriptions;
        if should_auto_connect {
            let auto_owner = owner.clone();
            cx.defer(move |cx| {
                auto_owner.update(cx, |workspace, cx| {
                    workspace.agents.auto_connect_if_ready();
                    cx.notify();
                });
            });
        }
        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .bg(cx.theme().sidebar)
            .child(header)
            .child(conversation_area)
            .child(approvals)
            // Without an agent there is nothing to send to, so the empty state
            // carries the next action instead of an inert composer.
            .when(provider_name.is_some(), |panel| panel.child(composer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ui_config_directory() -> &'static std::path::Path {
        static CONFIG: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        CONFIG
            .get_or_init(|| {
                let config = tempfile::tempdir().unwrap();
                std::env::set_var("ANODE_CONFIG_DIR", config.path());
                std::env::set_var("ANODE_DISABLE_HARDWARE_PROBE", "1");
                config
            })
            .path()
    }

    #[gpui_kit::test]
    fn saved_chat_rows_leave_room_for_both_text_lines(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::test::TestWindowExt as _;
        use gpui_kit::{px, size, Bounds, WindowBounds, WindowOptions};
        assert!(ui_config_directory().is_dir());
        cx.update(gpui_kit::init);
        let (handle, _workspace) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1180.), px(700.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let workspace = cx.new(|cx| Workspace::new(window, cx));
                    workspace.update(cx, |workspace, cx| {
                        workspace.settings.agent_auto_connect = false;
                        let root = workspace
                            .agents
                            .effective_root()
                            .to_string_lossy()
                            .into_owned();
                        workspace.agents.restore_chats(
                            (0..12)
                                .map(|ix| crate::agent::SavedChat {
                                    id: format!("history-{ix}"),
                                    provider: ProviderId::opencode(),
                                    root: root.clone(),
                                    title: format!("Saved chat {ix}"),
                                    draft: String::new(),
                                    messages: Vec::new(),
                                    updated_at: 12 - ix,
                                })
                                .collect(),
                        );
                        workspace.initialize_panels(window, cx);
                        workspace.show_tool(crate::panels::Tool::Agent, window, cx);
                    });
                    workspace
                },
            )
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.click("agent-history-trigger", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let first = window.find("chat-history-0").bounds();
            let next = window.find("chat-history-1").bounds();
            assert!(
                first.size.height >= px(48.),
                "two text lines need their own row"
            );
            assert!(next.top() >= first.bottom());
            let delete = window.find("chat-delete-history-0").bounds();
            assert!((delete.center().y - first.center().y).abs() <= px(1.));
            assert!(!window.find("chat-history-11").visible());
            window.scroll(
                "chat-history-0",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-800.))),
                cx,
            );
            assert!(window.find("chat-history-11").visible());
            window.click("chat-history-11", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("chat-history-11").is_none());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn opencode_model_and_variant_controls_fit_the_inspector(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
        use gpui_kit::{px, size, Bounds, WindowBounds, WindowOptions};
        assert!(ui_config_directory().is_dir());
        cx.update(gpui_kit::init);
        let (handle, workspace) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1180.), px(700.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let workspace = cx.new(|cx| Workspace::new(window, cx));
                    workspace.update(cx, |workspace, cx| {
                        workspace.settings.agent_auto_connect = false;
                        workspace.agents.select(&ProviderId::opencode());
                        let conversation = workspace.agents.current_mut().unwrap();
                        conversation.state = State::Ready;
                        conversation.options = json!({"configOptions":[
                            {"id":"model","name":"Model","category":"model","type":"select",
                             "currentValue":"opencode-go/claude-haiku-5-5","options":[
                                 {"value":"opencode-go/claude-haiku-5-5",
                                  "name":"opencode-go/Claude Haiku 5.5"},
                                 {"value":"opencode/claude-haiku-5-5",
                                  "name":"opencode/Claude Haiku 5.5"}
                             ]},
                            {"id":"effort","name":"Effort","category":"thought_level","type":"select",
                             "currentValue":"default","options":[
                                 {"value":"default","name":"Default"},
                                 {"value":"high","name":"High"}
                             ]},
                            {"id":"mode","name":"Mode","category":"mode","type":"select",
                             "currentValue":"build","options":[
                                 {"value":"build","name":"Build"},
                                 {"value":"plan","name":"Plan"}
                             ]}
                        ]});
                        workspace.initialize_panels(window, cx);
                        workspace.show_tool(crate::panels::Tool::Agent, window, cx);
                    });
                    workspace
                },
            )
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let send = window.find("agent-send").bounds();
            for id in [
                "agent-model-trigger",
                "agent-option-effort",
                "agent-option-mode",
            ] {
                let control = window.find(id);
                assert!(control.visible(), "{id} must be visible");
                assert!(control.bounds().size.width > px(40.), "{id} must be usable");
                assert!(
                    control.bounds().right() <= send.left(),
                    "{id} overlaps the send controls: {:?} vs {send:?}",
                    control.bounds()
                );
            }
            window.click("agent-option-effort", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find_by_label("High").visible());
            assert_eq!(window.find_by_label("Default").checked(), Some(true));
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.click("agent-model-trigger", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("command").visible());
            assert_eq!(window.find_all_by_label("Claude Haiku 5.5").len(), 2);
            window.press("escape", cx);
        })
        .unwrap();
        cx.update(|cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.agents.disconnect();
                cx.notify();
            });
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("agent-connect").visible());
        })
        .unwrap();
    }

    fn options() -> Vec<Value> {
        vec![
            json!({"value":"opencode-go/glm-5.3-flash","name":"opencode-go/GLM 5.3 Flash"}),
            json!({"value":"opencode/claude-opus-5","name":"opencode/Claude Opus 5"}),
            json!({"value":"amazon-bedrock/anthropic.claude-opus-5-5","name":"amazon-bedrock/Claude Opus 5.5"}),
            json!({"value":"opencode-go/glm-5.3","name":"opencode-go/GLM 5.3"}),
        ]
    }

    #[test]
    fn opencode_go_and_zen_lead_even_when_another_provider_is_current() {
        let groups = group_models(
            &options(),
            "amazon-bedrock/anthropic.claude-opus-5-5",
            "OpenCode",
        );
        let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["OpenCode Go", "OpenCode Zen", "Amazon Bedrock"]);

        let zen = &groups[1];
        assert_eq!(zen.choices.len(), 1);
        // The provider slug is not repeated inside its own group.
        assert_eq!(zen.choices[0].label, "Claude Opus 5");
        assert_eq!(groups[0].choices.len(), 2);
    }

    #[test]
    fn go_stays_first_when_zen_or_no_model_is_selected() {
        for current in ["opencode/claude-opus-5", ""] {
            let groups = group_models(&options(), current, "OpenCode");
            assert_eq!(groups[0].label, "OpenCode Go");
            assert_eq!(groups[1].label, "OpenCode Zen");
        }
    }

    #[test]
    fn the_selected_model_distinguishes_go_from_zen_for_the_same_model() {
        let choices = vec![
            json!({"value":"opencode/glm-5.3","name":"GLM 5.3"}),
            json!({"value":"opencode-go/glm-5.3","name":"GLM 5.3"}),
        ];
        let groups = group_models(&choices, "", "OpenCode");
        assert_eq!(
            selected_model_label(&groups, "opencode-go/glm-5.3"),
            "OpenCode Go · GLM 5.3"
        );
        assert_eq!(
            selected_model_label(&groups, "opencode/glm-5.3"),
            "OpenCode Zen · GLM 5.3"
        );
        assert_eq!(groups[0].choices[0].value, "opencode-go/glm-5.3");
        assert_eq!(groups[1].choices[0].value, "opencode/glm-5.3");
        assert_eq!(selected_model_label(&groups, ""), "Model");
        assert_eq!(
            selected_model_label(&groups, "unknown/model"),
            "unknown/model"
        );
    }

    /// Verbatim shapes from `codex-acp` 2.6: its own models carry no prefix,
    /// the models it resells do.
    #[test]
    fn an_agents_own_unprefixed_models_are_grouped_under_the_agent() {
        let choices = vec![
            json!({"value":"autorouter","name":"AutoRouter"}),
            json!({"value":"anthropic/claude-opus-5-5","name":"Claude Opus 5.5"}),
            json!({"value":"gpt-6.1-sol","name":"6.1 Sol"}),
            json!({"value":"google/gemini-4-carbon","name":"Gemini 4 Carbon"}),
        ];
        let groups = group_models(&choices, "autorouter", "Codex");
        let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["Codex", "Anthropic", "Google"]);
        assert_eq!(groups[0].choices.len(), 2);
        assert_eq!(groups[0].choices[0].label, "AutoRouter");
        assert_eq!(groups[0].choices[1].value, "gpt-6.1-sol");
        assert_eq!(groups[1].choices[0].label, "Claude Opus 5.5");
    }

    #[test]
    fn a_nested_option_group_and_a_bare_value_still_produce_choices() {
        let choices = vec![
            json!({"name":"Group","options":[{"value":"openai/gpt-6","name":"GPT-6"}]}),
            json!({"value":"local-model"}),
        ];
        let groups = group_models(&choices, "", "");
        let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
        // With no agent name to fall back on, the bare group stays generic.
        assert_eq!(labels, ["Models", "OpenAI"]);
        assert_eq!(groups[0].choices[0].value, "local-model");
        assert_eq!(groups[1].choices[0].label, "GPT-6");
    }

    #[test]
    fn the_model_option_is_read_separately_from_the_other_session_options() {
        let options = json!({"configOptions":[
            {"id":"model","name":"Model","currentValue":"opencode/x","type":"select","options":[]},
            {"id":"mode","name":"Session Mode","currentValue":"build","type":"select","options":[]},
        ]});
        let (id, current, _) = model_option(&options).unwrap();
        assert_eq!(id, "model");
        assert_eq!(current, "opencode/x");
        let others = other_options(&options);
        assert_eq!(others.len(), 1);
        assert_eq!(others[0]["id"], "mode");
    }
}
