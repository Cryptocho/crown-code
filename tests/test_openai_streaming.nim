import unittest
import std/[json, os, tables]
import api/types
import api/openai
import mcp/sse

proc sseChunk(data: string): string =
  "data: " & data & "\n\n"

suite "parseStreamEvent":
  test "plain text delta":
    let data = """{"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckText
    check chunks[0].text == "Hello"

  test "text delta with null content":
    let data = """{"id":"1","choices":[{"index":0,"delta":{"content":null},"finish_reason":null}]}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckText
    check chunks[0].text == ""

  test "tool call delta with id and name":
    let data = """{"id":"gen-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckToolCall
    check chunks[0].toolCall.id == "call_1"
    check chunks[0].toolCall.functionName == "read_file"

  test "tool call delta with arguments only":
    let data = """{"id":"gen-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\": \"test.txt\"}"}}]},"finish_reason":null}]}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckToolCall
    check chunks[0].toolCall.tcIndex == 0
    check chunks[0].toolCall.arguments == "{\"path\": \"test.txt\"}"

  test "reasoning content delta":
    let data = """{"id":"1","choices":[{"index":0,"delta":{"reasoning_content":"thinking step"},"finish_reason":null}]}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckReasoning
    check chunks[0].reasoning == "thinking step"

  test "usage chunk":
    let data = """{"usage":{"prompt_tokens":10,"completion_tokens":5}}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckUsage
    check chunks[0].inputTokens == 10
    check chunks[0].outputTokens == 5

  test "usage chunk partial":
    let data = """{"usage":{"prompt_tokens":10}}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckUsage
    check chunks[0].inputTokens == 10
    check chunks[0].outputTokens == 0

  test "done marker":
    let chunks = parseStreamEvent("[DONE]")
    check chunks.len == 1
    check chunks[0].kind == asckDone

  test "error response":
    let data = """{"error":{"code":401,"message":"Invalid API key"}}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckText

  test "empty delta with only finish_reason":
    let data = """{"id":"1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 1
    check chunks[0].kind == asckText
    check chunks[0].text == ""

  test "multiple tool calls in one delta":
    let data = """{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"fn1","arguments":""}},{"index":1,"id":"call_2","function":{"name":"fn2","arguments":""}}]},"finish_reason":null}]}"""
    let chunks = parseStreamEvent(data)
    check chunks.len == 2
    check chunks[0].kind == asckToolCall
    check chunks[1].kind == asckToolCall
    check chunks[0].toolCall.functionName == "fn1"
    check chunks[1].toolCall.functionName == "fn2"

suite "tool call delta accumulation via SSE parser":
  test "single tool call cross-chunk accumulation":
    let sseText = sseChunk("""{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}""") &
                 sseChunk("""{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\": "}}]},"finish_reason":null}]}""") &
                 sseChunk("""{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"test.txt\"}"}}]},"finish_reason":null}]}""")
    var parser = newSseParser()
    var toolCalls = initTable[int, ToolCall]()
    for evt in parser.feed(sseText):
      for chunk in parseStreamEvent(evt.data):
        if chunk.kind == asckToolCall:
          let tcIndex = chunk.toolCall.tcIndex
          if not toolCalls.hasKey(tcIndex):
            toolCalls[tcIndex] = ToolCall()
          var state = toolCalls[tcIndex]
          if chunk.toolCall.id.len > 0: state.id = chunk.toolCall.id
          if chunk.toolCall.functionName.len > 0: state.functionName = chunk.toolCall.functionName
          if chunk.toolCall.arguments.len > 0: state.arguments.add(chunk.toolCall.arguments)
          toolCalls[tcIndex] = state
    check toolCalls.len == 1
    check toolCalls[0].id == "call_1"
    check toolCalls[0].functionName == "read_file"
    check toolCalls[0].arguments == "{\"path\": \"test.txt\"}"

  test "multi tool call parallel accumulation":
    let sseText = sseChunk("""{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"fn1","arguments":""}},{"index":1,"id":"call_2","type":"function","function":{"name":"fn2","arguments":""}}]},"finish_reason":null}]}""") &
                 sseChunk("""{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"a\":1}"}}]},"finish_reason":null}]}""") &
                 sseChunk("""{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"b\":2}"}}]},"finish_reason":null}]}""")
    var parser = newSseParser()
    var toolCalls = initTable[int, ToolCall]()
    for evt in parser.feed(sseText):
      for chunk in parseStreamEvent(evt.data):
        if chunk.kind == asckToolCall:
          let tcIndex = chunk.toolCall.tcIndex
          if not toolCalls.hasKey(tcIndex):
            toolCalls[tcIndex] = ToolCall()
          var state = toolCalls[tcIndex]
          if chunk.toolCall.id.len > 0: state.id = chunk.toolCall.id
          if chunk.toolCall.functionName.len > 0: state.functionName = chunk.toolCall.functionName
          if chunk.toolCall.arguments.len > 0: state.arguments.add(chunk.toolCall.arguments)
          toolCalls[tcIndex] = state
    check toolCalls.len == 2
    check toolCalls[0].id == "call_1"
    check toolCalls[0].functionName == "fn1"
    check toolCalls[0].arguments == "{\"a\":1}"
    check toolCalls[1].id == "call_2"
    check toolCalls[1].functionName == "fn2"
    check toolCalls[1].arguments == "{\"b\":2}"

suite "SSE parser ignores comments":
  test "OPENROUTER PROCESSING comment skipped":
    let sseText = ": OPENROUTER PROCESSING\n\ndata: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n"
    var parser = newSseParser()
    let events = parser.feed(sseText)
    check events.len == 1
    check events[0].data == """{"id":"1","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"""

suite "real API: streaming":
  let apiKey = getEnv("OPENROUTER_API_KEY", "")

  test "streaming text response":
    if apiKey.len == 0:
      skip()
    else:
      let client = newApiClient(ApiClientConfig(
        baseUrl: "https://openrouter.ai/api/v1/chat/completions",
        apiKey: apiKey, model: "meta-llama/llama-3.1-8b-instruct",
        maxTokens: 20
      ))
      var receivedText = ""
      var receivedUsage = false
      var receivedDone = false
      let resp = createMessageStream(client, @[Message(role: mrUser, content: "Say hello in one word")],
        onChunk = proc(chunk: ApiStreamChunk): bool =
          case chunk.kind
          of asckText: receivedText.add(chunk.text)
          of asckUsage: receivedUsage = true
          of asckDone: receivedDone = true
          else: discard
          true
      )
      check resp.error.code == 0
      check receivedText.len > 0
      check receivedDone

  test "streaming tool call":
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
      var receivedDone = false
      let resp = createMessageStream(client, @[Message(role: mrUser, content: "Read file test.txt")], tools,
        onChunk = proc(chunk: ApiStreamChunk): bool =
          if chunk.kind == asckDone: receivedDone = true
          true
      )
      check resp.error.code == 0
      check resp.toolCalls.len > 0
      check resp.toolCalls[0].functionName == "read_file"
      check receivedDone