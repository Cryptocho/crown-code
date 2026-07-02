import std/[json, locks, os, posix]
import mcp/jsonrpc
import mcp/transport_stdio
import mcp/transport_http

const
  DEFAULT_REQUEST_TIMEOUT* = 30_000
  DEFAULT_CONNECT_TIMEOUT* = 10_000
  DEFAULT_PING_INTERVAL* = 30
  DEFAULT_PONG_TIMEOUT* = 10
  DEFAULT_MAX_RECONNECT* = 3
  DEFAULT_RECONNECT_DELAY* = 1000
  MAX_RECONNECT_DELAY* = 60_000
  DEFAULT_PROTOCOL_VERSION* = "2024-11-05"
  DEFAULT_CLIENT_NAME* = "crown-code"
  DEFAULT_CLIENT_VERSION* = "0.1.0"

type
  McpTransportKind* = enum mcpStdio, mcpHttp

  McpConnectionState* = enum
    csDisconnected
    csConnecting
    csConnected
    csReconnecting
    csError

  McpContent* = object
    kind*: string
    text*: string
    data*: string
    mimeType*: string

  McpCallToolResult* = object
    content*: seq[McpContent]
    isError*: bool

  McpTool* = object
    name*: string
    description*: string

  McpClientConfig* = object
    transport*: McpTransportKind
    command*: string
    args*: seq[string]
    serverUrl*: string
    authToken*: string
    getToken*: proc(userData: pointer): string
    refreshToken*: proc(userData: pointer): string
    authUserData*: pointer
    requestTimeoutMs*: int
    connectTimeoutMs*: int
    pingIntervalSec*: int
    pongTimeoutSec*: int
    maxReconnect*: int
    reconnectDelayMs*: int
    onDisconnect*: proc(userData: pointer)
    onReconnect*: proc(userData: pointer)
    callbackUserData*: pointer
    protocolVersion*: string
    clientName*: string
    clientVersion*: string

  McpClient* {.acyclic.} = ref object
    config*: McpClientConfig
    transportKind*: McpTransportKind
    stdio: StdioTransport
    http: HttpTransport
    state*: McpConnectionState
    lastError*: string
    requestIdCounter: int64
    initialized: bool
    heartbeatThread: Thread[McpClient]
    heartbeatRunning: bool
    stateLock: Lock
    transportLock: Lock

