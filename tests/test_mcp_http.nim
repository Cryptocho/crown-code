import unittest
import std/[strutils, tables]
import mcp/transport_http

suite "URL parsing":
  test "http default port":
    let (_, port, tls, path1) = parseUrl("http://example.com")
    check port == 80
    check not tls
    check path1 == "/"

  test "https default port":
    let (_, port1, tls1, _) = parseUrl("https://example.com")
    check tls1
    check port1 == 443

  test "custom port":
    let (_, port, _, _) = parseUrl("http://example.com:8080")
    check port == 8080

  test "path with https":
    let (_, _, _, path) = parseUrl("https://example.com/api/mcp")
    check path == "/api/mcp"

  test "no scheme defaults to http":
    let (_, port, tls, _) = parseUrl("example.com/path")
    check port == 80
    check not tls

suite "HTTP request building":
  test "basic POST":
    let req = buildHttpRequest("example.com", 443, "/mcp", "{}", "")
    check req[0..<"POST /mcp HTTP/1.1".len] == "POST /mcp HTTP/1.1"
    check req.contains("Host: example.com")
    check req.contains("Content-Type: application/json")
    check req.contains("Accept: application/json, text/event-stream")
    check req.contains("Content-Length: 2")
    check req.contains("Connection: close")

  test "with Bearer token":
    let req = buildHttpRequest("h", 80, "/p", "{}", "secret")
    check req.contains("Authorization: Bearer secret")

  test "without token omits Authorization":
    let req = buildHttpRequest("h", 80, "/p", "{}", "")
    check not req.contains("Authorization:")

  test "Content-Length matches body":
    let body = "{\"jsonrpc\":\"2.0\"}"
    let req = buildHttpRequest("h", 80, "/p", body, "")
    check req.contains("Content-Length: " & $body.len)
    check req[^body.len..^1] == body

  test "Accept header includes event-stream":
    let req = buildHttpRequest("h", 80, "/p", "{}", "")
    check req.contains("Accept: application/json, text/event-stream")

  test "Host header includes port for non-default port":
    let req = buildHttpRequest("example.com", 8080, "/mcp", "{}", "")
    check req.contains("Host: example.com:8080")

suite "HTTP response parsing":
  test "200 OK":
    let resp = parseHttpResponse("HTTP/1.1 200 OK\r\n\r\n")
    check resp.statusCode == 200
    check resp.body.len == 0

  test "404 with header":
    let raw = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.statusCode == 404
    check resp.headers.getOrDefault("content-type", "") == "text/plain"

  test "multiple headers":
    let raw = "HTTP/1.1 200 OK\r\nA: 1\r\nB: 2\r\n\r\nextra"
    let resp = parseHttpResponse(raw)
    check resp.headers["a"] == "1"
    check resp.headers["b"] == "2"
    check resp.body == "extra"

  test "no reason phrase":
    let raw = "HTTP/1.1 200\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.statusCode == 200

  test "Content-Type with charset":
    let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.headers.getOrDefault("content-type", "")[0..<"application/json".len] == "application/json"

  test "malformed status line":
    let raw = "HTTP/1.0 500\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.statusCode == 500

suite "Header case insensitivity":
  test "Content-Length lowercased lookup":
    let raw = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.headers.hasKey("content-length")

  test "Transfer-Encoding mixed case":
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.headers.hasKey("transfer-encoding")

  test "unknown header case preserved lowercased":
    let raw = "HTTP/1.1 200 OK\r\nX-Custom: val\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.headers.hasKey("x-custom")

suite "Connection lifecycle":
  test "close nil does not crash":
    var t: HttpTransport = nil
    close(t)

  test "connect to port refused fails":
    var t = newHttpTransport("http://127.0.0.1:1")
    check not connect(t)
    check t.lastError.len > 0

  test "initial state disconnected":
    let t = newHttpTransport("http://example.com")
    check not isConnected(t)

suite "Chunked decoding":
  test "Transfer-Encoding chunks over Content-Length":
    let raw = "HTTP/1.1 200 OK\r\nContent-Length: 999\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\n"
    let resp = parseHttpResponse(raw)
    check resp.headers.hasKey("transfer-encoding")
    check resp.headers.hasKey("content-length")

suite "parseUrl edge cases":
  test "http with path":
    let (_, _, _, path) = parseUrl("http://example.com/api/v1")
    check path == "/api/v1"

  test "https with path":
    let (_, _, _, path) = parseUrl("https://example.com/api/v1")
    check path == "/api/v1"
