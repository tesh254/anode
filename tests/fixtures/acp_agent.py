"""Deterministic ACP subprocess fixture. No model/provider/network access."""
import json
import os
import sys

mode = sys.argv[1]
prompt_id = None


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def result(request_id, value):
    send({"id": request_id, "result": value})


def update(value):
    send({"method": "session/update",
          "params": {"sessionId": "fixture-session", "update": value}})


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        assert request["params"]["protocolVersion"] == 1
        assert not request["params"]["clientCapabilities"].get("terminal")
        assert request["params"]["clientCapabilities"]["fs"]["readTextFile"]
        if mode == "malformed":
            print("this is not ACP JSON", flush=True)
            continue
        result(request["id"], {
            "protocolVersion": 99 if mode == "unsupported" else 1,
            "agentCapabilities": {},
            "agentInfo": {"name": "fixture", "version": "1.0"},
        })
        print("token=fixture-secret", file=sys.stderr, flush=True)
    elif method == "session/new":
        with open("agent-pid", "w") as pid_file:
            pid_file.write(str(os.getpid()))
        assert os.path.isabs(request["params"]["cwd"])
        bridge = request["params"]["mcpServers"][0]
        assert bridge["name"].startswith("anode-")
        assert bridge["args"] == ["--agent-mcp-bridge"]
        assert len(bridge["env"]) == 2
        result(request["id"], {"sessionId": "fixture-session"})
    elif method == "session/prompt":
        assert prompt_id is None, "Only one prompt may be active"
        prompt_id = request["id"]
        text = request["params"]["prompt"][0]["text"]
        if text == "permission":
            send({"id": "permission-1", "method": "session/request_permission",
                  "params": {"sessionId": "fixture-session",
                             "toolCall": {"toolCallId": "tool-1", "title": "Read firmware"},
                             "options": [{"optionId": "exact-allow-id", "name": "Allow once",
                                          "kind": "allow_once"}]}})
    elif request.get("id") == "permission-1":
        assert request["result"]["outcome"]["optionId"] == "exact-allow-id"
        update({"sessionUpdate": "future_optional_update", "data": "ignored"})
        update({"sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Hello 🦀"}})
        result(prompt_id, {"stopReason": "end_turn"})
        prompt_id = None
    elif method == "session/cancel":
        result(prompt_id, {"stopReason": "cancelled"})
        prompt_id = None
