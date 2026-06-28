import unittest
import std/[strutils, tables]
import mcp/sse
import mcp/transport_http

proc parseEvents(text: string): seq[SseEvent] =
  var p = newSseParser()
  result = p.feed(text)
  for e in p.flush():
    result.add(e)

template checkEvent(evt: SseEvent; expectedEvent, expectedData, expectedId: string) =
  check evt.event == expectedEvent
  check evt.data == expectedData
  check evt.id == expectedId

suite "SSE parseSse — 完整文本解析":
  test "single event with data":
    let evts = parseEvents("data: hello\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "multiple events":
    let evts = parseEvents("data: a\n\ndata: b\n\n")
    check evts.len == 2
    checkEvent(evts[0], "message", "a", "")
    checkEvent(evts[1], "message", "b", "")

  test "event type field":
    let evts = parseEvents("event: ping\ndata: pong\n\n")
    check evts.len == 1
    checkEvent(evts[0], "ping", "pong", "")

  test "id field":
    let evts = parseEvents("id: 42\ndata: x\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "x", "42")

  test "retry field":
    let evts = parseEvents("retry: 10000\ndata: x\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "x", "")

  test "multiline data":
    let evts = parseEvents("data: line1\ndata: line2\n\n")
    check evts.len == 1
    check evts[0].data == "line1\nline2"

  test "empty data line":
    let evts = parseEvents("data: line1\ndata: \ndata: line3\n\n")
    check evts.len == 1
    check evts[0].data == "line1\n\nline3"

  test "comment lines ignored":
    let evts = parseEvents(":comment\ndata: x\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "x", "")

  test "BOM at start":
    let evts = parseEvents("\xEF\xBB\xBFdata: x\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "x", "")

  test "no data field — no event dispatched":
    let evts = parseEvents("event: ping\n\n")
    check evts.len == 0

  test "id with null char ignored":
    var p = newSseParser()
    let evts = p.feed("id: abc\0def\ndata: x\n\n")
    let more = p.flush()
    check (evts & more).len == 1
    check (evts & more)[0].id == ""

  test "invalid retry ignored":
    var p = newSseParser()
    discard p.feed("retry: abc\ndata: x\n\n")
    discard p.flush()
    check p.reconnectionTime() == 0

  test "unknown field ignored":
    let evts = parseEvents("foo: bar\ndata: x\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "x", "")

  test "CRLF line endings":
    let evts = parseEvents("data: hello\r\n\r\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "CR line endings":
    let evts = parseEvents("data: hello\r\r")
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "retry with trailing space":
    var p = newSseParser()
    discard p.feed("retry: 5000 \ndata: x\n\n")
    discard p.flush()
    check p.reconnectionTime() == 5000

  test "event with colon":
    let evts = parseEvents("event: foo:bar\ndata: x\n\n")
    check evts.len == 1
    checkEvent(evts[0], "foo:bar", "x", "")

  test "data with leading spaces":
    let evts = parseEvents("data:   hello\n\n")
    check evts.len == 1
    check evts[0].data == "  hello"

suite "SseParser 流式解析":
  test "full event in one chunk":
    var p = newSseParser()
    let evts = p.feed("data: hello\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "event split across two chunks":
    var p = newSseParser()
    var evts = p.feed("data: hel")
    check evts.len == 0
    evts = p.feed("lo\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "event split at newline":
    var p = newSseParser()
    var evts = p.feed("data: hello\n")
    check evts.len == 0
    evts = p.feed("\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "empty chunk":
    var p = newSseParser()
    let evts = p.feed("")
    check evts.len == 0

  test "multiple chunks with leftover":
    var p = newSseParser()
    var evts = p.feed("data: a\n\n")
    check evts.len == 1
    evts = p.feed("data: b\n\n")
    check evts.len == 1
    checkEvent(evts[0], "message", "b", "")
    evts = p.feed("data: c\n")
    check evts.len == 0
    evts = p.flush()
    check evts.len == 1
    checkEvent(evts[0], "message", "c", "")

  test "flush emits residual event":
    var p = newSseParser()
    var evts = p.feed("data: hello\n")
    check evts.len == 0
    evts = p.flush()
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "flush without data lines":
    var p = newSseParser()
    var evts = p.feed("event: ping\n")
    check evts.len == 0
    evts = p.flush()
    check evts.len == 0

  test "reset clears state":
    var p = newSseParser()
    discard p.feed("data: hello\n\n")
    p.reset()
    let evts = p.feed("data: hello\n\n")
    let more = p.flush()
    check (evts & more).len == 1
    checkEvent(evts[0], "message", "hello", "")

  test "lastEventId persists across events":
    var p = newSseParser()
    discard p.feed("id: 5\ndata: a\n\n")
    discard p.flush()
    check p.lastEventId() == "5"
    discard p.feed("data: b\n\n")
    discard p.flush()
    check p.lastEventId() == "5"

  test "reconnectionTime reflects retry":
    var p = newSseParser()
    discard p.feed("retry: 5000\ndata: x\n\n")
    discard p.flush()
    check p.reconnectionTime() == 5000

suite "SSE HTTP 集成":
  test "content-type startsWith text/event-stream detection":
    let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: hello\n\n"
    let resp = parseHttpResponse(raw)
    check resp.statusCode == 200
    check resp.headers.getOrDefault("content-type") == "text/event-stream"

  test "content-type with charset still matches":
    let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.headers.getOrDefault("content-type").startsWith("text/event-stream")

  test "normal application/json response unaffected":
    let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}"
    let resp = parseHttpResponse(raw)
    check not resp.headers.getOrDefault("content-type").startsWith("text/event-stream")
    check resp.body == "{\"ok\":true}"

  test "chunked SSE condition detection":
    let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n"
    let resp = parseHttpResponse(raw)
    let ct = resp.headers.getOrDefault("content-type")
    let te = resp.headers.getOrDefault("transfer-encoding")
    check ct.startsWith("text/event-stream")
    check te.find("chunked") >= 0

  test "parse raw SSE HTTP response body with SSE parser":
    let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: hello\n\n"
    let resp = parseHttpResponse(raw)
    check resp.statusCode == 200
    check resp.headers.getOrDefault("content-type") == "text/event-stream"
    var parser = newSseParser()
    var evts = parser.feed(resp.body)
    for e in parser.flush():
      evts.add(e)
    check evts.len == 1
    checkEvent(evts[0], "message", "hello", "")
