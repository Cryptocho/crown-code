import std/[net, strutils, tables, uri, posix, monotimes, times]
import mcp/sse

const
  DEFAULT_HTTP_TIMEOUT_MS* = 30_000
  MAX_RESPONSE_SIZE* = 10 * 1024 * 1024
  SSE_READ_TIMEOUT_MS = 120_000
  HTTP_PORT = 80
  HTTPS_PORT = 443

type
  HttpResponse* = object
    statusCode*: int
    headers*: TableRef[string, string]
    body*: string
    error*: string
    events*: seq[SseEvent]

  HttpTransport* = ref object
    host: string
    port: int
    tls: bool
    basePath: string
    bearerToken*: string
    connected: bool
    lastError*: string
    socket*: Socket

func parseUrl*(url: string): tuple[host: string, port: int, tls: bool, path: string] {.raises: [].} =
  var u = parseUri(url)
  if u.scheme == "":
    u = parseUri("http://" & url)
  result.tls = u.scheme == "https"
  result.host = u.hostname
  let defaultPort = if result.tls: HTTPS_PORT else: HTTP_PORT
  if u.port.len > 0:
    var p = 0
    try:
      p = parseInt(u.port)
    except CatchableError:
      discard
    result.port = if p > 0: p else: defaultPort
  else:
    result.port = defaultPort
  result.path = if u.path.len > 0: u.path else: "/"

func buildHttpRequest*(host: string, port: int, path, body, token: string): string =
  result = "POST " & path & " HTTP/1.1\r\n"
  let hostHeader = if port == 80 or port == 443: host else: host & ":" & $port
  result.add("Host: " & hostHeader & "\r\n")
  result.add("Content-Type: application/json\r\n")
  result.add("Accept: application/json, text/event-stream\r\n")
  if token.len > 0:
    result.add("Authorization: Bearer " & token & "\r\n")
  result.add("Content-Length: " & $body.len & "\r\n")
  result.add("Connection: close\r\n")
  result.add("\r\n")
  result.add(body)

func parseHttpResponse*(raw: string): HttpResponse =
  let lines = raw.splitLines()
  if lines.len == 0:
    return HttpResponse(statusCode: 0, error: "empty response", headers: newTable[string, string](), body: "")
  let statusParts = lines[0].split()
  if statusParts.len < 2:
    return HttpResponse(statusCode: 0, error: "invalid status line: " & lines[0], headers: newTable[string, string](), body: "")
  try:
    result.statusCode = parseInt(statusParts[1])
  except CatchableError:
    result.statusCode = 0
  result.headers = newTable[string, string]()
  var bodyStart = 1
  while bodyStart < lines.len:
    if lines[bodyStart].len == 0:
      bodyStart.inc
      break
    let colonPos = lines[bodyStart].find(':')
    if colonPos >= 0:
      let key = lines[bodyStart][0 ..< colonPos].strip().toLowerAscii()
      let value = lines[bodyStart][colonPos + 1 .. ^1].strip()
      result.headers[key] = value
    bodyStart.inc
  result.body = lines[bodyStart .. ^1].join("\n")

proc readFixedBody*(socket: Socket, contentLength: int): string =
  let targetLen = min(contentLength, MAX_RESPONSE_SIZE)
  result = newString(targetLen)
  var received = 0
  while received < targetLen:
    let toRead = min(4096, targetLen - received)
    let n = recv(socket, addr(result[received]), toRead)
    if n <= 0: break
    received += n
  if contentLength > MAX_RESPONSE_SIZE:
    result.setLen(MAX_RESPONSE_SIZE)

proc readChunkedBody*(socket: Socket): string =
  result = ""
  while true:
    let line = recvLine(socket)
    var chunkSizeStr = line.strip()
    let semi = chunkSizeStr.find(';')
    if semi >= 0:
      chunkSizeStr = chunkSizeStr[0 ..< semi]
    chunkSizeStr = chunkSizeStr.strip()
    if chunkSizeStr.len == 0 and line.len == 0:
      break
    var chunkSize: int
    try:
      chunkSize = parseHexInt(chunkSizeStr)
    except CatchableError:
      chunkSize = 0
    if chunkSize == 0:
      while true:
        let trailerLine = recvLine(socket)
        if trailerLine.len == 0: break
      break
    var chunk = newString(chunkSize)
    var readSoFar = 0
    while readSoFar < chunkSize:
      let n = recv(socket, addr(chunk[readSoFar]), chunkSize - readSoFar)
      if n <= 0: break
      readSoFar += n
    discard recvLine(socket)
    if result.len + chunk.len > MAX_RESPONSE_SIZE:
      result.setLen(MAX_RESPONSE_SIZE)
      return
    result.add(chunk)

proc newHttpTransport*(url: string, bearerToken: string = ""): HttpTransport {.raises: [].} =
  let (host, port, tls, path) = parseUrl(url)
  result = HttpTransport(host: host, port: port, tls: tls, basePath: path,
                         bearerToken: bearerToken, connected: false, lastError: "")

proc isConnected*(t: HttpTransport): bool =
  if t.isNil: return false
  t.connected

proc close*(t: HttpTransport) =
  if t.isNil: return
  try:
    if not t.socket.isNil:
      t.socket.close()
  except Exception:
    discard
  t.connected = false

