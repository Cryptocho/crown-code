import std/json

func buildRequest*(meth: string, params: JsonNode, id: int64): string =
  ## 构建 JSON-RPC 2.0 请求字符串。
  ## params 为 newJNull() 时省略 params 字段。
  let obj = if params.isNil or params.kind == JNull:
    %*{"jsonrpc": "2.0", "method": meth, "id": id}
  else:
    %*{"jsonrpc": "2.0", "method": meth, "params": params, "id": id}
  result = $obj

func buildNotification*(meth: string, params: JsonNode): string =
  ## 构建 JSON-RPC 2.0 通知字符串（无 id 字段）。
  ## params 为 newJNull() 时省略 params 字段。
  let obj = if params.isNil or params.kind == JNull:
    %*{"jsonrpc": "2.0", "method": meth}
  else:
    %*{"jsonrpc": "2.0", "method": meth, "params": params}
  result = $obj

proc parseResponse*(jsonStr: string): JsonNode =
  ## 解析 JSON-RPC 响应字符串为 JsonNode。
  ## 仅做反序列化，不校验 jsonrpc 版本或 id 匹配。
  ## 空字符串或非法 JSON 会抛出 JsonParsingError。
  result = parseJson(jsonStr)
