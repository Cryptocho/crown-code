import unittest
import std/[os, strutils, tables]
import mcp/registry
import mcp/client

const mockServerPath = currentSourcePath().parentDir() / ".." / "build" / "test" / "mock_mcp_server"

suite "Nil safety":
  test "destroyRegistry on nil registry":
    destroyRegistry(nil)

  test "loadJsonConfig on nil registry":
    check loadJsonConfig(nil, "{}") == reConfigError

  test "getClient on nil registry":
    check getClient(nil, "test").isNil

  test "serverCount on nil registry":
    check serverCount(nil) == 0

  test "getLastError on nil registry":
    check registry.getLastError(nil) == "null registry"

  test "setStatusCallback on nil registry":
    setStatusCallback(nil, nil, nil)

  test "getServerNames on nil registry":
    check getServerNames(nil).len == 0

suite "Config parsing":
  test "valid JSON with stdio server":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"my-server": {"transport": "stdio", "command": "/path/to/server", "args": ["--flag"]}}}"""
    check loadJsonConfig(reg, jsonStr) == reOk
    check getServerNames(reg) == @["my-server"]
    destroyRegistry(reg)

  test "valid JSON with http server":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"http-server": {"transport": "http", "url": "https://example.com/mcp", "authToken": "token123"}}}"""
    check loadJsonConfig(reg, jsonStr) == reOk
    check getServerNames(reg) == @["http-server"]
    destroyRegistry(reg)

  test "valid JSON with both stdio and http":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"stdio-srv": {"transport": "stdio", "command": "/bin/echo"}, "http-srv": {"transport": "http", "url": "https://example.com/mcp"}}}"""
    check loadJsonConfig(reg, jsonStr) == reOk
    let names = getServerNames(reg)
    check names.len == 2
    check "stdio-srv" in names
    check "http-srv" in names
    destroyRegistry(reg)

  test "invalid JSON string":
    let reg = newMcpRegistry()
    check loadJsonConfig(reg, "{invalid") == reConfigError
    check getLastError(reg).len > 0
    destroyRegistry(reg)

  test "missing servers field":
    let reg = newMcpRegistry()
    check loadJsonConfig(reg, """{"other": {}}""") == reConfigError
    let errMsg = getLastError(reg)
    check errMsg.contains("servers")
    destroyRegistry(reg)

  test "stdio missing command":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"no-cmd": {"transport": "stdio"}}}"""
    check loadJsonConfig(reg, jsonStr) == reConfigError
    let errMsg = getLastError(reg)
    check errMsg.contains("command")
    destroyRegistry(reg)

  test "http missing url":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"no-url": {"transport": "http"}}}"""
    check loadJsonConfig(reg, jsonStr) == reConfigError
    let errMsg = getLastError(reg)
    check errMsg.contains("url")
    destroyRegistry(reg)

  test "unknown transport value":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"bad": {"transport": "ws"}}}"""
    check loadJsonConfig(reg, jsonStr) == reConfigError
    let errMsg = getLastError(reg)
    check errMsg.contains("transport")
    destroyRegistry(reg)

  test "empty server name":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"": {"transport": "stdio", "command": "/bin/echo"}}}"""
    check loadJsonConfig(reg, jsonStr) == reConfigError
    let errMsg = getLastError(reg)
    check errMsg.contains("empty server")
    destroyRegistry(reg)

  test "enabled false parsed correctly":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"disabled-srv": {"transport": "stdio", "command": "/bin/echo", "enabled": false}}}"""
    check loadJsonConfig(reg, jsonStr) == reOk
    check reg.configs["disabled-srv"].enabled == false
    destroyRegistry(reg)

  test "enabled defaults to true":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"default-srv": {"transport": "stdio", "command": "/bin/echo"}}}"""
    check loadJsonConfig(reg, jsonStr) == reOk
    check reg.configs["default-srv"].enabled == true
    destroyRegistry(reg)

suite "Server names":
  test "getServerNames returns correct list":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"a": {"command": "/bin/a"}, "b": {"command": "/bin/b"}, "c": {"command": "/bin/c"}}}"""
    discard loadJsonConfig(reg, jsonStr)
    let names = getServerNames(reg)
    check names.len == 3
    check "a" in names
    check "b" in names
    check "c" in names
    destroyRegistry(reg)

  test "empty registry returns empty seq":
    let reg = newMcpRegistry()
    check getServerNames(reg).len == 0
    destroyRegistry(reg)

