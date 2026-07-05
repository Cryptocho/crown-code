import std/[json, strutils, tables]
import api/types
import mcp/sse
import mcp/transport_http

proc newApiClient*(config: ApiClientConfig): ApiClient =
  let endpointUrl = if config.baseUrl.endsWith("/chat/completions"): config.baseUrl
                    else: config.baseUrl & "/chat/completions"
  let transport = newHttpTransport(endpointUrl, config.apiKey)
  ApiClient(config: config, http: transport)

proc messageToJson(msg: Message): JsonNode =
  result = %*{"role": $msg.role, "content": msg.content}
  if msg.role == mrAssistant and msg.toolCalls.len > 0:
    result["content"] = newJNull()
    var tcArr = newJArray()
    for tc in msg.toolCalls:
      tcArr.add(%*{
        "id": tc.id,
        "type": "function",
        "function": {"name": tc.functionName, "arguments": tc.arguments}
      })
    result["tool_calls"] = tcArr
  if msg.role == mrTool:
    result["tool_call_id"] = %msg.toolCallId
    if msg.name.len > 0:
      result["name"] = %msg.name

proc buildChatRequest*(client: ApiClient, messages: seq[Message],
                       tools: seq[Tool] = @[]): JsonNode =
  var msgArr = newJArray()
  for m in messages:
    msgArr.add(messageToJson(m))
  result = %*{
    "model": client.config.model,
    "messages": msgArr,
    "stream": false
  }
  if client.config.temperature > 0.0:
    result["temperature"] = %client.config.temperature
  if client.config.maxTokens > 0:
    result["max_tokens"] = %client.config.maxTokens
  if tools.len > 0:
    var toolArr = newJArray()
    for t in tools:
      var toolObj = %*{
        "type": "function",
        "function": {
          "name": t.name,
          "description": t.description,
          "parameters": t.parameters
        }
      }
      toolArr.add(toolObj)
    result["tools"] = toolArr

proc parseChatResponse*(body: string): ApiResponse =
  var root: JsonNode
  try:
    root = parseJson(body)
  except JsonParsingError:
    return ApiResponse(error: ApiError(code: 0, message: "invalid JSON response"))
  if root.hasKey("error"):
    let err = root["error"]
    let code = if err.hasKey("code"): err["code"].getInt(0) else: 0
    let msg = if err.hasKey("message"): err["message"].getStr("") else: ""
    return ApiResponse(error: ApiError(code: code.int, message: msg))
  if root.hasKey("usage"):
    let u = root["usage"]
    var usage = ApiUsage()
    if u.hasKey("prompt_tokens"):
      usage.inputTokens = u["prompt_tokens"].getInt(0).int
    if u.hasKey("completion_tokens"):
      usage.outputTokens = u["completion_tokens"].getInt(0).int
    if u.hasKey("cache_read_tokens"):
      usage.cacheReadTokens = u["cache_read_tokens"].getInt(0).int
    result.usage = usage
  if not root.hasKey("choices") or root["choices"].len == 0:
    if result.usage.inputTokens > 0 or result.usage.outputTokens > 0:
      return result
    return ApiResponse(error: ApiError(code: 0, message: "no choices in response"), usage: result.usage)
  let choice = root["choices"][0]
  if choice.hasKey("finish_reason") and not choice["finish_reason"].isNil:
    result.finishReason = choice["finish_reason"].getStr("")
  if choice.hasKey("message"):
    let msg = choice["message"]
    if msg.hasKey("content") and msg["content"].kind != JNull:
      result.content = msg["content"].getStr("")
    if msg.hasKey("tool_calls"):
      var tcs: seq[ToolCall] = @[]
      for tc in msg["tool_calls"]:
        let fn = tc["function"]
        let tcId = if tc.hasKey("id"): tc["id"].getStr("") else: ""
        let fnName = if fn.hasKey("name"): fn["name"].getStr("") else: ""
        let fnArgs = if fn.hasKey("arguments"): fn["arguments"].getStr("") else: ""
        tcs.add(ToolCall(id: tcId, functionName: fnName, arguments: fnArgs))
      result.toolCalls = tcs

proc createMessage*(client: ApiClient, messages: seq[Message],
                    tools: seq[Tool] = @[]): ApiResponse =
  close(client.http)
  if not connect(client.http):
    return ApiResponse(error: ApiError(code: 0, message: client.http.lastError))
  let reqBody = buildChatRequest(client, messages, tools)
  let httpResp = postJson(client.http, $reqBody)
  if httpResp.statusCode == 0:
    return ApiResponse(error: ApiError(code: 0, message: httpResp.error))
  if httpResp.statusCode != 200:
    return ApiResponse(error: ApiError(code: httpResp.statusCode, message: httpResp.body))
  result = parseChatResponse(httpResp.body)

