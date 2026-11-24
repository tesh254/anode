//! A per-conversation authenticated loopback endpoint. The stdio subprocess is
//! only an MCP relay; all workspace reads/mutations execute in the running app.
use super::{tools, Event, ToolRequest};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{sync_channel, SyncSender},
    Arc,
};
use std::time::Duration;

const LIMIT: u64 = 1024 * 1024;

pub(crate) struct Bridge {
    address: String,
    token: String,
    active: Arc<AtomicBool>,
}

impl Bridge {
    pub(crate) fn start(events: SyncSender<Event>) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let token = uuid::Uuid::new_v4().to_string();
        let active = Arc::new(AtomicBool::new(true));
        let running = active.clone();
        let secret = token.clone();
        std::thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Accepted sockets can inherit the listener's
                        // nonblocking mode. Read the request with the timeout
                        // below, even if the client has not written it yet.
                        if stream.set_nonblocking(false).is_err() {
                            continue;
                        }
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        let request = read_json(&mut stream);
                        let result = match request {
                            Ok(request) if request["token"].as_str() == Some(&secret) => {
                                let (reply, result) = sync_channel(1);
                                let tool = ToolRequest {
                                    name: request["name"].as_str().unwrap_or_default().into(),
                                    arguments: request["arguments"].clone(),
                                    reply,
                                    deadline: std::time::Instant::now() + Duration::from_secs(120),
                                };
                                if events.try_send(Event::Tool(tool)).is_err() {
                                    tools::error(
                                        "disconnected",
                                        "Anode is not accepting tool requests",
                                    )
                                } else {
                                    // Approval may wait, but disconnect revokes the token and wakes
                                    // this loop promptly. A late queued request cannot execute itself.
                                    let deadline =
                                        std::time::Instant::now() + Duration::from_secs(120);
                                    loop {
                                        match result.recv_timeout(Duration::from_millis(100)) {
                                            Ok(value) => break value,
                                            Err(
                                                std::sync::mpsc::RecvTimeoutError::Disconnected,
                                            ) => {
                                                break tools::error(
                                                    "cancelled",
                                                    "Request cancelled",
                                                )
                                            }
                                            _ if !running.load(Ordering::Acquire) => {
                                                break tools::error(
                                                    "disconnected",
                                                    "Conversation ended",
                                                )
                                            }
                                            _ if std::time::Instant::now() >= deadline => {
                                                break tools::error(
                                                    "timeout",
                                                    "Tool request expired",
                                                )
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            _ => tools::error("permission_denied", "Invalid bridge credentials"),
                        };
                        let _ = writeln!(stream, "{result}");
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            token,
            active,
        })
    }

    pub(crate) fn mcp(&self) -> Result<agent_client_protocol::McpServer, String> {
        // Keep the token out of argv and diagnostics. Only the relay receives it.
        serde_json::from_value(json!({
            "name": format!("anode-{}", uuid::Uuid::new_v4()),
            "command": std::env::current_exe().map_err(|e| e.to_string())?,
            "args": ["--agent-mcp-bridge"],
            "env": [
                {"name":"ANODE_BRIDGE_ADDRESS","value":self.address},
                {"name":"ANODE_BRIDGE_TOKEN","value":self.token}
            ]
        }))
        .map_err(|e| e.to_string())
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

fn read_json(stream: &mut TcpStream) -> Result<Value, String> {
    let mut line = String::new();
    BufReader::new(stream.take(LIMIT + 1))
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if line.len() as u64 > LIMIT {
        return Err("Request too large".into());
    }
    serde_json::from_str(&line).map_err(|e| e.to_string())
}

/// Run the headless MCP relay before initializing GPUI, if requested by an ACP
/// session. Returns false for a normal application launch.
pub fn run_bridge_if_requested() -> bool {
    if !std::env::args().any(|a| a == "--agent-mcp-bridge") {
        return false;
    }
    let address = std::env::var("ANODE_BRIDGE_ADDRESS").unwrap_or_default();
    let token = std::env::var("ANODE_BRIDGE_TOKEN").unwrap_or_default();
    let mut input = BufReader::new(std::io::stdin());
    loop {
        let mut line = String::new();
        match input.by_ref().take(LIMIT + 1).read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) if line.len() as u64 > LIMIT => break,
            _ => {}
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        let result = match request["method"].as_str().unwrap_or_default() {
            "initialize" => json!({
                "protocolVersion":"2024-11-05", "capabilities":{"tools":{}},
                "serverInfo":{"name":"anode-workspace","version":env!("CARGO_PKG_VERSION")}
            }),
            "ping" => json!({}),
            "tools/list" => json!({"tools":tools::schemas()}),
            "tools/call" => {
                let value = relay(&address, &token, &request["params"])
                    .unwrap_or_else(|e| tools::error("disconnected", &e));
                json!({"content":[{"type":"text","text":value.to_string()}],
                    "isError":value["status"] == "error"})
            }
            _ => {
                println!(
                    "{}",
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})
                );
                continue;
            }
        };
        println!("{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
    }
    true
}

fn relay(address: &str, token: &str, params: &Value) -> Result<Value, String> {
    let address: std::net::SocketAddr = address.parse().map_err(|_| "Invalid bridge address")?;
    if !address.ip().is_loopback() || token.is_empty() {
        return Err("Invalid bridge credentials".into());
    }
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(125)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    writeln!(
        stream,
        "{}",
        json!({"token":token,"name":params["name"],"arguments":params["arguments"]})
    )
    .map_err(|e| e.to_string())?;
    read_json(&mut stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_waits_for_a_client_to_write_its_request() {
        let (events, received) = sync_channel(8);
        let bridge = Bridge::start(events).unwrap();
        let mut client = TcpStream::connect(&bridge.address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        // Allow the server to accept before sending any bytes. A socket left
        // nonblocking can reject this client before its request arrives.
        assert!(matches!(
            received.recv_timeout(Duration::from_millis(80)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        writeln!(
            client,
            "{}",
            json!({
                "token": bridge.token,
                "name": "anode_project_context",
                "arguments": {}
            })
        )
        .unwrap();
        let Event::Tool(tool) = received.recv_timeout(Duration::from_secs(2)).unwrap() else {
            panic!("Expected tool request")
        };
        tool.reply.send(json!({"status": "ok"})).unwrap();
        assert_eq!(read_json(&mut client).unwrap()["status"], "ok");
    }

    #[test]
    fn bridge_authenticates_and_forwards_only_to_its_conversation() {
        let (events, received) = sync_channel(8);
        let bridge = Bridge::start(events).unwrap();
        let denied = relay(
            &bridge.address,
            "wrong",
            &json!({"name":"anode_project_context","arguments":{}}),
        )
        .unwrap();
        assert_eq!(denied["error"]["code"], "permission_denied");
        assert!(received.try_recv().is_err());
        let address = bridge.address.clone();
        let token = bridge.token.clone();
        let caller = std::thread::spawn(move || {
            relay(
                &address,
                &token,
                &json!({"name":"anode_project_context","arguments":{}}),
            )
            .unwrap()
        });
        let Event::Tool(tool) = received.recv_timeout(Duration::from_secs(2)).unwrap() else {
            panic!("Expected tool request")
        };
        assert_eq!(tool.name, "anode_project_context");
        tool.reply
            .send(json!({"status":"ok","data":{"revision":"live"}}))
            .unwrap();
        assert_eq!(caller.join().unwrap()["data"]["revision"], "live");
        let active = bridge.active.clone();
        drop(bridge);
        assert!(!active.load(Ordering::Acquire));
    }
}
