//! Bounded agent terminal jobs, separate from the developer's interactive PTY.
use super::tools;
use serde_json::{json, Value};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

pub(super) struct TerminalJob {
    request_id: String,
    arguments: Value,
    output: Arc<Mutex<JobOutput>>,
    cancel: Arc<AtomicBool>,
    pid: u32,
}

struct JobOutput {
    text: Vec<u8>,
    total_bytes: usize,
    state: &'static str,
    exit_code: Option<i32>,
}

impl TerminalJob {
    pub(super) fn matches(&self, request: &str, arguments: &Value) -> bool {
        self.request_id == request && self.arguments == *arguments
    }

    pub(super) fn has_request(&self, request: &str) -> bool {
        self.request_id == request
    }

    pub(super) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        if self.output.lock().unwrap().state == "running" {
            terminate_group(self.pid);
        }
    }

    pub(super) fn status(&self, limit: usize) -> Value {
        let output = self.output.lock().unwrap();
        json!({"state":output.state,"exit_code":output.exit_code,
            "output":tools::tail(&String::from_utf8_lossy(&output.text),limit),"total_bytes":output.total_bytes,
            "truncated":output.total_bytes>limit,
            "inputs":"saved files; independent process, not the developer's interactive shell"})
    }

    pub(super) fn spawn(root: &std::path::Path, args: &Value) -> Result<Self, String> {
        let request_id = args["request_id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or("request_id is required (1–128 characters)")?
            .to_string();
        let binary = args["command"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or("command is required")?;
        let argv = args["args"]
            .as_array()
            .ok_or("args must be a string array")?;
        if argv.len() > 100
            || argv
                .iter()
                .any(|v| v.as_str().is_none_or(|s| s.len() > 8192))
        {
            return Err("args exceeds 100 strings of 8192 characters".into());
        }
        let executable = if std::path::Path::new(binary).is_absolute() {
            std::path::PathBuf::from(binary)
        } else {
            crate::toolchain::locate_executable(binary)
                .ok_or("Command not found; use an absolute executable path")?
        };
        let timeout = args["timeout_seconds"].as_u64().unwrap_or(30).clamp(1, 120);
        let mut command = Command::new(&executable);
        if let Some(path) = crate::toolchain::subprocess_path(&executable) {
            command.env("PATH", path);
        }
        command
            .args(argv.iter().filter_map(Value::as_str))
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        let output = Arc::new(Mutex::new(JobOutput {
            text: Vec::new(),
            total_bytes: 0,
            state: "running",
            exit_code: None,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let stdout = read_output(child.stdout.take().unwrap(), output.clone());
        let stderr = read_output(child.stderr.take().unwrap(), output.clone());
        let updates = output.clone();
        let cancelled = cancel.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(timeout);
            let (state, exit) = loop {
                if cancelled.load(Ordering::Acquire) {
                    terminate(&mut child);
                    break ("cancelled", None);
                }
                if Instant::now() >= deadline {
                    terminate(&mut child);
                    break ("timeout", None);
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Descendants must not retain output handles after the job exits.
                        terminate_group(child.id());
                        break (
                            if status.success() {
                                "completed"
                            } else {
                                "failed"
                            },
                            status.code(),
                        );
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                    Err(_) => {
                        terminate(&mut child);
                        break ("failed", None);
                    }
                }
            };
            let _ = stdout.join();
            let _ = stderr.join();
            let mut output = updates.lock().unwrap();
            output.state = state;
            output.exit_code = exit;
        });
        Ok(Self {
            request_id,
            arguments: args.clone(),
            output,
            cancel,
            pid,
        })
    }
}

fn terminate(child: &mut Child) {
    terminate_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

fn terminate_group(pid: u32) {
    #[cfg(unix)]
    // The child creates its own process group before exec. Never target Anode's group.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

fn read_output(
    mut reader: impl Read + Send + 'static,
    output: Arc<Mutex<JobOutput>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut bytes = [0; 4096];
        while let Ok(count) = reader.read(&mut bytes) {
            if count == 0 {
                break;
            }
            let mut output = output.lock().unwrap();
            output.total_bytes += count;
            output.text.extend_from_slice(&bytes[..count]);
            if output.text.len() > 256 * 1024 {
                let mut start = output.text.len() - 256 * 1024;
                while start < output.text.len() && output.text[start] & 0xc0 == 0x80 {
                    start += 1;
                }
                output.text.drain(..start);
            }
        }
    })
}

impl Drop for TerminalJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn await_exit(job: &TerminalJob) -> Value {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let value = job.status(1024);
            if value["state"] != "running" {
                return value;
            }
            assert!(Instant::now() < end, "Job did not finish");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[test]
    #[cfg(unix)]
    fn execution_captures_output_exit_and_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let job = TerminalJob::spawn(dir.path(), &json!({"request_id":"one","command":"/bin/sh","args":["-c","printf diagnostic; exit 7"]})).unwrap();
        let result = await_exit(&job);
        assert_eq!(result["exit_code"], 7);
        assert_eq!(result["output"], "diagnostic");
        let job = TerminalJob::spawn(
            dir.path(),
            &json!({"request_id":"two","command":"/bin/sh","args":["-c","sleep 30"]}),
        )
        .unwrap();
        job.cancel();
        assert_eq!(await_exit(&job)["state"], "cancelled");
    }
}
