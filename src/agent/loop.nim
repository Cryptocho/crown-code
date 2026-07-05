import std/[json, os]
import api/types
import api/openai
import agent/tools
import agent/prompt

proc log(msg: string) =
  stderr.write(msg)
  stderr.flushFile()

proc runAgentLoop*(config: ApiClientConfig) =
  let client = newApiClient(config)
  let tools = getToolDefinitions()
  let systemPrompt = buildSystemPrompt(getCurrentDir())
  var history: seq[Message] = @[]

  history.add(Message(role: mrSystem, content: systemPrompt))
  log("[PROMPT] System prompt (" & $systemPrompt.len & " chars):\n" & systemPrompt & "\n---\n")

  stdout.write("crown-code — A vibe coding tool\n")
  stdout.write("Type your task. /quit to exit.\n")

  while true:
    stdout.write("\nYou: ")
    stdout.flushFile()
    let userInput = readLine(stdin)
    if userInput.len == 0: continue
    if userInput == "/quit" or userInput == "/exit": break

    history.add(Message(role: mrUser, content: userInput))

    while true:
      var assistantText = ""
      var toolCalls: seq[ToolCall] = @[]

      stdout.write("\nAssistant: ")
      stdout.flushFile()

      let resp = client.createMessageStream(history, tools,
        proc(chunk: ApiStreamChunk): bool {.closure.} =
          case chunk.kind
          of asckText:
            stdout.write(chunk.text)
            stdout.flushFile()
            assistantText.add(chunk.text)
          of asckReasoning, asckUsage, asckDone:
            discard
          of asckToolCall:
            discard
          return true
      )

      if resp.error.message.len > 0:
        stdout.write("\n[API Error] ", resp.error.message, "\n")
        stdout.flushFile()
        break

      toolCalls = resp.toolCalls

      history.add(Message(
        role: mrAssistant,
        content: assistantText,
        toolCalls: toolCalls
      ))

      if toolCalls.len == 0:
        stdout.write("\n")
        break

      if toolCalls.len > 0:
        log("[TOOL_CALL] Model requested " & $toolCalls.len & " tool call(s):\n")
        for tc in toolCalls:
          log("  - " & tc.functionName & "(" & tc.arguments & ")\n")
        log("---\n")

      var hasCompletion = false
      for tc in toolCalls:
        var args: JsonNode
        try:
          args = parseJson(tc.arguments)
        except JsonParsingError:
          let errorMsg = "Error: tool call arguments are invalid JSON (truncated response?): " & tc.arguments
          log("[TOOL_RESULT] " & errorMsg & "\n---\n")
          history.add(Message(
            role: mrTool,
            content: errorMsg,
            toolCallId: tc.id,
            name: tc.functionName
          ))
          continue

        let result = executeTool(tc.functionName, args)

        log("[TOOL_RESULT] " & tc.functionName & ":\n" & result & "\n---\n")

        stdout.write("\n  [" & tc.functionName & "]\n")
        stdout.flushFile()

        if tc.functionName == "attempt_completion":
          hasCompletion = true

        history.add(Message(
          role: mrTool,
          content: result,
          toolCallId: tc.id,
          name: tc.functionName
        ))

      if hasCompletion:
        stdout.write("\n--- Task finished. Enter new task or /quit ---\n")
        stdout.flushFile()
        break