use super::{
    bridge::Bridge, client::Client, Command, Connection, Event, ProviderProfile, Question,
    QuestionOutcome, State,
};
use agent_client_protocol::{self as acp, Agent as _};
use futures::{channel::mpsc::UnboundedSender, AsyncWriteExt as _, StreamExt as _};
use std::{
    cell::RefCell, path::PathBuf, process::Stdio, rc::Rc, sync::mpsc::sync_channel, time::Duration,
};
use tokio::{io::AsyncReadExt, sync::mpsc};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Bound SDK framing and reject non-ACP stdout without waiting for process
/// exit. `elicitation/create` requests are diverted to Anode's own question
/// handling instead of reaching the pinned SDK, which would answer them with
/// method-not-found and silently cancel the agent's question.
struct FramedRead<R> {
    inner: R,
    line: Vec<u8>,
    /// Validated bytes waiting for the SDK to read them.
    out: std::collections::VecDeque<u8>,
    /// Raw diverted `elicitation/create` request lines.
    questions: mpsc::UnboundedSender<Vec<u8>>,
}

impl<R: futures::io::AsyncRead + Unpin> futures::io::AsyncRead for FramedRead<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        loop {
            if !this.out.is_empty() {
                let count = bytes.len().min(this.out.len());
                for (slot, byte) in bytes.iter_mut().take(count).zip(this.out.drain(..count)) {
                    *slot = byte;
                }
                return std::task::Poll::Ready(Ok(count));
            }
            let count = match std::pin::Pin::new(&mut this.inner).poll_read(cx, bytes) {
                std::task::Poll::Ready(Ok(count)) => count,
                result => return result,
            };
            if count == 0 {
                return std::task::Poll::Ready(Ok(0));
            }
            for byte in &bytes[..count] {
                if *byte == b'\n' {
                    let value =
                        match serde_json::from_slice::<serde_json::Value>(&this.line) {
                            Ok(value) => value,
                            Err(_) => return std::task::Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Malformed ACP stdout. The executable must be a stdio ACP adapter.",
                            ))),
                        };
                    let valid = value["jsonrpc"] == "2.0"
                        && (value["method"].is_string()
                            || (value.get("id").is_some()
                                && (value.get("result").is_some()
                                    || value.get("error").is_some())));
                    if !valid {
                        return std::task::Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Malformed ACP stdout. The executable must be a stdio ACP adapter.",
                        )));
                    }
                    if value["method"] == "elicitation/create" && value.get("id").is_some() {
                        // The question never reaches the SDK; Anode answers it
                        // through the writer below and reports it as an event.
                        let _ = this.questions.send(this.line.clone());
                    } else {
                        let mut line = std::mem::take(&mut this.line);
                        line.push(b'\n');
                        this.out.extend(line);
                    }
                    this.line.clear();
                } else {
                    if this.line.len() >= 4 * 1024 * 1024 {
                        return std::task::Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "ACP frame exceeds 4 MiB",
                        )));
                    }
                    this.line.push(*byte);
                }
            }
        }
    }
}

/// The SDK's writer into the agent's stdin, patched on its way through: the
/// pinned 0.10.2 SDK cannot advertise the elicitation capability its
/// `ClientCapabilities` type predates, so the first `initialize` frame is
/// rewritten once to add `elicitation.form`. Every frame the SDK writes is
/// complete when it arrives, and this writer accepts it whole, so the patch
/// never sees a partial frame. All frames then flow through one shared ordered
/// channel that the question responses use too.
struct PatchingWriter {
    frames: UnboundedSender<Vec<u8>>,
    /// Accumulates the very first frame until its newline arrives.
    first: Option<Vec<u8>>,
}

impl PatchingWriter {
    fn enqueue(&mut self, chunk: &[u8]) {
        let Some(first) = &mut self.first else {
            // The channel preserves send order and frames are whole, so later
            // frames pass straight through.
            let _ = self.frames.unbounded_send(chunk.to_vec());
            return;
        };
        first.extend_from_slice(chunk);
        let Some(newline) = first.iter().position(|byte| *byte == b'\n') else {
            return;
        };
        let mut frame: Vec<u8> = first.drain(..=newline).collect();
        frame.pop(); // the newline; a rewritten frame re-adds its own
        if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&frame) {
            if value["method"] == "initialize" {
                value["params"]["clientCapabilities"]["elicitation"]["form"] =
                    serde_json::json!({});
                if let Ok(mut patched) = serde_json::to_vec(&value) {
                    patched.push(b'\n');
                    self.first = None;
                    let _ = self.frames.unbounded_send(patched);
                    return;
                }
            }
        }
        frame.push(b'\n');
        self.first = None;
        let _ = self.frames.unbounded_send(frame);
    }
}

impl futures::AsyncWrite for PatchingWriter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.enqueue(buf);
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// Writes complete frames to the agent's stdin in send order. One task owns
/// the pipe, so SDK frames and question responses can never interleave.
async fn writer_loop(
    mut frames: futures::channel::mpsc::UnboundedReceiver<Vec<u8>>,
    mut stdin: impl futures::AsyncWrite + Unpin,
) {
    while let Some(frame) = frames.next().await {
        if stdin.write_all(&frame).await.is_err() {
            // The agent is gone; the connection teardown owns the reporting.
            return;
        }
    }
}

