import unittest
import std/json
import mcp/jsonrpc

suite "buildRequest":
  test "with params":
    let req = buildRequest("tools/call", %*{"name": "echo"}, 1)
    let node = parseJson(req)
    check node["jsonrpc"].getStr == "2.0"
    check node["method"].getStr == "tools/call"
    check node["params"]["name"].getStr == "echo"
    check node["id"].getInt == 1

  test "with null params (omits params field)":
    let req = buildRequest("ping", newJNull(), 42)
    let node = parseJson(req)
    check node["jsonrpc"].getStr == "2.0"
    check node["method"].getStr == "ping"
    check node["id"].getInt == 42
    check "params" notin node

  test "with empty object params (preserves params field)":
    let req = buildRequest("tools/call", %*{}, 2)
    let node = parseJson(req)
    check "params" in node
    check node["params"].kind == JObject

  test "special chars in method":
    let req = buildRequest("tool\"test\\", newJNull(), 1)
    let node = parseJson(req)
    check node["method"].getStr == "tool\"test\\"
    check node["id"].getInt == 1

suite "buildNotification":
  test "with params":
    let notif = buildNotification("notifications/initialized", %*{})
    let node = parseJson(notif)
    check node["jsonrpc"].getStr == "2.0"
    check node["method"].getStr == "notifications/initialized"
    check "params" in node
    check "id" notin node

  test "with null params (omits params and id)":
    let notif = buildNotification("ping", newJNull())
    let node = parseJson(notif)
    check node["jsonrpc"].getStr == "2.0"
    check node["method"].getStr == "ping"
    check "params" notin node
    check "id" notin node

  test "with empty array params":
    let notif = buildNotification("test", %*[])
    let node = parseJson(notif)
    check node["params"].kind == JArray
    check node["params"].len == 0

suite "parseResponse":
  test "valid response with result":
    let resp = parseResponse("""{"jsonrpc":"2.0","id":1,"result":{}}""")
    check resp["jsonrpc"].getStr == "2.0"
    check resp["id"].getInt == 1
    check resp["result"].kind == JObject

  test "valid response with error":
    let resp = parseResponse("""{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method not found"}}""")
    check resp["error"]["code"].getInt == -32601
    check resp["error"]["message"].getStr == "method not found"

  test "valid response with both result and error not checked":
    let resp = parseResponse("""{"jsonrpc":"2.0","id":1,"result":{"tools":[]},"error":{"code":0}}""")
    check resp["result"]["tools"].kind == JArray

  test "empty JSON object":
    let resp = parseResponse("{}")
    check resp.kind == JObject

  test "JSON array":
    let resp = parseResponse("[1,2,3]")
    check resp.kind == JArray
    check resp[0].getInt == 1

  test "empty string raises":
    expect(JsonParsingError):
      discard parseResponse("")

  test "invalid JSON raises":
    expect(JsonParsingError):
      discard parseResponse("not json")