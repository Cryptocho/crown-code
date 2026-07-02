import std/[json, locks, tables]
import mcp/client

type
  McpRegistryError* = enum
    reOk
    reServerNotFound = -1
    reServerDisabled = -2
    reNotConnected = -3
    reConfigError = -4

  McpServerConfig* = object
    transport*: McpTransportKind
    command*: string
    args*: seq[string]
    serverUrl*: string
    authToken*: string
    enabled*: bool

  McpStatusCallback* = proc(serverName: string, state: McpConnectionState,
                             errorMessage: string, userData: pointer) {.gcsafe.}

  CallbackPayload = ref object
    registry: McpRegistry
    serverName: string

  McpRegistry* = ref object
    configs*: Table[string, McpServerConfig]
    clients: Table[string, McpClient]
    payloads: Table[string, CallbackPayload]
    statusCb: McpStatusCallback
    statusUserData: pointer
    lastError: string
    lock: Lock
    destroyed: bool

proc newMcpRegistry*(): McpRegistry =
  result = McpRegistry(
    configs: initTable[string, McpServerConfig](),
    clients: initTable[string, McpClient](),
    payloads: initTable[string, CallbackPayload](),
  )
  initLock(result.lock)

proc destroyRegistry*(reg: McpRegistry) =
  if reg.isNil or reg.destroyed: return
  reg.destroyed = true
  for name, client in reg.clients:
    destroyMcpClient(client)
  reg.clients.clear()
  reg.payloads.clear()
  reg.configs.clear()
  deinitLock(reg.lock)

proc serverCount*(reg: McpRegistry): int =
  if reg.isNil or reg.destroyed: return 0
  reg.clients.len

proc getLastError*(reg: McpRegistry): string =
  if reg.isNil or reg.destroyed: return "null registry"
  acquire(reg.lock)
  let err = reg.lastError
  release(reg.lock)
  err

proc setError(reg: McpRegistry, msg: string) =
  if reg.isNil or reg.destroyed: return
  acquire(reg.lock)
  reg.lastError = msg
  release(reg.lock)

proc getServerNames*(reg: McpRegistry): seq[string] =
  if reg.isNil or reg.destroyed: return @[]
  result = @[]
  for name in reg.configs.keys:
    result.add(name)

proc loadJsonConfig*(reg: McpRegistry, jsonStr: string): McpRegistryError =
  if reg.isNil or reg.destroyed: return reConfigError

  var root: JsonNode
  try:
    root = parseJson(jsonStr)
  except JsonParsingError:
    setError(reg, "invalid JSON: " & getCurrentExceptionMsg())
    return reConfigError
  except CatchableError:
    setError(reg, "invalid JSON: " & getCurrentExceptionMsg())
    return reConfigError

  let servers = root{"servers"}
  if servers.isNil or servers.kind != JObject:
    setError(reg, "missing 'servers' field")
    return reConfigError

  for serverName, serverVal in servers.fields:
    if serverName.len == 0:
      setError(reg, "empty server name")
      return reConfigError

    var config = McpServerConfig(enabled: true)

    let transportStr = serverVal{"transport"}.getStr("stdio")
    case transportStr
    of "stdio":
      config.transport = mcpStdio
    of "http":
      config.transport = mcpHttp
    else:
      setError(reg, "unknown transport '" & transportStr & "' for server '" & serverName & "'")
      return reConfigError

    config.command = serverVal{"command"}.getStr("")
    config.serverUrl = serverVal{"url"}.getStr("")
    config.authToken = serverVal{"authToken"}.getStr("")

    let argsVal = serverVal{"args"}
    if not argsVal.isNil and argsVal.kind == JArray:
      for arg in argsVal.items:
        config.args.add(arg.getStr(""))

    let enabledVal = serverVal{"enabled"}
    if not enabledVal.isNil and enabledVal.kind == JBool:
      config.enabled = enabledVal.getBool()

    if config.transport == mcpStdio and config.command.len == 0:
      setError(reg, "stdio server '" & serverName & "' missing 'command'")
      return reConfigError

    if config.transport == mcpHttp and config.serverUrl.len == 0:
      setError(reg, "http server '" & serverName & "' missing 'url'")
      return reConfigError

    reg.configs[serverName] = config

  return reOk

proc forwardDisconnect(userData: pointer) {.gcsafe.} =
  let payload = cast[CallbackPayload](userData)
  if payload.isNil: return
  let reg = payload.registry
  acquire(reg.lock)
  let cb = reg.statusCb
  let cbUserData = reg.statusUserData
  release(reg.lock)
  if cb != nil:
    cb(payload.serverName, csDisconnected, "connection lost", cbUserData)

proc forwardReconnect(userData: pointer) {.gcsafe.} =
  let payload = cast[CallbackPayload](userData)
  if payload.isNil: return
  let reg = payload.registry
  acquire(reg.lock)
  let cb = reg.statusCb
  let cbUserData = reg.statusUserData
  release(reg.lock)
  if cb != nil:
    cb(payload.serverName, csConnected, "", cbUserData)

proc getClient*(reg: McpRegistry, name: string): McpClient =
  if reg.isNil or reg.destroyed: return nil

  if name notin reg.configs:
    setError(reg, "server '" & name & "' not found")
    return nil

  let config = reg.configs[name]

  if not config.enabled:
    setError(reg, "server '" & name & "' is disabled")
    return nil

  let existing = reg.clients.getOrDefault(name)
  if not existing.isNil:
    return existing

  var clientConfig = McpClientConfig(
    transport: config.transport,
    command: config.command,
    args: config.args,
    serverUrl: config.serverUrl,
    authToken: config.authToken,
  )

  let payload = CallbackPayload(registry: reg, serverName: name)
  reg.payloads[name] = payload
  clientConfig.callbackUserData = cast[pointer](payload)
  clientConfig.onDisconnect = forwardDisconnect
  clientConfig.onReconnect = forwardReconnect

  let client = newMcpClient(clientConfig)
  reg.clients[name] = client
  return client

proc setStatusCallback*(reg: McpRegistry, cb: McpStatusCallback, userData: pointer) =
  if reg.isNil or reg.destroyed: return
  acquire(reg.lock)
  reg.statusCb = cb
  reg.statusUserData = userData
  release(reg.lock)