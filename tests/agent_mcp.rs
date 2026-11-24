//! Exercise the shipped executable's headless MCP entrypoint.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn headless_mcp_initializes_lists_only_implemented_tools_and_reports_disconnects() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_anode"))
        .arg("--agent-mcp-bridge")
        .env_remove("ANODE_BRIDGE_ADDRESS")
        .env_remove("ANODE_BRIDGE_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut call = |request: Value| {
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    let initialized = call(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}));
    assert_eq!(
        initialized["result"]["serverInfo"]["name"],
        "anode-workspace"
    );
    let listed = call(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let names: Vec<_> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"anode_repl_tail"));
    assert!(names.contains(&"anode_terminal_exec"));
    assert!(names.contains(&"anode_output_tail"));
    assert!(!names.contains(&"anode_flash_firmware"));
    assert!(!names.contains(&"anode_repl_eval"));
    let called = call(json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"anode_project_context","arguments":{}}}));
    assert_eq!(called["result"]["isError"], true);
    drop(call);
    drop(input);
    assert!(child.wait().unwrap().success());
}