proc sendJsonRpc(c: McpClient, meth: string, params: JsonNode): JsonNode =
  acquire(c.transportLock)
  defer: release(c.transportLock)
  let timeoutMs = if c.config.requestTimeoutMs > 0: c.config.requestTimeoutMs else: DEFAULT_REQUEST_TIMEOUT

  var id: int64
  acquire(c.stateLock)
  id = c.requestIdCounter
  inc c.requestIdCounter
  release(c.stateLock)

  let reqJson = buildRequest(meth, params, id)

  case c.transportKind
  of mcpStdio:
    let writeErr = writeJsonLine(c.stdio, reqJson, timeoutMs)
    if writeErr != teOk:
      acquire(c.stateLock)
      c.lastError = "sendJsonRpc: write error: " & $writeErr
      release(c.stateLock)
      return nil

    let readResult = readJsonLine(c.stdio, timeoutMs)
    if readResult.error != teOk:
      acquire(c.stateLock)
      c.lastError = "sendJsonRpc: read error: " & $readResult.error
      release(c.stateLock)
      return nil

    try:
      let resp = parseResponse(readResult.line)
      if resp.hasKey("error"):
        acquire(c.stateLock)
        c.lastError = "sendJsonRpc: " & resp["error"]{"message"}.getStr("unknown error")
        release(c.stateLock)
        return nil
      if resp.hasKey("id") and resp["id"].getInt() != id:
        acquire(c.stateLock)
        c.lastError = "sendJsonRpc: response id mismatch"
        release(c.stateLock)
        return nil
      return resp{"result"}
    except JsonParsingError:
      acquire(c.stateLock)
      c.lastError = "sendJsonRpc: invalid JSON response"
      release(c.stateLock)
      return nil

  of mcpHttp:
    close(c.http)
    if not connect(c.http):
      acquire(c.stateLock)
      c.lastError = "sendJsonRpc: " & c.http.lastError
      release(c.stateLock)
      return nil

    let httpResp = postJson(c.http, reqJson)
    if httpResp.statusCode == 401:
      {.cast(gcsafe).}:
        if c.config.refreshToken != nil:
          let newToken = c.config.refreshToken(c.config.authUserData)
          if newToken.len > 0:
            c.http.bearerToken = newToken
            c.config.authToken = newToken
            close(c.http)
            if connect(c.http):
              let retryResp = postJson(c.http, reqJson)
              if retryResp.statusCode == 200 and retryResp.error.len == 0:
                try:
                  let rj = parseJson(retryResp.body)
                  if rj.hasKey("error"):
                    acquire(c.stateLock)
                    c.lastError = "sendJsonRpc: " & rj["error"]{"message"}.getStr("unknown error")
                    release(c.stateLock)
                    return nil
                  if rj.hasKey("id") and rj["id"].getInt() != id:
                    acquire(c.stateLock)
                    c.lastError = "sendJsonRpc: response id mismatch"
                    release(c.stateLock)
                    return nil
                  return rj{"result"}
                except JsonParsingError:
                  discard
      acquire(c.stateLock)
      c.lastError = "sendJsonRpc: HTTP 401 (token rejected)"
      release(c.stateLock)
      return nil

    if httpResp.statusCode != 200 or httpResp.error.len > 0:
      acquire(c.stateLock)
      c.lastError = "sendJsonRpc: " & (if httpResp.error.len > 0: httpResp.error else: "HTTP " & $httpResp.statusCode)
      release(c.stateLock)
      return nil

    try:
      let resp = parseJson(httpResp.body)
      if resp.hasKey("error"):
        acquire(c.stateLock)
        c.lastError = "sendJsonRpc: " & resp["error"]{"message"}.getStr("unknown error")
        release(c.stateLock)
        return nil
      if resp.hasKey("id") and resp["id"].getInt() != id:
        acquire(c.stateLock)
        c.lastError = "sendJsonRpc: response id mismatch"
        release(c.stateLock)
        return nil
      return resp{"result"}
    except JsonParsingError:
      acquire(c.stateLock)
      c.lastError = "sendJsonRpc: invalid JSON response"
      release(c.stateLock)
      return nil

proc sendNotification(c: McpClient, meth: string, params: JsonNode): bool =
  acquire(c.transportLock)
  defer: release(c.transportLock)
  let timeoutMs = if c.config.requestTimeoutMs > 0: c.config.requestTimeoutMs else: DEFAULT_REQUEST_TIMEOUT
  let notifJson = buildNotification(meth, params)

  case c.transportKind
  of mcpStdio:
    let err = writeJsonLine(c.stdio, notifJson, timeoutMs)
    return err == teOk
  of mcpHttp:
    close(c.http)
    if not connect(c.http):
      return false
    let httpResp = postJson(c.http, notifJson)
    return httpResp.statusCode == 200 and httpResp.error.len == 0

proc initialize(c: McpClient): bool =
  let params = %*{
    "protocolVersion": c.config.protocolVersion,
    "clientInfo": {"name": c.config.clientName, "version": c.config.clientVersion},
    "capabilities": {}
  }
  let resp = sendJsonRpc(c, "initialize", params)
  if resp.isNil:
    return false
  if not sendNotification(c, "notifications/initialized", newJNull()):
    acquire(c.stateLock)
    c.lastError = "initialize: notification failed"
    release(c.stateLock)
    return false
  c.initialized = true
  return true