suite "Server count":
  test "empty registry returns 0":
    let reg = newMcpRegistry()
    check serverCount(reg) == 0
    destroyRegistry(reg)

  test "after loadJsonConfig returns correct count":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"a": {"command": "/bin/a"}, "b": {"command": "/bin/b"}}}"""
    discard loadJsonConfig(reg, jsonStr)
    check serverCount(reg) == 0
    destroyRegistry(reg)

suite "Get client":
  test "unknown server returns nil and sets lastError":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"known": {"command": "/bin/echo"}}}"""
    discard loadJsonConfig(reg, jsonStr)
    let c = getClient(reg, "unknown")
    check c.isNil
    let errMsg = getLastError(reg)
    check errMsg.contains("not found")
    destroyRegistry(reg)

  test "disabled server returns nil and sets lastError":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"disabled-srv": {"command": "/bin/echo", "enabled": false}}}"""
    discard loadJsonConfig(reg, jsonStr)
    let c = getClient(reg, "disabled-srv")
    check c.isNil
    let errMsg = getLastError(reg)
    check errMsg.contains("disabled")
    destroyRegistry(reg)

  test "normal stdio server connection":
    if not fileExists(mockServerPath):
      skip()
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"mock": {"command": """" & mockServerPath & """", "requestTimeoutMs": 5000}}}"""
    discard loadJsonConfig(reg, jsonStr)
    let c = getClient(reg, "mock")
    check not c.isNil
    check getState(c) == csConnected
    destroyRegistry(reg)

  test "repeated getClient returns same instance":
    if not fileExists(mockServerPath):
      skip()
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"mock": {"command": """" & mockServerPath & """", "requestTimeoutMs": 5000}}}"""
    discard loadJsonConfig(reg, jsonStr)
    let c1 = getClient(reg, "mock")
    let c2 = getClient(reg, "mock")
    check c1 == c2
    destroyRegistry(reg)

suite "Status callback":
  test "setStatusCallback does not crash":
    let reg = newMcpRegistry()
    proc dummyCb(serverName: string, state: McpConnectionState,
                  errorMessage: string, userData: pointer) {.gcsafe.} =
      discard
    setStatusCallback(reg, dummyCb, nil)
    destroyRegistry(reg)

suite "Error handling":
  test "getLastError returns correct error message":
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"known": {"command": "/bin/echo"}}}"""
    discard loadJsonConfig(reg, jsonStr)
    discard getClient(reg, "unknown")
    let err = getLastError(reg)
    check err.len > 0
    check err.contains("not found")
    destroyRegistry(reg)

suite "Lifecycle":
  test "destroyRegistry cleans up clients":
    if not fileExists(mockServerPath):
      skip()
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"mock": {"command": """" & mockServerPath & """", "requestTimeoutMs": 5000}}}"""
    discard loadJsonConfig(reg, jsonStr)
    discard getClient(reg, "mock")
    check serverCount(reg) == 1
    destroyRegistry(reg)
    check serverCount(reg) == 0

  test "getClient after destroy returns nil":
    if not fileExists(mockServerPath):
      skip()
    let reg = newMcpRegistry()
    let jsonStr = """{"servers": {"mock": {"command": """" & mockServerPath & """", "requestTimeoutMs": 5000}}}"""
    discard loadJsonConfig(reg, jsonStr)
    discard getClient(reg, "mock")
    destroyRegistry(reg)
    check getClient(reg, "mock").isNil