proc connect*(t: HttpTransport): bool =
  if t.isNil or t.connected: return t.connected
  var s: Socket = nil
  try:
    s = newSocket(Domain.AF_UNSPEC, SockType.SOCK_STREAM, Protocol.IPPROTO_TCP)
    s.connect(t.host, Port(t.port))
  except Exception:
    if not s.isNil:
      try: s.close() except Exception: discard
    t.lastError = "connect failed to " & t.host & ":" & $t.port & ": " & getCurrentExceptionMsg()
    return false
  if t.tls:
    var ctx: SslContext
    try:
      ctx = newContext(verifyMode = CVerifyNone)
      wrapConnectedSocket(ctx, s, handshakeAsClient, t.host)
    except Exception:
      if not s.isNil:
        try: s.close() except Exception: discard
      t.lastError = "TLS handshake failed for " & t.host & ": " & getCurrentExceptionMsg()
      return false
  t.socket = s
  t.connected = true
  return true

proc waitReadable(socket: Socket, timeoutMs: int): bool =
  if timeoutMs <= 0: return true
  let fd = socket.getFd()
  var readfds: TFdSet
  FD_ZERO(readfds); FD_SET(fd, readfds)
  var tv: Timeval
  tv.tv_sec = cast[posix.Time](clong(timeoutMs div 1000))
  tv.tv_usec = ((timeoutMs mod 1000) * 1000).clong
  let selRet = posix.select(fd.cint + 1, addr(readfds), nil, nil, addr(tv))
  if selRet > 0: return true
  false

proc readSseResponse(socket: Socket, timeoutMs: int): seq[SseEvent] =
  result = @[]
  var parser = newSseParser()
  let deadline = getMonoTime() + initDuration(milliseconds = timeoutMs)
  var buf: array[4096, char]
  while true:
    let remMs = (deadline - getMonoTime()).inMilliseconds
    if remMs <= 0:
      for evt in parser.flush(): result.add(evt)
      break
    if not waitReadable(socket, min(1000, remMs).int):
      for evt in parser.flush(): result.add(evt)
      break
    let n = recv(socket, addr(buf), sizeof(buf))
    if n <= 0:
      for evt in parser.flush(): result.add(evt)
      break
    var chunk = newString(n)
    copyMem(chunk.cstring, addr(buf), n)
    for evt in parser.feed(chunk):
      result.add(evt)

proc postJson*(t: HttpTransport, jsonBody: string): HttpResponse =
  if t.isNil or not t.connected:
    return HttpResponse(statusCode: 0, error: "not connected",
                       headers: newTable[string, string](), body: "")
  let request = buildHttpRequest(t.host, t.port, t.basePath, jsonBody, t.bearerToken)
  if not waitReadable(t.socket, DEFAULT_HTTP_TIMEOUT_MS):
    return HttpResponse(statusCode: 0, error: "socket timeout before send",
                       headers: newTable[string, string](), body: "")
  try:
    let sent = t.socket.send(cstring(request), request.len)
    if sent < 0:
      return HttpResponse(statusCode: 0, error: "send failed",
                         headers: newTable[string, string](), body: "")
  except Exception:
    return HttpResponse(statusCode: 0, error: "send failed: " & getCurrentExceptionMsg(),
                       headers: newTable[string, string](), body: "")
  if not waitReadable(t.socket, DEFAULT_HTTP_TIMEOUT_MS):
    return HttpResponse(statusCode: 0, error: "socket timeout reading status",
                       headers: newTable[string, string](), body: "")
  let statusLine = t.socket.recvLine()
  let statusParts = statusLine.split()
  var statusCode = 0
  if statusParts.len >= 2:
    try:
      statusCode = parseInt(statusParts[1])
    except CatchableError:
      discard
  result.statusCode = statusCode
  result.headers = newTable[string, string]()
  while true:
    let headerLine = t.socket.recvLine()
    if headerLine.len == 0: break
    let colonPos = headerLine.find(':')
    if colonPos >= 0:
      let key = headerLine[0 ..< colonPos].strip().toLowerAscii()
      let value = headerLine[colonPos + 1 .. ^1].strip()
      result.headers[key] = value
  let contentType = result.headers.getOrDefault("content-type", "")
  let transferEncoding = result.headers.getOrDefault("transfer-encoding", "")
  let contentLengthStr = result.headers.getOrDefault("content-length", "")
  if contentType.startsWith("text/event-stream"):
    if transferEncoding.find("chunked") >= 0:
      result.error = "chunked SSE not supported"
      return
    result.events = readSseResponse(t.socket, SSE_READ_TIMEOUT_MS)
    return result
  elif transferEncoding.find("chunked") >= 0:
    result.body = readChunkedBody(t.socket)
  elif contentLengthStr.len > 0:
    var clen = 0
    try:
      clen = parseInt(contentLengthStr)
    except CatchableError:
      discard
    result.body = readFixedBody(t.socket, clen)
  else:
    var bodyLines: seq[string] = @[]
    while true:
      let line = t.socket.recvLine()
      if line.len == 0: break
      bodyLines.add(line)
    result.body = bodyLines.join("\n")
