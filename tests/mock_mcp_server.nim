import std/[json, os, strutils]

proc main() =
  for line in stdin.lines:
    let trimmed = line.strip()
    if trimmed.len == 0:
      continue

    let req = parseJson(trimmed)
    let msgId = req{"id"}
    if msgId.isNil or msgId.kind == JNull:
      continue

    let id = msgId
    let meth = req{"method"}.getStr("")

    var response: JsonNode

    case meth
    of "initialize":
      response = %*{"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": "2024-11-05", "serverInfo": {"name": "mock-mcp", "version": "1.0.0"}, "capabilities": {"tools": {}}}}
    of "tools/list":
      response = %*{"jsonrpc": "2.0", "id": id, "result": {"tools": [{"name": "echo", "description": "Echo back a message"}, {"name": "add", "description": "Add two numbers"}, {"name": "greet", "description": "Greet someone"}]}}
    of "tools/call":
      let params = req{"params"}
      let name = params{"name"}.getStr("")
      let toolArgs = params{"arguments"}
      case name
      of "echo":
        response = %*{"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": toolArgs{"message"}.getStr("")}]}}
      of "add":
        let a = toolArgs{"a"}.getInt(0)
        let b = toolArgs{"b"}.getInt(0)
        response = %*{"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": $(a + b)}]}}
      of "greet":
        let nameParam = toolArgs{"name"}.getStr("")
        response = %*{"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "Hello, " & nameParam & "!"}]}}
      of "image_tool":
        response = %*{"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "image", "data": "iVBORw0KGgo", "mimeType": "image/png"}, {"type": "text", "text": "image generated"}]}}
      of "error_tool":
        response = %*{"jsonrpc": "2.0", "id": id, "result": {"content": [], "isError": true}}
      of "empty_tool":
        response = %*{"jsonrpc": "2.0", "id": id, "result": {"content": [], "isError": false}}
      else:
        response = %*{"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Unknown tool: " & name}}
    of "ping":
      response = %*{"jsonrpc": "2.0", "id": id, "result": {}}
    else:
      response = %*{"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Unknown method: " & meth}}

    echo $response

when isMainModule:
  main()
