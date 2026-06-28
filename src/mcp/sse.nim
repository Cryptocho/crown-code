import std/strutils

type
  SseEvent* = object
    event*: string
    data*: string
    id*: string

  SseParser* = ref object
    eventType: string
    dataLines: seq[string]
    eventId: string
    lastEventId: string
    retryValue: int
    buf: string
    ignoreBom: bool

proc newSseParser*(): SseParser =
  SseParser(
    eventType: "",
    dataLines: @[],
    eventId: "",
    lastEventId: "",
    retryValue: 0,
    buf: "",
    ignoreBom: true
  )

proc processLine(p: SseParser, line: string) =
  if line.len == 0:
    return
  if line[0] == ':':
    return
  let colonPos = line.find(':')
  var field: string
  var value: string
  if colonPos >= 0:
    field = line[0 ..< colonPos]
    value = line[colonPos + 1 .. ^1]
    if value.len > 0 and value[0] == ' ':
      value = value[1 .. ^1]
  else:
    field = line
    value = ""
  case field.toLowerAscii()
  of "event":
    p.eventType = value
  of "data":
    p.dataLines.add(value)
  of "id":
    if value.find('\0') < 0:
      p.eventId = value
      p.lastEventId = value
  of "retry":
    let trimmed = value.strip()
    if trimmed.len > 0:
      try:
        let n = parseInt(trimmed)
        if n >= 0:
          p.retryValue = n
      except CatchableError:
        discard
  else:
    discard

proc dispatchEvent(p: SseParser): SseEvent =
  result = SseEvent(
    event: if p.eventType.len > 0: p.eventType else: "message",
    data: p.dataLines.join("\n"),
    id: p.eventId
  )
  p.eventType = ""
  p.dataLines = @[]
  p.eventId = ""

proc feed*(p: SseParser, chunk: string): seq[SseEvent] =
  result = @[]
  if chunk.len == 0:
    return
  var s = chunk
  if p.ignoreBom:
    if s.len >= 3 and s[0] == '\xEF' and s[1] == '\xBB' and s[2] == '\xBF':
      s = s[3 .. ^1]
    p.ignoreBom = false
  if s.len == 0:
    return
  p.buf.add(s)
  p.buf = p.buf.replace("\r\n", "\n").replace("\r", "\n")
  let lastIdx = p.buf.rfind('\n')
  if lastIdx < 0:
    return
  let complete = p.buf[0 ..< lastIdx]
  if lastIdx + 1 < p.buf.len:
    p.buf = p.buf[lastIdx + 1 .. ^1]
  else:
    p.buf = ""
  for line in complete.split('\n'):
    if line.len == 0:
      if p.dataLines.len > 0:
        result.add(p.dispatchEvent())
    else:
      p.processLine(line)

proc flush*(p: SseParser): seq[SseEvent] =
  result = @[]
  if p.buf.len > 0:
    var s = p.buf
    p.buf = ""
    for line in s.split('\n'):
      if line.len == 0:
        if p.dataLines.len > 0:
          result.add(p.dispatchEvent())
      else:
        p.processLine(line)
  if p.dataLines.len > 0:
    result.add(p.dispatchEvent())

proc reset*(p: SseParser) =
  p.eventType = ""
  p.dataLines = @[]
  p.eventId = ""
  p.lastEventId = ""
  p.retryValue = 0
  p.buf = ""
  p.ignoreBom = true

proc lastEventId*(p: SseParser): string = p.lastEventId

proc reconnectionTime*(p: SseParser): int = p.retryValue