proc parseStreamEvent*(data: string): seq[ApiStreamChunk] =
  if data == "[DONE]":
    return @[ApiStreamChunk(kind: asckDone)]
  var root: JsonNode
  try:
    root = parseJson(data)
  except JsonParsingError:
    return @[ApiStreamChunk(kind: asckText, text: "")]
  if root.hasKey("error"):
    return @[ApiStreamChunk(kind: asckText, text: "")]
  if root.hasKey("usage"):
    let u = root["usage"]
    let input = if u.hasKey("prompt_tokens"): u["prompt_tokens"].getInt(0).int else: 0
    let output = if u.hasKey("completion_tokens"): u["completion_tokens"].getInt(0).int else: 0
    return @[ApiStreamChunk(kind: asckUsage, inputTokens: input, outputTokens: output)]
  if root.hasKey("choices") and root["choices"].len > 0:
    let choice = root["choices"][0]
    if not choice.hasKey("delta"):
      return @[ApiStreamChunk(kind: asckText, text: "")]
    let delta = choice["delta"]
    if delta.hasKey("reasoning_content") and delta["reasoning_content"].kind != JNull:
      return @[ApiStreamChunk(kind: asckReasoning, reasoning: delta["reasoning_content"].getStr(""))]
    if delta.hasKey("tool_calls") and delta["tool_calls"].len > 0:
      result = @[]
      for tcData in delta["tool_calls"]:
        var tc = ToolCall()
        if tcData.hasKey("index"):
          tc.tcIndex = tcData["index"].getInt(0).int
        if tcData.hasKey("id"):
          tc.id = tcData["id"].getStr("")
        if tcData.hasKey("function"):
          let fn = tcData["function"]
          if fn.hasKey("name"):
            tc.functionName = fn["name"].getStr("")
          if fn.hasKey("arguments"):
            tc.arguments = fn["arguments"].getStr("")
        result.add(ApiStreamChunk(kind: asckToolCall, toolCall: tc))
      return
    if delta.hasKey("content") and delta["content"].kind != JNull:
      return @[ApiStreamChunk(kind: asckText, text: delta["content"].getStr(""))]
  @[ApiStreamChunk(kind: asckText, text: "")]

proc createMessageStream*(client: ApiClient, messages: seq[Message],
                          tools: seq[Tool] = @[],
                          onChunk: proc(chunk: ApiStreamChunk): bool {.closure.}): ApiResponse =
  close(client.http)
  if not connect(client.http):
    return ApiResponse(error: ApiError(code: 0, message: client.http.lastError))
  var reqBody = buildChatRequest(client, messages, tools)
  reqBody["stream"] = %true
  if not client.config.streamOptions.isNil:
    reqBody["stream_options"] = client.config.streamOptions
  else:
    reqBody["stream_options"] = %*{"include_usage": true}
  var toolCallState = initTable[int, ToolCall]()
  var accumulatedContent = ""
  var usage = ApiUsage()
  proc onEvent(event: SseEvent): bool {.closure.} =
    let chunks = parseStreamEvent(event.data)
    for chunk in chunks:
      case chunk.kind
      of asckText:
        accumulatedContent.add(chunk.text)
      of asckReasoning:
        discard
      of asckUsage:
        if chunk.inputTokens > 0: usage.inputTokens = chunk.inputTokens
        if chunk.outputTokens > 0: usage.outputTokens = chunk.outputTokens
      of asckToolCall:
        let tcIndex = chunk.toolCall.tcIndex
        if not toolCallState.hasKey(tcIndex):
          toolCallState[tcIndex] = ToolCall()
        var state = toolCallState[tcIndex]
        if chunk.toolCall.id.len > 0: state.id = chunk.toolCall.id
        if chunk.toolCall.functionName.len > 0: state.functionName = chunk.toolCall.functionName
        if chunk.toolCall.arguments.len > 0: state.arguments.add(chunk.toolCall.arguments)
        toolCallState[tcIndex] = state
        var emitChunk = chunk
        emitChunk.toolCall = state
      of asckDone:
        discard onChunk(chunk)
        return false
      if not onChunk(chunk):
        return false
    true
  let (statusCode, err) = postJsonStream(client.http, $reqBody, onEvent)
  if statusCode != 200:
    return ApiResponse(error: ApiError(code: statusCode, message: err))
  result.content = accumulatedContent
  result.usage = usage
  if toolCallState.len > 0:
    var tcs: seq[ToolCall] = @[]
    var maxIdx = -1
    for idx in keys(toolCallState):
      if idx > maxIdx: maxIdx = idx
    for idx in 0 .. maxIdx:
      if toolCallState.hasKey(idx):
        tcs.add(toolCallState[idx])
    result.toolCalls = tcs