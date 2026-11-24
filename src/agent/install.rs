//! Installing and signing in to local ACP agents from inside Anode.
//!
//! Every command here is one the user could run themselves; Anode runs it as a
//! managed child process so the result, the output, and the failure are visible
//! in the settings screen instead of being typed into a terminal that may
//! already be busy. Credentials are never read or stored — sign-in happens in
//! the provider's own CLI and browser flow.
use super::profiles::{InstallStep, ProviderKind, ProviderProfile};
use futures::channel::mpsc::UnboundedSender;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Progress from a managed command.
#[derive(Clone, Debug)]
pub(crate) enum JobEvent {
    Line(String),
    Finished(Result<String, String>),
}

/// Cancels a running managed command and reports whether one is still live.
#[derive(Clone, Default)]
pub(crate) struct JobHandle {
    pid: Arc<Mutex<Option<u32>>>,
    cancelled: Arc<AtomicBool>,
}

impl JobHandle {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        #[cfg(unix)]
        if let Some(pid) = self.pid.lock().ok().and_then(|pid| *pid) {
            // The child runs in its own process group, so this never reaches
            // Anode's own processes.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// What a provider needs before Anode can run it, described for the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Readiness {
    /// Anode knows how to install everything that is missing.
    Installable(Vec<InstallStep>),
    /// The provider is a custom agent; only the user knows how to install it.
    Manual,
    Ready,
}

pub(crate) fn readiness(profile: &ProviderProfile, missing: &[InstallStep]) -> Readiness {
    if missing.is_empty() && profile.resolve().is_ok() {
        return Readiness::Ready;
    }
    if profile.kind == ProviderKind::Custom {
        return Readiness::Manual;
    }
    Readiness::Installable(missing.to_vec())
}

/// The package manager Anode installs built-in agents with.
pub(crate) fn package_manager() -> Result<PathBuf, String> {
    crate::toolchain::locate_executable("npm").ok_or_else(|| {
        "Cannot find `npm`. Install Node.js, then check for the agent again.".to_string()
    })
}

/// Install the packages a built-in provider is missing, one after another.
/// Blocking; run it on a background executor.
pub(crate) fn install_steps(
    steps: Vec<InstallStep>,
    handle: JobHandle,
    sender: UnboundedSender<JobEvent>,
) {
    let npm = match package_manager() {
        Ok(npm) => npm,
        Err(error) => {
            let _ = sender.unbounded_send(JobEvent::Finished(Err(error)));
            return;
        }
    };
    for step in &steps {
        let _ = sender.unbounded_send(JobEvent::Line(format!("$ npm install -g {}", step.package)));
        let outcome = run(
            &npm,
            &["install".into(), "-g".into(), step.package.into()],
            &BTreeMap::new(),
            &handle,
            &sender,
        );
        if let Err(error) = outcome {
            let _ = sender.unbounded_send(JobEvent::Finished(Err(format!(
                "Could not install {}: {error}",
                step.package
            ))));
            return;
        }
        if !step.is_installed() {
            let _ = sender.unbounded_send(JobEvent::Finished(Err(format!(
                "{} installed, but `{}` is still not on PATH. Check your npm global bin directory.",
                step.package, step.binary
            ))));
            return;
        }
    }
    let installed = steps
        .iter()
        .map(|step| step.package)
        .collect::<Vec<_>>()
        .join(", ");
    let _ = sender.unbounded_send(JobEvent::Finished(Ok(format!("Installed {installed}."))));
}

/// Start the provider's own sign-in command. Codex opens a browser and keeps
/// running until the sign-in completes, so the caller keeps the handle to stop
/// it. OpenCode's sign-in is interactive and is not run through here.
pub(crate) fn sign_in(
    profile: &ProviderProfile,
    handle: JobHandle,
    sender: UnboundedSender<JobEvent>,
) {
    let Some(binary) = profile.kind.auth_binary() else {
        let _ = sender.unbounded_send(JobEvent::Finished(Err(
            "This agent has no sign-in command.".into(),
        )));
        return;
    };
    let Some(executable) = crate::toolchain::locate_executable(binary) else {
        let _ = sender.unbounded_send(JobEvent::Finished(Err(format!(
            "Cannot find `{binary}`. Install the agent first."
        ))));
        return;
    };
    let _ = sender.unbounded_send(JobEvent::Line(format!("$ {binary} login")));
    match run(
        &executable,
        &["login".into()],
        &profile.env,
        &handle,
        &sender,
    ) {
        Ok(()) => {
            let _ = sender.unbounded_send(JobEvent::Finished(Ok(
                "Sign-in command finished. Check the account below.".into(),
            )));
        }
        Err(error) => {
            let _ = sender.unbounded_send(JobEvent::Finished(Err(error)));
        }
    }
}

/// Sign out through the provider's own command.
pub(crate) fn sign_out(profile: &ProviderProfile) -> Result<String, String> {
    let binary = profile
        .kind
        .auth_binary()
        .ok_or_else(|| "This agent has no sign-out command.".to_string())?;
    let executable = crate::toolchain::locate_executable(binary)
        .ok_or_else(|| format!("Cannot find `{binary}`."))?;
    let args: &[&str] = match profile.kind {
        ProviderKind::Codex => &["logout"],
        _ => return Err("Sign out from the OpenCode CLI.".into()),
    };
    let output = super::profiles::run(&executable, args, profile)
        .ok_or_else(|| format!("Could not run `{binary} {}`.", args.join(" ")))?;
    if output.success {
        Ok("Signed out.".into())
    } else {
        Err(output.text)
    }
}

/// The sign-in URL a provider printed, if its output contains one.
pub(crate) fn sign_in_url(lines: &[String]) -> Option<String> {
    lines.iter().rev().find_map(|line| {
        let start = line.find("http://").or_else(|| line.find("https://"))?;
        let url: String = line[start..]
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'')
            .collect();
        (url.len() > "https://".len()).then_some(url)
    })
}

