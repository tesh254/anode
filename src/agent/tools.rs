use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) const OUTPUT_LIMIT: usize = 32 * 1024;

pub(crate) fn head(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].into()
}

pub(crate) fn tail(text: &str, limit: usize) -> String {
    let mut start = text.len().saturating_sub(limit);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].into()
}

pub(crate) fn error(code: &str, message: &str) -> Value {
    json!({"status":"error","error":{"code":code,"message":message}})
}

pub(crate) fn scoped_path(root: &Path, path: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let candidate = root.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !candidate.starts_with(&root) {
        return Err("Path escapes the current project".into());
    }
    if !candidate.is_file() {
        return Err("Path is not a regular file".into());
    }
    Ok(candidate)
}

pub(crate) fn scoped_write_path(root: &Path, path: &str) -> Result<PathBuf, String> {
    let rel = Path::new(path);
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Path must be relative to the current project".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let target = root.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let canonical_parent = parent.canonicalize().map_err(|e| e.to_string())?;
        if !canonical_parent.starts_with(&root) {
            return Err("Path escapes the current project".into());
        }
    }
    if target.exists() {
        let canonical = target.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(&root) {
            return Err("Path escapes the current project".into());
        }
    }
    Ok(target)
}

pub(crate) fn schemas() -> Vec<Value> {
    let string = json!({"type":"string","maxLength":4096});
    let limit = json!({"type":"integer","minimum":1,"maximum":32768,"default":8192});
    let definitions = [
        ("anode_project_context", "Read current board, runtime, project and unsaved-buffer context.", json!({}), vec![]),
        ("anode_board_status", "Inspect selected board, detection confidence and current lease state. No verified pinout is implied.", json!({}), vec![]),
        ("anode_project_files", "List project files, including unsaved-buffer revisions.", json!({"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":200}}), vec![]),
        ("anode_project_read", "Read a live project buffer, falling back to a bounded disk read. Paths must stay inside the project.", json!({"path":string,"limit":limit}), vec!["path"]),
        ("anode_project_search", "Literal search of current project buffers. Results carry file and line numbers.", json!({"query":string,"limit":{"type":"integer","minimum":1,"maximum":100}}), vec!["query"]),
        ("anode_output_tail", "Read timestamped Output records after a cursor. Includes errors, compiler diagnostics and device progress.", json!({"cursor":{"type":"integer","minimum":0},"limit":limit}), vec![]),
        ("anode_repl_tail", "Read bounded existing REPL output and tracebacks without executing board code.", json!({"limit":limit}), vec![]),
        ("anode_terminal_tail", "Read captured project terminal output without typing into the user's shell.", json!({"limit":limit}), vec![]),
        ("anode_terminal_exec", "Request an independent terminal command in the project directory. Requires approval showing exact argv in Anode. This is an unrestricted local process; use read tools first. Returns a cancellable bounded job.", json!({"command":string,"args":{"type":"array","maxItems":100,"items":{"type":"string","maxLength":8192}},"timeout_seconds":{"type":"integer","minimum":1,"maximum":120},"context_revision":string,"request_id":string}), vec!["command","args","context_revision","request_id"]),
        ("anode_diagnostics", "Read compiler errors, warnings and toolchain availability.", json!({"limit":limit}), vec![]),
        ("anode_build", "Request a build of saved project files with a context revision. Requires approval in Anode; returns a job handle.", json!({"context_revision":string,"request_id":string}), vec!["context_revision","request_id"]),
        ("anode_job_status", "Inspect an Anode build job and its attributed output.", json!({"job_id":string}), vec!["job_id"]),
        ("anode_job_cancel", "Cancel an agent terminal job. Existing build subprocesses do not yet support cancellation.", json!({"job_id":string}), vec!["job_id"]),
        ("anode_project_templates", "List Anode's built-in project templates, runtimes and generated files.", json!({}), vec![]),
        ("anode_project_create", "Create a new Anode firmware project under ~/Documents/Anode from a template or custom files, optionally initialize git, and open it in the IDE.", json!({"name":string,"template_id":string,"files":{"type":"object","additionalProperties":{"type":"string"}},"init_git":{"type":"boolean","default":false},"open":{"type":"boolean","default":true}}), vec!["name"]),
        ("anode_project_open", "Open an existing project folder in Anode and switch the workspace to it.", json!({"path":string}), vec!["path"]),
        ("anode_project_write", "Write or update a file inside the currently open Anode project and sync the live editor.", json!({"path":string,"content":{"type":"string","maxLength":262144}}), vec!["path","content"]),
    ];
    definitions.into_iter().map(|(name, description, properties, required)| json!({
        "name":name,"description":description,
        "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}
    })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_unicode_reads_preserve_boundaries() {
        assert_eq!(head("abc🦀tail", 5), "abc");
        assert_eq!(tail("abc🦀tail", 6), "tail");
        assert!(head(&"🦀".repeat(100), 33).len() <= 33);
    }
    #[test]
    fn paths_reject_traversal_and_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        assert!(scoped_path(root.path(), outside.path().to_str().unwrap()).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
            assert!(scoped_path(root.path(), "escape").is_err());
        }
    }
}
