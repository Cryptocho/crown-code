import unittest
import std/[json, os]
import api/types
import api/openai

suite "buildChatRequest JSON structure":
  let client = newApiClient(ApiClientConfig(
    baseUrl: "https://openrouter.ai/api/v1/chat/completions",
    apiKey: "", model: "test-model"
  ))

  test "basic request structure":
    let req = buildChatRequest(client, @[Message(role: mrUser, content: "hi")])
    check req["model"].getStr == "test-model"
    check not req["stream"].getBool
    check req["messages"].len == 1
    check req["messages"][0]["role"].getStr == "user"
    check req["messages"][0]["content"].getStr == "hi"

  test "multiple messages":
    let req = buildChatRequest(client, @[
      Message(role: mrSystem, content: "you are a bot"),
      Message(role: mrUser, content: "hello")
    ])
    check req["messages"].len == 2
    check req["messages"][0]["role"].getStr == "system"
    check req["messages"][1]["role"].getStr == "user"

  test "temperature and maxTokens when set":
    let cfg = ApiClientConfig(
      baseUrl: "https://openrouter.ai/api/v1/chat/completions",
      apiKey: "", model: "m", temperature: 0.7, maxTokens: 2048
    )
    let c = newApiClient(cfg)
    let req = buildChatRequest(c, @[Message(role: mrUser, content: "hi")])
    check req["temperature"].getFloat == 0.7
    check req["max_tokens"].getInt == 2048

  test "no temperature when zero":
    let req = buildChatRequest(client, @[Message(role: mrUser, content: "hi")])
    check not req.hasKey("temperature")

suite "buildChatRequest with tools":
  let client = newApiClient(ApiClientConfig(
    baseUrl: "https://openrouter.ai/api/v1/chat/completions",
    apiKey: "", model: "m"
  ))

  test "tools included when non-empty":
    let params = %*{"type": "object"}
    let tools = @[Tool(name: "fn1", description: "desc", parameters: params)]
    let req = buildChatRequest(client, @[Message(role: mrUser, content: "hi")], tools)
    check req.hasKey("tools")
    check req["tools"].len == 1
    check req["tools"][0]["type"].getStr == "function"
    check req["tools"][0]["function"]["name"].getStr == "fn1"
    check req["tools"][0]["function"]["description"].getStr == "desc"

  test "no tools field when empty":
    let req = buildChatRequest(client, @[Message(role: mrUser, content: "hi")])
    check not req.hasKey("tools")

suite "buildChatRequest message conversion":
  let client = newApiClient(ApiClientConfig(
    baseUrl: "https://openrouter.ai/api/v1/chat/completions",
    apiKey: "", model: "m"
  ))

  test "assistant with tool calls has null content":
    let tc = ToolCall(id: "call_1", functionName: "read_file", arguments: "{}")
    let msgs = @[Message(role: mrAssistant, content: "", toolCalls: @[tc])]
    let req = buildChatRequest(client, msgs)
    let msg = req["messages"][0]
    check msg["role"].getStr == "assistant"
    check msg["content"].kind == JNull
    check msg["tool_calls"].len == 1
    check msg["tool_calls"][0]["id"].getStr == "call_1"
    check msg["tool_calls"][0]["type"].getStr == "function"
    check msg["tool_calls"][0]["function"]["name"].getStr == "read_file"
    check msg["tool_calls"][0]["function"]["arguments"].getStr == "{}"

  test "tool message includes tool_call_id":
    let msgs = @[Message(role: mrTool, content: "result", toolCallId: "call_1", name: "read_file")]
    let req = buildChatRequest(client, msgs)
    let msg = req["messages"][0]
    check msg["role"].getStr == "tool"
    check msg["content"].getStr == "result"
    check msg["tool_call_id"].getStr == "call_1"
    check msg["name"].getStr == "read_file"

suite "parseChatResponse":
  test "normal response":
    let body = """{"id":"chat-1","choices":[{"index":0,"message":{"role":"assistant","content":"Hello!"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"""
    let resp = parseChatResponse(body)
    check resp.content == "Hello!"
    check resp.finishReason == "stop"
    check resp.usage.inputTokens == 10
    check resp.usage.outputTokens == 5
    check resp.error.code == 0

  test "response with tool calls":
    let body = """{"id":"chat-2","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"test.txt\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":10}}"""
    let resp = parseChatResponse(body)
    check resp.content.len == 0
    check resp.finishReason == "tool_calls"
    check resp.toolCalls.len == 1
    check resp.toolCalls[0].id == "call_1"
    check resp.toolCalls[0].functionName == "read_file"
    check resp.toolCalls[0].arguments == "{\"path\":\"test.txt\"}"
    check resp.usage.inputTokens == 20

  test "error response 401":
    let body = """{"error":{"code":401,"message":"Invalid API key"}}"""
    let resp = parseChatResponse(body)
    check resp.error.code == 401
    check resp.error.message == "Invalid API key"

  test "error response 400":
    let body = """{"error":{"code":400,"message":"Bad request"}}"""
    let resp = parseChatResponse(body)
    check resp.error.code == 400

  test "error response 429":
    let body = """{"error":{"code":429,"message":"Rate limit exceeded"}}"""
    let resp = parseChatResponse(body)
    check resp.error.code == 429

  test "empty choices":
    let body = """{"id":"chat-3","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":0}}"""
    let resp = parseChatResponse(body)
    check resp.error.code == 0
    check resp.content.len == 0
    check resp.usage.inputTokens == 5

  test "invalid JSON":
    let resp = parseChatResponse("not json")
    check resp.error.code == 0
    check resp.error.message == "invalid JSON response"

suite "newApiClient":
  test "creates client with config":
    let cfg = ApiClientConfig(
      baseUrl: "https://openrouter.ai/api/v1/chat/completions",
      apiKey: "sk-test", model: "llama3"
    )
    let client = newApiClient(cfg)
    check client.config.baseUrl == "https://openrouter.ai/api/v1/chat/completions"
    check client.config.apiKey == "sk-test"
    check client.config.model == "llama3"

suite "real API: non-streaming":
  test "text response":
    let apiKey = getEnv("OPENROUTER_API_KEY", "")
    if apiKey.len == 0:
      skip()
    else:
      let client = newApiClient(ApiClientConfig(
        baseUrl: "https://openrouter.ai/api/v1/chat/completions",
        apiKey: apiKey, model: "meta-llama/llama-3.1-8b-instruct",
        maxTokens: 20
      ))
      let resp = createMessage(client, @[Message(role: mrUser, content: "Say hello in one word")])
      check resp.error.code == 0
      check resp.content.len > 0

  test "tool call":
    let apiKey = getEnv("OPENROUTER_API_KEY", "")
    if apiKey.len == 0:
      skip()
    else:
      let client = newApiClient(ApiClientConfig(
        baseUrl: "https://openrouter.ai/api/v1/chat/completions",
        apiKey: apiKey, model: "meta-llama/llama-3.1-8b-instruct",
        maxTokens: 20
      ))
      let tools = @[Tool(
        name: "read_file", description: "Read a file",
        parameters: %*{"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}
      )]
      let resp = createMessage(client, @[Message(role: mrUser, content: "Read file test.txt")], tools)
      check resp.error.code == 0
      check resp.toolCalls.len > 0
      check resp.toolCalls[0].functionName == "read_file"