fn run(
    program: &PathBuf,
    args: &[String],
    env: &BTreeMap<String, String>,
    handle: &JobHandle,
    sender: &UnboundedSender<JobEvent>,
) -> Result<(), String> {
    let mut command = Command::new(program);
    if let Some(path) = crate::toolchain::subprocess_path(program) {
        command.env("PATH", path);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Never inherit a pager that would wait for a key press.
        .env("PAGER", "cat")
        .env("CI", "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("{}: {error}", program.display()))?;
    if let Ok(mut pid) = handle.pid.lock() {
        *pid = child.id().into();
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let errors = Arc::new(Mutex::new(String::new()));
    let collected = errors.clone();
    let error_sender = sender.clone();
    let error_reader = std::thread::spawn(move || {
        if let Some(stderr) = stderr {
            stream(stderr, |line| {
                if let Ok(mut collected) = collected.lock() {
                    if collected.len() < 4096 {
                        collected.push_str(&line);
                        collected.push('\n');
                    }
                }
                let _ = error_sender.unbounded_send(JobEvent::Line(line));
            });
        }
    });
    if let Some(stdout) = stdout {
        let sender = sender.clone();
        stream(stdout, |line| {
            let _ = sender.unbounded_send(JobEvent::Line(line));
        });
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    let _ = error_reader.join();
    if let Ok(mut pid) = handle.pid.lock() {
        *pid = None;
    }
    if handle.is_cancelled() {
        return Err("Stopped.".into());
    }
    if status.success() {
        return Ok(());
    }
    let detail = errors
        .lock()
        .ok()
        .map(|errors| errors.trim().to_string())
        .filter(|errors| !errors.is_empty())
        .unwrap_or_else(|| format!("exited with {status}"));
    Err(super::transport::redact(&detail))
}

/// Forward a child's output line by line, bounding the length of any one line.
fn stream(source: impl Read, mut emit: impl FnMut(String)) {
    let mut reader = BufReader::new(source);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = crate::device::strip_ansi(&String::from_utf8_lossy(&line));
        let text = text.trim_end();
        if !text.is_empty() {
            emit(super::transport::redact(text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_custom_agent_is_never_offered_an_automatic_install() {
        let profile = ProviderProfile::custom("Mine");
        assert_eq!(readiness(&profile, &[]), Readiness::Manual);
    }

    #[test]
    fn sign_in_output_yields_the_last_url_to_open() {
        let lines = vec![
            "Starting local login server".to_string(),
            "Open https://auth.example.com/activate?id=abc to continue".to_string(),
            "Waiting…".to_string(),
        ];
        assert_eq!(
            sign_in_url(&lines).as_deref(),
            Some("https://auth.example.com/activate?id=abc")
        );
        assert_eq!(sign_in_url(&["no url here".to_string()]), None);
    }

    fn drain(receiver: &mut futures::channel::mpsc::UnboundedReceiver<JobEvent>) -> Vec<String> {
        let mut lines = Vec::new();
        while let Ok(JobEvent::Line(line)) = receiver.try_recv() {
            lines.push(line);
        }
        lines
    }

    #[cfg(unix)]
    #[test]
    fn a_managed_command_streams_its_output_and_explains_a_failure() {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let handle = JobHandle::default();
        let env = BTreeMap::new();

        let outcome = run(
            &PathBuf::from("/bin/echo"),
            &["first".into(), "line".into()],
            &env,
            &handle,
            &sender,
        );
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(drain(&mut receiver), vec!["first line".to_string()]);

        // A non-zero exit reports what the command printed, not just a code.
        let outcome = run(
            &PathBuf::from("/bin/sh"),
            &["-c".into(), "echo refused 1>&2; exit 3".into()],
            &env,
            &handle,
            &sender,
        );
        let error = outcome.expect_err("a failing command is an error");
        assert!(error.contains("refused"), "{error}");
        assert_eq!(drain(&mut receiver), vec!["refused".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_cancelled_command_reports_that_it_was_stopped() {
        let (sender, _receiver) = futures::channel::mpsc::unbounded();
        let handle = JobHandle::default();
        handle.cancel();
        let outcome = run(
            &PathBuf::from("/bin/echo"),
            &["ignored".into()],
            &BTreeMap::new(),
            &handle,
            &sender,
        );
        assert_eq!(outcome.unwrap_err(), "Stopped.");
    }

    /// Exercises the real `npm install -g` path. It changes the machine, so it
    /// never runs as part of the suite:
    /// `cargo test -- --ignored installs_a_builtin_agent`.
    #[test]
    #[ignore = "installs npm packages globally"]
    fn installs_a_builtin_agent_and_reports_the_binaries_it_provides() {
        let steps: Vec<InstallStep> = ProviderKind::Codex
            .install_steps()
            .iter()
            .copied()
            .filter(|step| !step.is_installed())
            .collect();
        assert!(
            !steps.is_empty(),
            "nothing to install; uninstall a Codex package first"
        );
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        install_steps(steps.clone(), JobHandle::default(), sender);

        let mut lines = Vec::new();
        let mut outcome = None;
        while let Ok(event) = receiver.try_recv() {
            match event {
                JobEvent::Line(line) => lines.push(line),
                JobEvent::Finished(result) => outcome = Some(result),
            }
        }
        let outcome = outcome.expect("the install reports an outcome");
        assert!(outcome.is_ok(), "{outcome:?}\n{}", lines.join("\n"));
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("$ npm install -g")),
            "the command it ran is shown: {lines:?}"
        );
        for step in steps {
            assert!(step.is_installed(), "`{}` is on PATH now", step.binary);
        }
    }

    #[test]
    fn a_missing_package_manager_is_reported_before_anything_runs() {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        if package_manager().is_err() {
            install_steps(Vec::new(), JobHandle::default(), sender);
            let event = receiver.try_recv().unwrap();
            assert!(matches!(event, JobEvent::Finished(Err(message)) if message.contains("npm")));
        }
    }
}