/// A stable key for one pending question within its connection.
fn question_key(id: &serde_json::Value) -> String {
    match id {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn json_frame(value: serde_json::Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&value).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// Answer `elicitation/create` requests: validate them, ask the user through
/// the event queue, and write the response the SDK cannot send itself.
async fn question_pump(
    mut questions: mpsc::UnboundedReceiver<Vec<u8>>,
    responses: UnboundedSender<Vec<u8>>,
    events: std::sync::mpsc::SyncSender<Event>,
    session: Rc<RefCell<Option<acp::SessionId>>>,
) {
    while let Some(raw) = questions.recv().await {
        let Ok(request) = serde_json::from_slice::<serde_json::Value>(&raw) else {
            continue;
        };
        let id = request["id"].clone();
        if id.is_null() {
            continue;
        }
        let params = &request["params"];
        // Only form mode was advertised; anything else is invalid params.
        let is_form = params["mode"] == "form";
        let has_message = params["message"].as_str().is_some_and(|m| !m.is_empty());
        let has_schema = params["requestedSchema"].is_object();
        if !(is_form && has_message && has_schema) {
            let _ = responses.unbounded_send(json_frame(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32602, "message": "Anode only supports form-mode elicitation."}
            })));
            continue;
        }
        if let Some(session_id) = params["sessionId"].as_str() {
            let session = session.borrow();
            let current = session.as_ref().map(|session| session.0.as_ref());
            if current != Some(session_id) {
                // A question for another session cannot be shown here.
                let _ = responses.unbounded_send(json_frame(serde_json::json!({
                    "jsonrpc": "2.0", "id": id, "result": {"action": "cancel"}
                })));
                continue;
            }
        }
        let (reply, outcome) = tokio::sync::oneshot::channel();
        let question = Question {
            key: question_key(&id),
            message: params["message"].as_str().unwrap_or_default().to_string(),
            schema: params["requestedSchema"].clone(),
            reply,
        };
        if events
            .try_send(Event::Question(Box::new(question)))
            .is_err()
        {
            let _ = responses.unbounded_send(json_frame(serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": {"action": "cancel"}
            })));
            continue;
        }
        // A dropped reply channel (turn end, stop, disconnect) cancels here.
        let outcome = tokio::time::timeout(Duration::from_secs(300), outcome)
            .await
            .ok()
            .and_then(Result::ok);
        let result = match outcome {
            Some(QuestionOutcome::Accept(content)) => serde_json::json!({
                "action": "accept",
                "content": content,
            }),
            Some(QuestionOutcome::Decline) => serde_json::json!({"action": "decline"}),
            None | Some(QuestionOutcome::Cancel) => serde_json::json!({"action": "cancel"}),
        };
        let _ = responses.unbounded_send(json_frame(serde_json::json!({
            "jsonrpc": "2.0", "id": id, "result": result
        })));
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
    // One ordered stream of whole frames into the agent's stdin, shared by the
    // SDK connection and Anode's own elicitation responses.
    let (frames, frames_rx) = futures::channel::mpsc::unbounded::<Vec<u8>>();
    let (questions_tx, questions_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    tokio::task::spawn_local(writer_loop(
        frames_rx,
        child.stdin.take().unwrap().compat_write(),
    ));
    tokio::task::spawn_local(question_pump(
        questions_rx,
        frames.clone(),
        events.clone(),
        session.clone(),
    ));
    let (connection, io) = acp::ClientSideConnection::new(
        Client {
            events: events.clone(),
            session: session.clone(),
            fault,
        },
        PatchingWriter {
            frames,
            first: Some(Vec::new()),
        },
        FramedRead {
            inner: child.stdout.take().unwrap().compat(),
            line: Vec::new(),
            out: Default::default(),
            questions: questions_tx,
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
    fn questions_are_diverted_answered_and_written_back_over_the_wire() {
        let (_root, connection) = fixture("normal");
        wait(&connection, |e| matches!(e, Event::Ready(..)));
        connection
            .commands
            .try_send(Command::Prompt("question".into()))
            .unwrap();
        // The elicitation request never reaches the SDK: it arrives as an
        // Anode question, with the message and form schema intact.
        wait(&connection, |e| match e {
            Event::Question(question) => {
                assert_eq!(question.key, "question-1");
                assert_eq!(question.message, "Which runtime should the project use?");
                assert_eq!(
                    question.schema["properties"]["runtime"]["default"],
                    "micropython"
                );
                question
                    .reply
                    .send(QuestionOutcome::Accept(
                        serde_json::json!({"runtime": "circuitpython", "runtime_custom": "esp-idf"}),
                    ))
                    .unwrap();
                true
            }
            Event::Failed(e) => panic!("{e}"),
            _ => false,
        });
        // The fixture asserts the exact accept frame and continues the turn.
        wait(&connection, |e| match e {
            Event::Update(update) => update["content"]["text"] == "Answered: circuitpython esp-idf",
            Event::Failed(e) => panic!("{e}"),
            _ => false,
        });
        wait(&connection, |e| matches!(e, Event::Finished(_)));
        connection.shutdown_and_wait();
    }

    #[test]
    fn an_unanswered_question_resolves_as_cancelled_without_breaking_the_turn() {
        let (_root, connection) = fixture("normal");
        wait(&connection, |e| matches!(e, Event::Ready(..)));
        connection
            .commands
            .try_send(Command::Prompt("question-cancel".into()))
            .unwrap();
        // Dropping the question drops its reply channel, which the pump
        // answers as a cancel: the agent keeps going without an answer.
        wait(&connection, |e| matches!(e, Event::Question(_)));
        wait(&connection, |e| match e {
            Event::Update(update) => update["content"]["text"] == "No answer given",
            Event::Failed(e) => panic!("{e}"),
            _ => false,
        });
        wait(&connection, |e| matches!(e, Event::Finished(_)));
        connection.shutdown_and_wait();
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
