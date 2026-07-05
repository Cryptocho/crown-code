import std/json
import std/net
import std/os
import std/unittest
import api/types
import api/openai
import agent/tools
import agent/prompt

proc isOllamaRunning(): bool =
  try:
    let socket = newSocket()
    socket.connect("localhost", Port(11434), timeout = 1000)
    socket.close()
    return true
  except:
    return false

suite "agent loop - integration (requires ollama)":
  setup:
    if not isOllamaRunning():
      skip()

  test "simple text conversation via API":
    let config = ApiClientConfig(
      baseUrl: "http://localhost:11434/v1",
      apiKey: "",
      model: "gemma4:e4b",
      temperature: 0.0,
      maxTokens: 256
    )
    let client = newApiClient(config)
    let messages = @[
      Message(role: mrSystem, content: "You are a helpful assistant."),
      Message(role: mrUser, content: "Say hello in one word.")
    ]
    let resp = client.createMessage(messages)
    check resp.error.message.len == 0
    check resp.content.len > 0

  test "tool calling with list_files":
    let config = ApiClientConfig(
      baseUrl: "http://localhost:11434/v1",
      apiKey: "",
      model: "gemma4:e4b",
      temperature: 0.0,
      maxTokens: 512
    )
    let client = newApiClient(config)
    let tools = getToolDefinitions()
    let systemPrompt = buildSystemPrompt(getCurrentDir())
    let messages = @[
      Message(role: mrSystem, content: systemPrompt),
      Message(role: mrUser, content: "List files in the current directory using the list_files tool.")
    ]
    let resp = client.createMessage(messages, tools)
    check resp.error.message.len == 0
    if resp.toolCalls.len > 0:
      check resp.toolCalls[0].functionName == "list_files"
      check resp.toolCalls[0].arguments.len > 0

  test "streaming text response":
    let config = ApiClientConfig(
      baseUrl: "http://localhost:11434/v1",
      apiKey: "",
      model: "gemma4:e4b",
      temperature: 0.0,
      maxTokens: 256,
      streamOptions: %*{"include_usage": true}
    )
    let client = newApiClient(config)
    let messages = @[
      Message(role: mrSystem, content: "You are a helpful assistant."),
      Message(role: mrUser, content: "Say hello in one word.")
    ]
    var accumulated = ""
    let resp = client.createMessageStream(messages, @[],
      proc(chunk: ApiStreamChunk): bool {.closure.} =
        if chunk.kind == asckText and chunk.text.len > 0:
          accumulated.add(chunk.text)
        return true
    )
    check resp.error.message.len == 0
    check resp.content.len > 0
    check accumulated.len > 0