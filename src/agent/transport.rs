use super::{bridge::Bridge, client::Client, Command, Connection, Event, ProviderProfile, State};
use agent_client_protocol::{self as acp, Agent as _};
use std::{
    cell::RefCell, path::PathBuf, process::Stdio, rc::Rc, sync::mpsc::sync_channel, time::Duration,
};
use tokio::{io::AsyncReadExt, sync::mpsc};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Bound SDK framing and reject non-ACP stdout without waiting for process exit.
struct FramedRead<R> {
    inner: R,
    line: Vec<u8>,
}

impl<R: futures::io::AsyncRead + Unpin> futures::io::AsyncRead for FramedRead<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let count = match std::pin::Pin::new(&mut self.inner).poll_read(cx, bytes) {
            std::task::Poll::Ready(Ok(count)) => count,
            result => return result,
        };
        for byte in &bytes[..count] {
            if *byte == b'\n' {
                let valid = serde_json::from_slice::<serde_json::Value>(&self.line)
                    .ok()
                    .is_some_and(|value| {
                        value["jsonrpc"] == "2.0"
                            && (value["method"].is_string()
                                || (value.get("id").is_some()
                                    && (value.get("result").is_some()
                                        || value.get("error").is_some())))
                    });
                if !valid {
                    return std::task::Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Malformed ACP stdout. The executable must be a stdio ACP adapter.",
                    )));
                }
                self.line.clear();
            } else {
                if self.line.len() >= 4 * 1024 * 1024 {
                    return std::task::Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "ACP frame exceeds 4 MiB",
                    )));
                }
                self.line.push(*byte);
            }
        }
        std::task::Poll::Ready(Ok(count))
    }
}

pub(super) fn start(profile: &ProviderProfile, root: PathBuf) -> Connection {
    let profile = profile.clone();
    let (commands, receiver) = mpsc::channel(8);
    let (events, event_receiver) = sync_channel(256);
    let (shutdown, shutdown_receiver) = tokio::sync::oneshot::channel();
    let (done_sender, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
            .and_then(|runtime| {
                runtime.block_on(tokio::task::LocalSet::new().run_until(run(
                    root,
                    profile,
                    receiver,
                    events.clone(),
                    shutdown_receiver,
                )))
            });
        if let Err(error) = result {
            let _ = events.send(Event::Failed(redact(&error)));
        }
        let _ = done_sender.send(());
    });
    Connection {
        commands,
        events: event_receiver,
        shutdown: Some(shutdown),
        done,
    }
}