proc reconnect(c: McpClient): bool =
  acquire(c.stateLock)
  c.state = csReconnecting
  release(c.stateLock)

  case c.transportKind
  of mcpStdio: close(c.stdio)
  of mcpHttp: close(c.http)

  let delayMs = if c.config.reconnectDelayMs > 0: c.config.reconnectDelayMs else: DEFAULT_RECONNECT_DELAY
  let maxRetries = if c.config.maxReconnect > 0: c.config.maxReconnect else: DEFAULT_MAX_RECONNECT
  var currentDelay = delayMs

  for attempt in 1..maxRetries:
    os.sleep(currentDelay)

    case c.transportKind
    of mcpStdio:
      let t = startStdioTransport(c.config.command, c.config.args)
      if t.isNil or t.readFd < 0:
        currentDelay = min(currentDelay * 2, MAX_RECONNECT_DELAY)
        continue
      c.stdio = t
    of mcpHttp:
      let t = newHttpTransport(c.config.serverUrl, c.config.authToken)
      c.http = t
      if not connect(t):
        currentDelay = min(currentDelay * 2, MAX_RECONNECT_DELAY)
        continue

    if initialize(c):
      acquire(c.stateLock)
      c.state = csConnected
      release(c.stateLock)
      return true

    currentDelay = min(currentDelay * 2, MAX_RECONNECT_DELAY)

  acquire(c.stateLock)
  c.state = csError
  c.lastError = "reconnect failed after " & $maxRetries & " attempts"
  release(c.stateLock)
  return false

proc heartbeatProc(client: McpClient) {.thread.} =
  let pingIntervalMs = (if client.config.pingIntervalSec > 0: client.config.pingIntervalSec else: DEFAULT_PING_INTERVAL) * 1000

  while true:
    var waited = 0
    while waited < pingIntervalMs:
      os.sleep(100)
      waited += 100
      acquire(client.stateLock)
      let running = client.heartbeatRunning
      release(client.stateLock)
      if not running:
        return

    acquire(client.stateLock)
    let running = client.heartbeatRunning
    let state = client.state
    release(client.stateLock)

    if not running:
      return
    if state != csConnected:
      continue

    let resp = sendJsonRpc(client, "ping", newJNull())
    if resp.isNil:
      var shouldReconnect = false
      acquire(client.stateLock)
      if client.state == csConnected:
        shouldReconnect = true
      release(client.stateLock)
      if shouldReconnect:
        {.cast(gcsafe).}:
          if client.config.onDisconnect != nil:
            client.config.onDisconnect(client.config.callbackUserData)
      if reconnect(client):
        {.cast(gcsafe).}:
          if client.config.onReconnect != nil:
            client.config.onReconnect(client.config.callbackUserData)

proc newMcpClient*(config: McpClientConfig): McpClient =
  result = McpClient(
    config: config,
    transportKind: config.transport,
    state: csDisconnected,
    requestIdCounter: 1,
  )
  initLock(result.stateLock)
  initLock(result.transportLock)

  posix.signal(posix.SIGPIPE, posix.SIG_IGN)

  if result.config.requestTimeoutMs <= 0: result.config.requestTimeoutMs = DEFAULT_REQUEST_TIMEOUT
  if result.config.connectTimeoutMs <= 0: result.config.connectTimeoutMs = DEFAULT_CONNECT_TIMEOUT
  if result.config.pingIntervalSec <= 0: result.config.pingIntervalSec = DEFAULT_PING_INTERVAL
  if result.config.pongTimeoutSec <= 0: result.config.pongTimeoutSec = DEFAULT_PONG_TIMEOUT
  if result.config.maxReconnect <= 0: result.config.maxReconnect = DEFAULT_MAX_RECONNECT
  if result.config.reconnectDelayMs <= 0: result.config.reconnectDelayMs = DEFAULT_RECONNECT_DELAY
  if result.config.protocolVersion.len == 0: result.config.protocolVersion = DEFAULT_PROTOCOL_VERSION
  if result.config.clientName.len == 0: result.config.clientName = DEFAULT_CLIENT_NAME
  if result.config.clientVersion.len == 0: result.config.clientVersion = DEFAULT_CLIENT_VERSION

  result.state = csConnecting

  case config.transport
  of mcpStdio:
    let t = startStdioTransport(config.command, config.args)
    if t.isNil or t.readFd < 0:
      result.lastError = if not t.isNil: t.lastError else: "failed to start stdio transport"
      result.state = csError
      return
    result.stdio = t
  of mcpHttp:
    let t = newHttpTransport(config.serverUrl, config.authToken)
    result.http = t
    if not connect(t):
      result.lastError = t.lastError
      result.state = csError
      return

  if not initialize(result):
    result.state = csError
    return

  result.state = csConnected
  result.initialized = true
  result.heartbeatRunning = true
  try:
    createThread(result.heartbeatThread, heartbeatProc, result)
  except ResourceExhaustedError:
    result.lastError = "failed to create heartbeat thread"
    result.state = csError

