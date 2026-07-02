import unittest
import std/[json, os]
import mcp/client

const mockServerPath = currentSourcePath().parentDir() / ".." / "build" / "test" / "mock_mcp_server"

suite "Null handling":
  test "newMcpClient with empty config returns error":
    let c = newMcpClient(McpClientConfig())
    check c.state == csError or c.state == csDisconnected
    destroyMcpClient(c)

  test "callTool on nil client":
    let result = callTool(nil, "test", newJObject())
    check result.content.len == 0
    check result.isError == false

  test "listTools on nil client":
    check listTools(nil).len == 0

  test "getState on nil client":
    check getState(nil) == csDisconnected

  test "getLastError on nil client":
    check getLastError(nil) == "null client"

  test "destroyMcpClient on nil client":
    destroyMcpClient(nil)

suite "Error state":
  test "nonexistent command produces error state":
    let config = McpClientConfig(transport: mcpStdio, command: "/nonexistent/path")
    let c = newMcpClient(config)
    check getState(c) == csError
    check getLastError(c).len > 0

  test "callTool on error client returns empty result":
    let config = McpClientConfig(transport: mcpStdio, command: "/nonexistent/path")
    let c = newMcpClient(config)
    let result = callTool(c, "echo", %*{"message": "hi"})
    check result.content.len == 0
    check result.isError == false

  test "listTools on error client returns empty seq":
    let config = McpClientConfig(transport: mcpStdio, command: "/nonexistent/path")
    let c = newMcpClient(config)
    check listTools(c).len == 0

  test "destroyMcpClient on error client does not crash":
    let config = McpClientConfig(transport: mcpStdio, command: "/nonexistent/path")
    let c = newMcpClient(config)
    destroyMcpClient(c)

suite "Default values":
  test "McpCallToolResult defaults":
    let r = McpCallToolResult()
    check r.content.len == 0
    check r.isError == false

  test "McpTool defaults":
    let t = McpTool()
    check t.name.len == 0
    check t.description.len == 0

suite "Mock server integration":
  test "connect and list tools":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    check getState(c) == csConnected

    let tools = listTools(c)
    check tools.len == 3
    check tools[0].name == "echo"
    check tools[1].name == "add"
    check tools[2].name == "greet"

    destroyMcpClient(c)

  test "call echo tool":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    let result = callTool(c, "echo", %*{"message": "hello world"})
    check result.content.len == 1
    check result.content[0].text == "hello world"
    check result.isError == false
    destroyMcpClient(c)

  test "call add tool":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    let result = callTool(c, "add", %*{"a": 3, "b": 4})
    check result.content.len == 1
    check result.content[0].text == "7"
    destroyMcpClient(c)

  test "call greet tool":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    let result = callTool(c, "greet", %*{"name": "Kilo"})
    check result.content.len == 1
    check result.content[0].text == "Hello, Kilo!"
    destroyMcpClient(c)

  test "call image_tool returns image content":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    let result = callTool(c, "image_tool", %*{})
    check result.content.len == 2
    check result.content[0].kind == "image"
    check result.content[0].data == "iVBORw0KGgo"
    check result.content[0].mimeType == "image/png"
    check result.content[1].kind == "text"
    destroyMcpClient(c)

  test "call error_tool returns isError true":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    let result = callTool(c, "error_tool", %*{})
    check result.content.len == 0
    check result.isError == true
    destroyMcpClient(c)

  test "call empty_tool returns empty content":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    let result = callTool(c, "empty_tool", %*{})
    check result.content.len == 0
    check result.isError == false
    destroyMcpClient(c)

  test "call unknown tool returns empty result":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 requestTimeoutMs: 5000, clientName: "test",
                                 clientVersion: "1.0")
    let c = newMcpClient(config)
    let result = callTool(c, "unknown_tool", %*{})
    check result.content.len == 0
    destroyMcpClient(c)

suite "Heartbeat lifecycle":
  test "heartbeat starts and stops cleanly":
    if not fileExists(mockServerPath):
      skip()
    let config = McpClientConfig(transport: mcpStdio, command: mockServerPath,
                                 pingIntervalSec: 1, requestTimeoutMs: 5000,
                                 clientName: "test", clientVersion: "1.0")
    let c = newMcpClient(config)
    check getState(c) == csConnected
    destroyMcpClient(c)