async fn run(
    root: PathBuf,
    profile: ProviderProfile,
    mut commands: mpsc::Receiver<Command>,
    events: std::sync::mpsc::SyncSender<Event>,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let executable = profile.resolve().map_err(|e| {
        format!(
            "{e} Install it or set its command in Settings → Providers.\n{}",
            profile.kind.guidance()
        )
    })?;
    let _ = events.try_send(Event::Diagnostic(format!(
        "Executable: {}",
        executable.display()
    )));
    if !profile.env.is_empty() {
        // Names only: a value here can be an API key.
        let _ = events.try_send(Event::Diagnostic(format!(
            "Environment: {}",
            profile.env.keys().cloned().collect::<Vec<_>>().join(", ")
        )));
    }
    let mut probe = tokio::process::Command::new(&executable);
    let child_path = crate::toolchain::subprocess_path(&executable);
    if let Some(path) = &child_path {
        probe.env("PATH", path);
    }
    probe
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let probe_result = tokio::select! {
        _ = &mut shutdown => return Ok(()),
        result = tokio::time::timeout(Duration::from_secs(4), probe.output()) => result,
    };
    if let Ok(Ok(output)) = probe_result {
        let version = super::tools::head(&String::from_utf8_lossy(&output.stdout), 256);
        let _ = events.try_send(Event::Diagnostic(format!(
            "Version: {} (compatibility not yet certified)",
            redact(version.trim())
        )));
    }
    let bridge = Bridge::start(events.clone())?;
    let mut launch = tokio::process::Command::new(&executable);
    if let Some(path) = child_path {
        launch.env("PATH", path);
    }
    for (name, value) in &profile.env {
        launch.env(name, value);
    }
    launch
        .args(&profile.args)
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    launch.process_group(0);
    let mut child = launch
        .spawn()
        .map_err(|e| format!("Could not launch {}: {e}", executable.display()))?;
    let process_group = child.id();
    let stderr = child.stderr.take().unwrap();
    let diagnostics = events.clone();
    tokio::task::spawn_local(async move {
        // Never allow an unbounded line or a log flood into the UI event queue.
        let mut stderr = stderr;
        let mut bytes = [0; 2048];
        let mut displayed = 0;
        let mut line = Vec::new();
        let mut oversized = false;
        while let Ok(count) = stderr.read(&mut bytes).await {
            if count == 0 {
                break;
            }
            for byte in &bytes[..count] {
                if *byte == b'\n' {
                    if !oversized && displayed < 8192 {
                        displayed += line.len();
                        let text = redact(&String::from_utf8_lossy(&line));
                        let _ = diagnostics
                            .try_send(Event::Diagnostic(format!("Agent stderr: {text}")));
                    }
                    line.clear();
                    oversized = false;
                } else if line.len() < 8192 && !oversized {
                    line.push(*byte);
                } else {
                    line.clear();
                    oversized = true;
                }
            }
        }
        if !line.is_empty() && !oversized && displayed < 8192 {
            let _ = diagnostics.try_send(Event::Diagnostic(format!(
                "Agent stderr: {}",
                redact(&String::from_utf8_lossy(&line))
            )));
        }
    });
    let session = Rc::new(RefCell::new(None));
    let (fault, mut faults) = tokio::sync::mpsc::channel(1);
    let (connection, io) = acp::ClientSideConnection::new(
        Client {
            events: events.clone(),
            session: session.clone(),
            fault,
        },
        child.stdin.take().unwrap().compat_write(),
        FramedRead {
            inner: child.stdout.take().unwrap().compat(),
            line: Vec::new(),
        },
        |future| {
            tokio::task::spawn_local(future);
        },
    );
    let mut io = tokio::task::spawn_local(io);
    let workflow = async {
        let _ = events.try_send(Event::State(State::Initializing));
        let init = tokio::time::timeout(
            Duration::from_secs(20),
            connection.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                    .client_info(acp::Implementation::new("anode", env!("CARGO_PKG_VERSION")))
                    .client_capabilities(
                        acp::ClientCapabilities::default()
                            .fs(acp::FileSystemCapabilities::default().read_text_file(true)),
                    ),
            ),
        )
        .await
        .map_err(|_| "Initialization timed out")?
        .map_err(|e| format!("Initialize failed: {e}"))?;
        if init.protocol_version != acp::ProtocolVersion::V1 {
            return Err(format!(
                "Unsupported ACP protocol {}. Install an ACP v1 compatible adapter.",
                init.protocol_version
            ));
        }
        let _ = events.try_send(Event::Diagnostic(format!(
            "Negotiated ACP v1: {}",
            redact(&serde_json::to_string(&init).unwrap_or_default())
        )));
        let _ = events.try_send(Event::State(State::Opening));
        let opened = tokio::time::timeout(Duration::from_secs(30), connection.new_session(
            acp::NewSessionRequest::new(root).mcp_servers(vec![bridge.mcp()?])
        )).await.map_err(|_| "Opening session timed out")?
            .map_err(|e| format!("Cannot open session: {e}\n{}\nAdvertised login methods: {}. Sign in from Settings → Providers, then Reconnect.",
                profile.kind.guidance(), serde_json::to_string(&init.auth_methods).unwrap_or_default()))?;
        *session.borrow_mut() = Some(opened.session_id.clone());
        let _ = events.try_send(Event::Ready(
            opened.session_id.to_string(),
            serde_json::to_value(&opened).unwrap_or_default(),
        ));
        while let Some(command) = commands.recv().await {
            match command {
                Command::Shutdown => break,
                Command::Cancel => {}
                Command::SetConfig(id, value) => {
                    match tokio::time::timeout(
                        Duration::from_secs(15),
                        connection.set_session_config_option(
                            acp::SetSessionConfigOptionRequest::new(
                                opened.session_id.clone(),
                                id,
                                value,
                            ),
                        ),
                    )
                    .await
                    {
                        Ok(Ok(result)) => {
                            let _ = events.try_send(Event::Update(serde_json::json!({
                                "sessionUpdate":"config_option_update","configOptions":result.config_options
                            })));
                        }
                        result => {
                            let _ = events.try_send(Event::Diagnostic(format!(
                                "Could not change session option: {result:?}"
                            )));
                        }
                    }
                }
                Command::Prompt(text) => {
                    let prompt = connection.prompt(acp::PromptRequest::new(
                        opened.session_id.clone(),
                        vec![text.into()],
                    ));
                    tokio::pin!(prompt);
                    let deadline = tokio::time::sleep(Duration::from_secs(600));
                    tokio::pin!(deadline);
                    let mut cancelling = false;
                    let outcome = loop {
                        tokio::select! {
                            // Poll prompt first so its request enters the ordered
                            // transport before an immediately queued Stop notification.
                            biased;
                            result = &mut prompt => {
                                break result.map(|r| format!("Turn ended: {:?}", r.stop_reason));
                            }
                            _ = &mut deadline => return Err(if cancelling {
                                "Agent did not acknowledge Stop. Response interrupted; reconnect to continue."
                            } else { "Agent response timed out. Reconnect to continue." }.into()),
                            command = commands.recv() => match command {
                                Some(Command::Cancel) if !cancelling => {
                                    cancelling = true;
                                    tokio::time::timeout(Duration::from_secs(3), connection.cancel(acp::CancelNotification::new(opened.session_id.clone())))
                                        .await.map_err(|_| "Cancellation timed out")?.map_err(|e| e.to_string())?;
                                    deadline.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(5));
                                }
                                Some(Command::Shutdown) | None => return Ok(()),
                                _ => {}
                            }
                        }
                    };
                    match outcome {
                        Ok(reason) => {
                            let _ = events.try_send(Event::Finished(reason));
                        }
                        Err(error) => {
                            let _ = events.try_send(Event::TurnError(redact(&error.to_string())));
                        }
                    }
                }
            }
        }
        Ok(())
    };
    let result = tokio::select! {
        result = workflow => result,
        result = &mut io => Err(format!("ACP transport closed: {result:?}. Reconnect to start a new session.")),
        status = child.wait() => Err(format!("Agent exited: {status:?}. Reconnect to start a new session.")),
        _ = &mut shutdown => Ok(()),
        _ = faults.recv() => Err("Agent output exceeded the bounded event queue. Response interrupted; reconnect to continue.".into()),
    };
    io.abort();
    // Reap explicitly on normal disconnect as well as failure.
    #[cfg(unix)]
    if let Some(pid) = process_group {
        // The adapter and its Codex App Server/MCP descendants share this
        // dedicated group, never Anode's own process group.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
    drop(bridge);
    result
}