proc callTool*(c: McpClient, toolName: string, arguments: JsonNode): McpCallToolResult =
  if c.isNil: return McpCallToolResult()

  var args = arguments
  if args.isNil:
    args = newJObject()

  acquire(c.stateLock)
  if c.state != csConnected:
    c.lastError = "client not connected"
    release(c.stateLock)
    return McpCallToolResult()
  release(c.stateLock)

  let params = %*{"name": toolName, "arguments": args}
  let resp = sendJsonRpc(c, "tools/call", params)
  if resp.isNil:
    return McpCallToolResult()

  var mcpResult = McpCallToolResult()
  let contentArray = resp{"content"}
  if contentArray.isNil or contentArray.kind != JArray:
    mcpResult.isError = resp{"isError"}.getBool(false)
    return mcpResult

  for item in contentArray.items():
    var content = McpContent()
    content.kind = item{"type"}.getStr("")
    if content.kind == "text" or content.kind == "resource":
      content.text = item{"text"}.getStr("")
    elif content.kind == "image":
      content.data = item{"data"}.getStr("")
      content.mimeType = item{"mimeType"}.getStr("")
    mcpResult.content.add(content)

  mcpResult.isError = resp{"isError"}.getBool(false)
  return mcpResult

proc listTools*(c: McpClient): seq[McpTool] =
  if c.isNil: return @[]

  acquire(c.stateLock)
  if c.state != csConnected:
    c.lastError = "client not connected"
    release(c.stateLock)
    return @[]
  release(c.stateLock)

  let resp = sendJsonRpc(c, "tools/list", newJNull())
  if resp.isNil:
    return @[]

  let toolsArray = resp{"tools"}
  if toolsArray.isNil or toolsArray.kind != JArray:
    return @[]

  result = @[]
  for item in toolsArray.items():
    var tool = McpTool()
    tool.name = item{"name"}.getStr("")
    tool.description = item{"description"}.getStr("")
    result.add(tool)

proc getState*(c: McpClient): McpConnectionState =
  if c.isNil: return csDisconnected
  acquire(c.stateLock)
  let s = c.state
  release(c.stateLock)
  return s

proc getLastError*(c: McpClient): string =
  if c.isNil: return "null client"
  acquire(c.stateLock)
  let err = c.lastError
  release(c.stateLock)
  return err

proc destroyMcpClient*(c: McpClient) =
  if c.isNil: return

  acquire(c.stateLock)
  c.heartbeatRunning = false
  release(c.stateLock)

  case c.transportKind
  of mcpStdio: close(c.stdio)
  of mcpHttp: close(c.http)

  if c.heartbeatThread.running:
    joinThread(c.heartbeatThread)

  deinitLock(c.stateLock)
  deinitLock(c.transportLock)
