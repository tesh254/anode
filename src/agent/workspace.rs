use super::{tools, BuildJob, Conversation, ToolRequest};
use crate::workspace_state::Workspace;
use gpui_kit::{App, AppContext as _, Context};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::{
    hash::{Hash, Hasher},
    time::Duration,
};

impl Workspace {
    pub(crate) fn reset_agent_file_watch(&mut self, cx: &mut Context<Self>) {
        self.agents.disk_revisions = self
            .project_files
            .iter()
            .map(|file| (file.path.clone(), revision(&file.content)))
            .collect();
        self.agents.conflicts.clear();
        self.agents.output_start = self.log_entries.last().map(|entry| entry.id).unwrap_or(0);
        let Some(root) = self.project_path.clone() else {
            return;
        };
        self._agent_watch = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let directory = root.clone();
            let files = cx.background_spawn(async move { crate::project::load_project(&directory) }).await;
            let Ok(files) = files else { continue; };
            let running = this.update(cx, |this, cx| {
                if this.project_path.as_ref() != Some(&root) { return false; }
                let mut changed = false;
                for disk in files {
                    // Project discovery can include symlinks. Never import one that escapes the root.
                    if tools::scoped_path(&root, disk.path.to_string_lossy().as_ref()).is_err() { continue; }
                    let disk_revision = revision(&disk.content);
                    if this.agents.disk_revisions.get(&disk.path) == Some(&disk_revision) { continue; }
                    this.agents.disk_revisions.insert(disk.path.clone(), disk_revision);
                    let dirty = this.open_tabs.iter().any(|tab| tab.path == disk.path && tab.is_dirty);
                    if let Some(file) = this.project_files.iter_mut().find(|file| file.path == disk.path) {
                        if file.content == disk.content {
                            this.agents.conflicts.remove(&disk.path);
                            continue;
                        }
                        if dirty {
                            this.agents.conflicts.insert(disk.path.clone(), disk.content);
                            this.add_log(crate::LogKind::Warn, format!("External edit conflict: {}. Your unsaved buffer is preserved; the agent can read both versions.", disk.relative));
                        } else {
                            *file = disk.clone();
                            if this.current_file.as_ref().is_some_and(|file| file.path == disk.path) {
                                this.current_file = Some(disk);
                                this.needs_editor_sync = true;
                            }
                        }
                    } else {
                        this.project_files.push(disk);
                    }
                    changed = true;
                }
                if changed { cx.notify(); }
                true
            }).unwrap_or(false);
            if !running { break; }
        }));
    }

    pub(crate) fn start_agent_poll(&mut self, cx: &mut Context<Self>) {
        self._agent_poll = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            if this
                .update(cx, |this, cx| {
                    let before = this.agents.current().map(|c| c.revision);
                    let requests = this
                        .agents
                        .current_mut()
                        .map(Conversation::drain)
                        .unwrap_or_default();
                    this.drain_repl_events();
                    for request in requests {
                        this.agent_tool(request, cx);
                    }
                    let after = this.agents.current().map(|c| c.revision);
                    if before != after {
                        this.persist_agent_chats();
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        }));
    }

    pub(crate) fn agent_context(&self) -> Value {
        let files: Vec<_> = self
            .project_files
            .iter()
            .take(200)
            .map(|f| {
                json!({"path":f.relative,"revision":revision(&f.content),
                "unsaved":self.open_tabs.iter().any(|tab| tab.path == f.path && tab.is_dirty)})
            })
            .collect();
        let mut context = json!({
            "project_id":self.project_path, "kind":self.project_kind.label(),
            "runtime":self.active_runtime().label(),
            "active_file":self.current_file.as_ref().map(|file| &file.relative),
            "board":self.active_board().map(|b| json!({"id":b.id,"name":b.name})),
            "detection":self.resolved_board().confidence(),
            "port":self.selected_port,"repl_connected":self.repl_connected,
            "device_busy":self.device_busy,
            "port_lease_busy":self.selected_port_identity().is_some_and(|port|self.port_leases.is_busy(&crate::hardware::device_key(&port))),
            "connection_generation":null,
            "hardware_execution_enabled":false,
            "source_revision":revision(&self.project_files.iter().map(|file| format!("{}:{}", file.relative, revision(&file.content))).collect::<Vec<_>>().join("\n")),
            "files":files,"file_count":self.project_files.len(),
            "external_edit_conflicts":self.agents.conflicts.keys().collect::<Vec<_>>(),
            "toolchain":self.tool_statuses.iter().map(|t| json!({"tool":t.tool.id(),"available":t.is_available(),"detail":t.detail()})).collect::<Vec<_>>(),
            "included_context":["project metadata","board/runtime labels","buffer revisions","toolchain status"],
            "missing_information":["Exact board revision and verified pinout are not established by family-level USB detection"],
            "execution_policy":"Read tools inspect existing state. Builds require Anode approval. Agent-native shell commands are not sandboxed by Anode."
        });
        context["context_revision"] = json!(revision(&context.to_string()));
        context
    }

    fn agent_tool(&mut self, request: ToolRequest, cx: &mut Context<Self>) {
        if request.expired() {
            return;
        }
        if matches!(request.name.as_str(), "anode_build" | "anode_terminal_exec") {
            if let Some(conversation) = self
                .agents
                .current_mut()
                .filter(|c| c.state == super::State::Responding)
            {
                conversation.tool_requests.push(request);
                cx.notify();
                return;
            }
            let _ = request.reply.send(tools::error(
                "cancelled",
                "No active prompt can authorize this operation",
            ));
            return;
        }
        let result = match request.name.as_str() {
            "anode_project_create" => self.agent_create_project(&request.arguments, cx),
            "anode_project_open" => self.agent_open_project(&request.arguments, cx),
            "anode_project_write" => self.agent_write_file(&request.arguments, cx),
            _ => self.agent_read_tool(&request.name, &request.arguments, cx),
        };
        let _ = request.reply.send(result);
    }

    pub(crate) fn agent_approve_operation(&mut self, allow: bool, cx: &mut Context<Self>) {
        let Some(conversation) = self.agents.current_mut() else {
            return;
        };
        if conversation.tool_requests.is_empty() {
            return;
        }
        let request = conversation.tool_requests.remove(0);
        let result = if request.expired() {
            tools::error("timeout", "Request expired; ask the agent to retry")
        } else if !allow {
            tools::error("permission_denied", "Operation declined in Anode")
        } else if request.name == "anode_terminal_exec" {
            self.agent_start_terminal(&request.arguments)
        } else {
            self.agent_start_build(&request.arguments)
        };
        let _ = request.reply.send(result);
        cx.notify();
    }

    fn agent_start_terminal(&mut self, args: &Value) -> Value {
        if args["context_revision"] != self.agent_context()["context_revision"] {
            return tools::error(
                "revision_conflict",
                "Refresh project context before executing a command",
            );
        }
        let Some(root) = self.project_path.clone() else {
            return tools::error("project_required", "Open a project");
        };
        let Some(conversation) = self.agents.current_mut() else {
            return tools::error("disconnected", "No conversation");
        };
        let request = args["request_id"].as_str().unwrap_or_default();
        for (id, job) in &conversation.terminal_jobs {
            if job.has_request(request) {
                return if job.matches(request, args) {
                    json!({"status":"ok","data":{"job_id":id}})
                } else {
                    tools::error(
                        "request_conflict",
                        "request_id was used for different arguments",
                    )
                };
            }
        }
        if conversation.terminal_jobs.len() >= 32 {
            return tools::error("job_limit", "Start a new chat after 32 terminal jobs");
        }
        match super::jobs::TerminalJob::spawn(&root, args) {
            Ok(job) => {
                let id = uuid::Uuid::new_v4().to_string();
                conversation.terminal_jobs.insert(id.clone(), job);
                json!({"status":"ok","project_id":root,"context_revision":args["context_revision"],"data":{"job_id":id}})
            }
            Err(error) => tools::error("execution_failed", &error),
        }
    }

    fn agent_start_build(&mut self, args: &Value) -> Value {
        let Some(root) = self.project_path.clone() else {
            return tools::error("project_required", "Open a project first");
        };
        let Some(request_id) = args["request_id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
        else {
            return tools::error(
                "invalid_arguments",
                "request_id is required (1–128 characters)",
            );
        };
        if let Some(job) = &self.agents.job {
            if job.project == root && job.request_id == request_id {
                return json!({"status":"ok","data":{"job_id":job.id}});
            }
        }
        if args["context_revision"] != self.agent_context()["context_revision"] {
            return tools::error(
                "revision_conflict",
                "Refresh project context before building",
            );
        }
        if self.device_busy {
            return tools::error("port_busy", "Another Anode operation is running");
        }
        if self.open_tabs.iter().any(|tab| tab.is_dirty) {
            return tools::error(
                "revision_conflict",
                "Save or reconcile unsaved editor buffers before building saved files",
            );
        }
        let cursor = self.log_entries.last().map(|entry| entry.id).unwrap_or(0);
        self.build_project();
        if !self.device_busy {
            return tools::error("build_unavailable", &tools::tail(&self.log_as_text(), 4096));
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.agents.job = Some(BuildJob {
            id: id.clone(),
            request_id: request_id.into(),
            project: root,
            cursor,
            result: None,
        });
        json!({"status":"ok","context_revision":args["context_revision"],
            "data":{"job_id":id,"inputs":"saved files"}})
    }

    pub(crate) fn agent_build_finished(&mut self, result: Result<(), String>) {
        if let Some(job) = self.agents.job.as_mut().filter(|job| job.result.is_none()) {
            job.result = Some(result);
        }
    }

    fn agent_create_project(&mut self, args: &Value, cx: &mut Context<Self>) -> Value {
        let Some(raw_name) = args["name"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 80)
        else {
            return tools::error("invalid_arguments", "name is required (1–80 characters)");
        };
        let safe_name: String = raw_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        if safe_name.is_empty() {
            return tools::error(
                "invalid_arguments",
                "name must contain alphanumeric characters",
            );
        }
        let base = super::AgentService::default_root();
        let mut project_dir = base.join(&safe_name);
        if project_dir.exists() {
            let Some(unique) = (2..100)
                .map(|n| base.join(format!("{safe_name}-{n}")))
                .find(|p| !p.exists())
            else {
                return tools::error("already_exists", "A project with that name already exists");
            };
            project_dir = unique;
        }
        if let Some(template_id) = args["template_id"].as_str().filter(|s| !s.is_empty()) {
            let Some(template) = crate::project::TEMPLATES
                .iter()
                .find(|t| t.id == template_id)
            else {
                return tools::error(
                    "unknown_template",
                    "Unknown template_id; call anode_project_templates to list valid IDs",
                );
            };
            if let Err(error) = crate::project::create_from_template(&project_dir, template) {
                return tools::error("create_failed", &error.to_string());
            }
        } else if let Err(error) = std::fs::create_dir_all(&project_dir) {
            return tools::error("create_failed", &error.to_string());
        }
        if let Some(files) = args["files"].as_object() {
            for (rel, content) in files {
                let Some(text) = content.as_str() else {
                    continue;
                };
                let target = match tools::scoped_write_path(&project_dir, rel) {
                    Ok(path) => path,
                    Err(error) => return tools::error("permission_denied", &error),
                };
                if let Err(error) = crate::project::write_file(&target, text) {
                    return tools::error("write_failed", &error.to_string());
                }
            }
        } else if args["template_id"].as_str().is_none() {
            let main_py = project_dir.join("main.py");
            let _ = crate::project::write_file(&main_py, "# Anode project\n");
        }

        let mut git_initialized = false;
        let mut git_note = None;
        if args["init_git"].as_bool() == Some(true) {
            match std::process::Command::new("git")
                .arg("init")
                .current_dir(&project_dir)
                .output()
            {
                Ok(out) if out.status.success() => {
                    git_initialized = true;
                }
                Ok(out) => {
                    git_note = Some(String::from_utf8_lossy(&out.stderr).trim().to_string());
                }
                Err(error) => {
                    git_note = Some(error.to_string());
                }
            }
        }

        let should_open = args["open"].as_bool().unwrap_or(true);
        if should_open {
            self.open_project_preserving_agent(project_dir.clone(), cx);
            cx.notify();
        }
        let context = self.agent_context();
        json!({
            "status":"ok",
            "project_id":project_dir,
            "context_revision":context["context_revision"],
            "data":{
                "project_path":project_dir,
                "opened":should_open,
                "git_initialized":git_initialized,
                "git_note":git_note,
                "files":context["files"]
            }
        })
    }

    fn agent_open_project(&mut self, args: &Value, cx: &mut Context<Self>) -> Value {
        let Some(raw) = args["path"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return tools::error("invalid_arguments", "path is required");
        };
        let path = PathBuf::from(raw);
        let resolved = if path.is_absolute() {
            path
        } else {
            super::AgentService::default_root().join(path)
        };
        if !resolved.is_dir() {
            return tools::error("not_found", "Project directory does not exist");
        }
        self.open_project_preserving_agent(resolved.clone(), cx);
        cx.notify();
        let context = self.agent_context();
        json!({
            "status":"ok",
            "project_id":self.project_path,
            "context_revision":context["context_revision"],
            "data":context
        })
    }

    fn agent_write_file(&mut self, args: &Value, cx: &mut Context<Self>) -> Value {
        let Some(root) = self.project_path.clone() else {
            return tools::error("project_required", "Open or create a project first");
        };
        let Some(rel) = args["path"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return tools::error("invalid_arguments", "path is required");
        };
        let Some(content) = args["content"].as_str() else {
            return tools::error("invalid_arguments", "content must be a string");
        };
        let target = match tools::scoped_write_path(&root, rel) {
            Ok(path) => path,
            Err(error) => return tools::error("permission_denied", &error),
        };
        if let Err(error) = crate::project::write_file(&target, content) {
            return tools::error("write_failed", &error.to_string());
        }
        let disk_rev = revision(content);
        self.agents
            .disk_revisions
            .insert(target.clone(), disk_rev.clone());
        self.agents.conflicts.remove(&target);
        if let Some(file) = self.project_files.iter_mut().find(|f| f.path == target) {
            file.content = content.to_string();
        } else {
            let name = target
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel.to_string());
            let language =
                crate::project::language_for_file(&name).unwrap_or(crate::project::Language::Text);
            self.project_files.push(crate::project::ProjectFile {
                name,
                relative: rel.to_string(),
                path: target.clone(),
                content: content.to_string(),
                language,
            });
        }
        if self.current_file.as_ref().is_some_and(|f| f.path == target) {
            if let Some(current) = self.current_file.as_mut() {
                current.content = content.to_string();
            }
            self.needs_editor_sync = true;
        }
        cx.notify();
        let context = self.agent_context();
        json!({
            "status":"ok",
            "project_id":root,
            "context_revision":context["context_revision"],
            "data":{"path":rel,"revision":disk_rev}
        })
    }

    fn agent_read_tool(&self, name: &str, args: &Value, cx: &App) -> Value {
        let context = self.agent_context();
        let root_opt = self.project_path.as_ref();
        let require_root = || {
            root_opt
                .ok_or_else(|| tools::error("project_required", "Open or create a project first"))
        };
        let limit = args["limit"]
            .as_u64()
            .unwrap_or(8192)
            .clamp(1, tools::OUTPUT_LIMIT as u64) as usize;
        let data = match name {
            "anode_project_templates" => {
                json!({"templates":crate::project::TEMPLATES.iter().map(|template| json!({
                "id":template.id,"name":template.name,"description":template.description,
                "runtime":template.runtime.label(),"files":template.files.iter().map(|(name,_)|name).collect::<Vec<_>>()
            })).collect::<Vec<_>>()})
            }
            "anode_project_context" | "anode_board_status" => context.clone(),
            "anode_project_files" => {
                let offset = args["offset"].as_u64().unwrap_or(0) as usize;
                let count = args["limit"].as_u64().unwrap_or(100).clamp(1, 200) as usize;
                let files: Vec<_> = self
                    .project_files
                    .iter()
                    .skip(offset)
                    .take(count)
                    .map(|file| json!({"path":file.relative,"revision":revision(&file.content)}))
                    .collect();
                json!({"files":files,"total":self.project_files.len(),"next_offset":offset.saturating_add(count).min(self.project_files.len())})
            }
            "anode_project_read" | "_acp_read_file" => {
                let Some(path) = args["path"].as_str() else {
                    return tools::error("invalid_arguments", "path is required");
                };
                let root = match require_root() {
                    Ok(r) => r,
                    Err(e) => return e,
                };
                let path = match tools::scoped_path(root, path) {
                    Ok(p) => p,
                    Err(e) => return tools::error("permission_denied", &e),
                };
                if let Some(file) = self
                    .project_files
                    .iter()
                    .find(|file| file.path.canonicalize().ok().as_ref() == Some(&path))
                {
                    let start = args["line"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                    let count = args["line_limit"].as_u64().unwrap_or(u64::MAX) as usize;
                    let text = if name == "_acp_read_file" {
                        file.content
                            .lines()
                            .skip(start)
                            .take(count)
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        file.content.clone()
                    };
                    json!({"text":tools::head(&text, limit),"revision":revision(&file.content),
                        "source":"live buffer","truncated":text.len() > limit,
                        "conflict_disk_text":self.agents.conflicts.get(&file.path).map(|text|tools::head(text, limit))})
                } else {
                    use std::io::Read as _;
                    if name == "_acp_read_file" {
                        return tools::error("file_not_loaded","Open this file in Anode, or use anode_project_read for a bounded disk read");
                    }
                    let file = match std::fs::File::open(&path) {
                        Ok(file) => file,
                        Err(error) => return tools::error("read_failed", &error.to_string()),
                    };
                    let mut bytes = Vec::new();
                    if let Err(error) = file.take((limit + 4) as u64).read_to_end(&mut bytes) {
                        return tools::error("read_failed", &error.to_string());
                    }
                    let text = String::from_utf8_lossy(&bytes);
                    json!({"text":tools::head(&text,limit),"source":"saved disk file",
                        "revision":null,"truncated":bytes.len()>limit})
                }
            }
            "anode_project_search" => {
                let Some(query) = args["query"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 4096)
                else {
                    return tools::error("invalid_arguments", "query is required");
                };
                let mut matches = Vec::new();
                let root = match require_root() {
                    Ok(r) => r,
                    Err(e) => return e,
                };
                'files: for file in &self.project_files {
                    if tools::scoped_path(root, file.path.to_string_lossy().as_ref()).is_err() {
                        continue;
                    }
                    for (line, text) in file.content.lines().enumerate() {
                        if text.contains(query) {
                            matches.push(json!({"path":file.relative,"line":line+1,"text":tools::head(text,512)}));
                            if matches.len() >= limit.min(100) {
                                break 'files;
                            }
                        }
                    }
                }
                json!({"matches":matches,"limit":limit.min(100)})
            }
            "anode_output_tail" | "anode_diagnostics" => {
                let cursor = args["cursor"]
                    .as_u64()
                    .unwrap_or(0)
                    .max(self.agents.output_start);
                let mut size = 0;
                let mut records = Vec::new();
                let mut entries: Vec<_> = self
                    .log_entries
                    .iter()
                    .filter(|entry| {
                        (root_opt.is_none() || entry.project.as_ref() == root_opt)
                            && entry.id > cursor
                            && (name != "anode_diagnostics" || entry.kind.is_problem())
                    })
                    .collect();
                let newest = args["cursor"].as_u64().is_none();
                if newest {
                    entries.reverse();
                }
                for entry in entries {
                    if entry.id <= cursor
                        || (name == "anode_diagnostics" && !entry.kind.is_problem())
                    {
                        continue;
                    }
                    let text = tools::head(&entry.message, limit.saturating_sub(size));
                    size += text.len();
                    records.push(json!({"cursor":entry.id,"timestamp":entry.time.as_ref(),"kind":format!("{:?}",entry.kind),"text":text}));
                    if size >= limit {
                        break;
                    }
                }
                if newest {
                    records.reverse();
                }
                json!({"records":records,"next_cursor":records.last().map(|r| &r["cursor"]),
                    "oldest_cursor":self.log_entries.first().map(|e|e.id),
                    "latest_cursor":self.log_entries.last().map(|e|e.id),
                    "truncated":size >= limit,"toolchain":context["toolchain"]})
            }
            "anode_repl_tail" => {
                json!({"text":tools::tail(&self.repl_output,limit),"connected":self.repl_connected,"port":self.selected_port,"truncated":self.repl_output.len()>limit})
            }
            "anode_terminal_tail" => {
                let root = match require_root() {
                    Ok(r) => r,
                    Err(e) => return e,
                };
                let text = self
                    .terminal
                    .read(cx)
                    .agent_output(root)
                    .unwrap_or_default();
                json!({"text":tools::tail(&text,limit),"cwd_at_launch":root,"truncated":text.len()>limit,
                    "note":"Terminal output may include programs started by the developer. Shell cwd may have changed."})
            }
            "anode_job_status" | "anode_job_cancel" => {
                let root = match require_root() {
                    Ok(r) => r,
                    Err(e) => return e,
                };
                if let Some(job) = self.agents.current().and_then(|c| {
                    c.terminal_jobs
                        .get(args["job_id"].as_str().unwrap_or_default())
                }) {
                    if name == "anode_job_cancel" {
                        job.cancel();
                    }
                    return json!({"status":"ok","project_id":root,"context_revision":context["context_revision"],
                        "data":job.status(limit),"cancellation_requested":name=="anode_job_cancel"});
                }
                if name == "anode_job_cancel" {
                    return tools::error(
                        "cancellation_unsupported",
                        "Only agent terminal jobs support cancellation; Anode builds must finish",
                    );
                }
                match self
                    .agents
                    .job
                    .as_ref()
                    .filter(|job| job.project == *root && args["job_id"].as_str() == Some(&job.id))
                {
                    Some(job) => {
                        json!({"job_id":job.id,"state":match &job.result {None=>"running",Some(Ok(()))=>"completed",Some(Err(_))=>"failed"},
                        "error":job.result.as_ref().and_then(|r|r.as_ref().err()),
                        "output_cursor":job.cursor,"inputs":"saved files"})
                    }
                    None => return tools::error("unknown_job", "No such job in this project"),
                }
            }
            _ => {
                return tools::error(
                    "unsupported_tool",
                    "This tool is not implemented or enabled",
                )
            }
        };
        json!({"status":"ok","project_id":root_opt,"context_revision":context["context_revision"],"observed_at":chrono::Utc::now().to_rfc3339(),"data":data,
            "warnings":[],"missing_information":[]})
    }
}

fn revision(text: &str) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hash);
    format!("{:016x}", hash.finish())
}