pub(crate) fn redact(text: &str) -> String {
    let secret = regex::Regex::new(r#"(?i)(?:bearer\s+[^\s,;"}]+|(?:api[_-]?key|token|authorization|secret)["']?\s*[=:]\s*["']?(?:bearer\s+)?[^\s,;"'}]+|sk-(?:proj-)?[^\s,;"}]+)"#).unwrap();
    super::tools::head(&secret.replace_all(text, "[redacted]"), 4096)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;

    fn fixture(mode: &str) -> (tempfile::TempDir, Connection) {
        let root = tempfile::Builder::new()
            .prefix("anode acp ")
            .tempdir()
            .unwrap();
        let python = crate::toolchain::locate_executable("python3")
            .expect("Python required for ACP contract fixtures");
        let script = root.path().join("agent.py");
        std::fs::write(&script, include_str!("../../tests/fixtures/acp_agent.py")).unwrap();
        let mut profile = super::ProviderProfile::custom("ACP fixture");
        profile.command = python.to_string_lossy().into();
        profile.args = vec!["-u".into(), script.to_string_lossy().into(), mode.into()];
        let connection = start(&profile, root.path().to_path_buf());
        (root, connection)
    }

    fn wait(connection: &Connection, mut predicate: impl FnMut(Event) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(event) = connection.events.recv_timeout(Duration::from_millis(100)) {
                if predicate(event) {
                    return;
                }
            }
        }
        panic!("ACP fixture did not produce expected event");
    }

    #[test]
    fn actual_stdio_streams_permissions_and_acknowledges_stop() {
        let (_root, connection) = fixture("normal");
        wait(&connection, |e| match e {
            Event::Ready(id, _) => {
                assert_eq!(id, "fixture-session");
                true
            }
            Event::Failed(e) => panic!("{e}"),
            _ => false,
        });
        connection
            .commands
            .try_send(Command::Prompt("permission".into()))
            .unwrap();
        wait(&connection, |e| match e {
            Event::Permission(p) => {
                assert_eq!(p.request.options[0].option_id.to_string(), "exact-allow-id");
                p.reply
                    .send(acp::RequestPermissionResponse::new(
                        acp::RequestPermissionOutcome::Selected(
                            acp::SelectedPermissionOutcome::new("exact-allow-id"),
                        ),
                    ))
                    .unwrap();
                true
            }
            Event::Failed(e) => panic!("{e}"),
            _ => false,
        });
        wait(&connection, |e| match e {
            Event::Update(update) => update["content"]["text"] == "Hello 🦀",
            Event::Failed(e) => panic!("{e}"),
            _ => false,
        });
        wait(&connection, |e| matches!(e, Event::Finished(_)));
        connection
            .commands
            .try_send(Command::Prompt("wait for stop".into()))
            .unwrap();
        connection.commands.try_send(Command::Cancel).unwrap();
        wait(&connection, |e| match e {
            Event::Finished(reason) => {
                assert!(reason.contains("Cancelled"));
                true
            }
            Event::Failed(e) => panic!("{e}"),
            _ => false,
        });
    }

    #[test]
    fn incompatible_protocol_and_malformed_output_fail_without_replay() {
        for mode in ["unsupported", "malformed"] {
            let (_root, connection) = fixture(mode);
            wait(&connection, |e| matches!(e, Event::Failed(_)));
        }
    }

    #[test]
    fn diagnostics_redact_secrets() {
        let result =
            redact("Authorization: Bearer sensitive-value token=other sk-proj-secret-value");
        assert!(!result.contains("sensitive-value"));
        assert!(!result.contains("other"));
        assert!(!result.contains("secret-value"));
        assert!(serde_json::from_value::<acp::McpServer>(json!({
            "name":"anode-test","command":"/tmp/anode","args":[],"env":[]
        }))
        .is_ok());
    }
}
