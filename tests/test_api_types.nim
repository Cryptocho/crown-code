import unittest
import std/json
import api/types

suite "MessageRole enum":
  test "all enum values":
    check $mrSystem == "system"
    check $mrUser == "user"
    check $mrAssistant == "assistant"
    check $mrTool == "tool"
    check $mrDeveloper == "developer"

suite "Message construction":
  test "simple user message":
    let msg = Message(role: mrUser, content: "Hello")
    check msg.role == mrUser
    check msg.content == "Hello"
    check msg.toolCalls.len == 0
    check msg.toolCallId.len == 0

  test "assistant message with tool calls":
    let tc = ToolCall(id: "call_123", functionName: "read_file", arguments: "{}")
    let msg = Message(role: mrAssistant, content: "", toolCalls: @[tc])
    check msg.role == mrAssistant
    check msg.toolCalls.len == 1
    check msg.toolCalls[0].id == "call_123"
    check msg.toolCalls[0].functionName == "read_file"

  test "tool result message":
    let msg = Message(role: mrTool, content: "file contents", toolCallId: "call_123", name: "read_file")
    check msg.role == mrTool
    check msg.toolCallId == "call_123"
    check msg.name == "read_file"

  test "developer message":
    let msg = Message(role: mrDeveloper, content: "reasoning instructions")
    check msg.role == mrDeveloper
    check msg.content == "reasoning instructions"

suite "Tool construction":
  test "basic tool with parameters":
    let params = %*{"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}
    let tool = Tool(name: "read_file", description: "Read a file", parameters: params)
    check tool.name == "read_file"
    check tool.description == "Read a file"
    check tool.parameters["type"].getStr == "object"
    check tool.parameters["required"][0].getStr == "path"

  test "tool without description":
    let params = %*{"type": "object"}
    let tool = Tool(name: "echo", parameters: params)
    check tool.name == "echo"
    check tool.description.len == 0

suite "ToolCall construction":
  test "complete tool call":
    let tc = ToolCall(id: "call_456", functionName: "write_file", arguments: "{\"path\":\"test.txt\"}")
    check tc.id == "call_456"
    check tc.functionName == "write_file"
    check tc.arguments == "{\"path\":\"test.txt\"}"

  test "partial tool call (streaming fragment)":
    let tc = ToolCall(id: "", functionName: "", arguments: "")
    check tc.id.len == 0
    check tc.functionName.len == 0
    check tc.arguments.len == 0

suite "ApiStreamChunk construction":
  test "text chunk":
    let c = ApiStreamChunk(kind: asckText, text: "Hello World")
    check c.kind == asckText
    check c.text == "Hello World"

  test "reasoning chunk":
    let c = ApiStreamChunk(kind: asckReasoning, reasoning: "thinking step")
    check c.kind == asckReasoning
    check c.reasoning == "thinking step"

  test "usage chunk":
    let c = ApiStreamChunk(kind: asckUsage, inputTokens: 10, outputTokens: 5)
    check c.kind == asckUsage
    check c.inputTokens == 10
    check c.outputTokens == 5

  test "tool call chunk":
    let tc = ToolCall(id: "call_1", functionName: "read_file", arguments: "{}")
    let c = ApiStreamChunk(kind: asckToolCall, toolCall: tc)
    check c.kind == asckToolCall
    check c.toolCall.id == "call_1"
    check c.toolCall.functionName == "read_file"

  test "done chunk":
    let c = ApiStreamChunk(kind: asckDone)
    check c.kind == asckDone

suite "ApiResponse defaults":
  test "empty response":
    let resp = ApiResponse()
    check resp.content.len == 0
    check resp.toolCalls.len == 0
    check resp.finishReason.len == 0
    check resp.error.code == 0

  test "response with content and usage":
    let usage = ApiUsage(inputTokens: 10, outputTokens: 5)
    let resp = ApiResponse(content: "Hello", usage: usage, finishReason: "stop")
    check resp.content == "Hello"
    check resp.usage.inputTokens == 10
    check resp.usage.outputTokens == 5
    check resp.finishReason == "stop"

  test "response with error":
    let err = ApiError(code: 401, message: "invalid API key")
    let resp = ApiResponse(error: err)
    check resp.error.code == 401
    check resp.error.message == "invalid API key"

suite "ApiClientConfig construction":
  test "minimal config":
    let cfg = ApiClientConfig(baseUrl: "https://openrouter.ai/api/v1", apiKey: "sk-xxx", model: "model-id")
    check cfg.baseUrl == "https://openrouter.ai/api/v1"
    check cfg.apiKey == "sk-xxx"
    check cfg.model == "model-id"
    check cfg.temperature == 0.0
    check cfg.maxTokens == 0

  test "full config":
    let cfg = ApiClientConfig(
      baseUrl: "http://localhost:11434/v1",
      apiKey: "",
      model: "llama3",
      temperature: 0.7,
      maxTokens: 2048,
      streamOptions: %*{"include_usage": true}
    )
    check cfg.baseUrl == "http://localhost:11434/v1"
    check cfg.model == "llama3"
    check cfg.temperature == 0.7
    check cfg.maxTokens == 2048
    check cfg.streamOptions["include_usage"].getBool == true