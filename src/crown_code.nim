import api/types
import agent/loop

proc runApp*() =
  let config = ApiClientConfig(
    baseUrl: "http://localhost:11434/v1",
    apiKey: "",
    model: "gemma4:e4b",
    temperature: 0.0,
    maxTokens: 4096
  )
  runAgentLoop(config)

when isMainModule:
  runApp()