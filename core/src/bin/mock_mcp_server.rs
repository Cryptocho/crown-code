use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let trimmed = match line {
            Ok(l) => l.trim().to_string(),
            Err(_) => break,
        };
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(&trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_id = &req["id"];
        if msg_id.is_null() {
            continue;
        }

        let id = msg_id.clone();
        let method = req["method"].as_str().unwrap_or("").to_string();

        let response = match method.as_str() {
            "initialize" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {"name": "mock-mcp", "version": "1.0.0"},
                    "capabilities": {"tools": {}}
                }
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "tools": [
                        {"name": "echo", "description": "Echo back a message"},
                        {"name": "add", "description": "Add two numbers"},
                        {"name": "greet", "description": "Greet someone"}
                    ]
                }
            }),
            "tools/call" => {
                let params = &req["params"];
                let name = params["name"].as_str().unwrap_or("");
                let tool_args = &params["arguments"];
                match name {
                    "echo" => json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {
                            "content": [{"type": "text", "text": tool_args["message"].as_str().unwrap_or("")}]
                        }
                    }),
                    "add" => {
                        let a = tool_args["a"].as_i64().unwrap_or(0);
                        let b = tool_args["b"].as_i64().unwrap_or(0);
                        json!({
                            "jsonrpc": "2.0", "id": id,
                            "result": {
                                "content": [{"type": "text", "text": (a + b).to_string()}]
                            }
                        })
                    }
                    "greet" => {
                        let name_param = tool_args["name"].as_str().unwrap_or("");
                        json!({
                            "jsonrpc": "2.0", "id": id,
                            "result": {
                                "content": [{"type": "text", "text": format!("Hello, {}!", name_param)}]
                            }
                        })
                    }
                    "image_tool" => json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {
                            "content": [
                                {"type": "image", "data": "iVBORw0KGgo", "mimeType": "image/png"},
                                {"type": "text", "text": "image generated"}
                            ]
                        }
                    }),
                    "error_tool" => json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {"content": [], "isError": true}
                    }),
                    "empty_tool" => json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {"content": [], "isError": false}
                    }),
                    _ => json!({
                        "jsonrpc": "2.0", "id": id,
                        "error": {"code": -32601, "message": format!("Unknown tool: {}", name)}
                    }),
                }
            }
            "ping" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {}
            }),
            _ => json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": format!("Unknown method: {}", method)}
            }),
        };

        let output = response.to_string();
        let _ = writeln!(io::stdout(), "{}", output);
        let _ = io::stdout().flush();
    }
}