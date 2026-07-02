import std/json
import mcp/transport_http

type
  MessageRole* = enum
    mrSystem = "system"
    mrUser = "user"
    mrAssistant = "assistant"
    mrTool = "tool"
    mrDeveloper = "developer"

  Message* = object
    role*: MessageRole
    content*: string
    toolCalls*: seq[ToolCall]
    toolCallId*: string
    name*: string

  Tool* = object
    name*: string
    description*: string
    parameters*: JsonNode

  ToolCall* = object
    id*: string
    functionName*: string
    arguments*: string
    tcIndex*: int

  ApiStreamChunkKind* = enum
    asckText
    asckReasoning
    asckUsage
    asckToolCall
    asckDone

  ApiStreamChunk* = object
    case kind*: ApiStreamChunkKind
    of asckText: text*: string
    of asckReasoning: reasoning*: string
    of asckUsage: inputTokens*, outputTokens*: int
    of asckToolCall: toolCall*: ToolCall
    of asckDone: discard

  ApiError* = object
    code*: int
    message*: string

  ApiUsage* = object
    inputTokens*: int
    outputTokens*: int
    cacheReadTokens*: int

  ApiResponse* = object
    content*: string
    toolCalls*: seq[ToolCall]
    usage*: ApiUsage
    error*: ApiError
    finishReason*: string

  ApiClientConfig* = object
    baseUrl*: string
    apiKey*: string
    model*: string
    temperature*: float
    maxTokens*: int
    streamOptions*: JsonNode

  ApiClient* = ref object
    config*: ApiClientConfig
    http*: HttpTransport