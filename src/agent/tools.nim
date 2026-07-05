import std/json
import std/strutils
import api/types
import file_reader
import file_writer
import file_edit
import command_exec
import search_files
import list_files

proc readFileTool(): Tool =
  Tool(
    name: "read_file",
    description: "Read the contents of a file at the specified path. Returns content with line numbers.",
    parameters: %*{
      "type": "object",
      "properties": {
        "path": {"type": "string", "description": "File path relative to workspace root"},
        "start_line": {"type": "integer", "description": "1-based start line (default: 1)"},
        "end_line": {"type": "integer", "description": "1-based end line (default: start_line + 1000)"}
      },
      "required": ["path"]
    }
  )

proc writeToFileTool(): Tool =
  Tool(
    name: "write_to_file",
    description: "Write content to a file at the specified path. Creates the file if it doesn't exist.",
    parameters: %*{
      "type": "object",
      "properties": {
        "path": {"type": "string", "description": "File path relative to workspace root"},
        "content": {"type": "string", "description": "Content to write to the file"}
      },
      "required": ["path", "content"]
    }
  )

proc replaceInFileTool(): Tool =
  Tool(
    name: "replace_in_file",
    description: "Replace exact string matches in a file. Used for editing existing files.",
    parameters: %*{
      "type": "object",
      "properties": {
        "path": {"type": "string", "description": "File path relative to workspace root"},
        "old_string": {"type": "string", "description": "The exact line to match and replace"},
        "new_string": {"type": "string", "description": "The replacement line content"}
      },
      "required": ["path", "old_string", "new_string"]
    }
  )

proc executeCommandTool(): Tool =
  Tool(
    name: "execute_command",
    description: "Execute a shell command on the system. Returns stdout, stderr, and exit code.",
    parameters: %*{
      "type": "object",
      "properties": {
        "command": {"type": "string", "description": "Shell command to execute"}
      },
      "required": ["command"]
    }
  )

proc searchFilesTool(): Tool =
  Tool(
    name: "search_files",
    description: "Search for regex pattern matches in files within a directory. Supports glob file pattern filtering.",
    parameters: %*{
      "type": "object",
      "properties": {
        "directory": {"type": "string", "description": "Directory to search in"},
        "regex": {"type": "string", "description": "Regular expression pattern to search for"},
        "file_pattern": {"type": "string", "description": "Optional glob pattern to filter files (e.g. *.nim)"}
      },
      "required": ["directory", "regex"]
    }
  )

proc listFilesTool(): Tool =
  Tool(
    name: "list_files",
    description: "List files and directories at the specified path.",
    parameters: %*{
      "type": "object",
      "properties": {
        "path": {"type": "string", "description": "Directory path to list"}
      },
      "required": ["path"]
    }
  )

proc attemptCompletionTool(): Tool =
  Tool(
    name: "attempt_completion",
    description: "Signal that the task is complete. Provide a result summary for the user.",
    parameters: %*{
      "type": "object",
      "properties": {
        "result": {"type": "string", "description": "Summary of what was accomplished"}
      },
      "required": ["result"]
    }
  )

proc getToolDefinitions*(): seq[Tool] =
  @[
    readFileTool(),
    writeToFileTool(),
    replaceInFileTool(),
    executeCommandTool(),
    searchFilesTool(),
    listFilesTool(),
    attemptCompletionTool()
  ]

proc executeTool*(name: string, args: JsonNode): string =
  case name
  of "read_file":
    let path = args{"path"}.getStr("")
    if path.len == 0:
      return "Error: path parameter is required"
    let startLine = args{"start_line"}.getInt(0).int
    let endLine = args{"end_line"}.getInt(0).int
    let res = readFileRange(path, startLine, endLine)
    if res.error != FileReaderError.Success:
      return "Error: " & res.errorMessage
    result = res.content

  of "write_to_file":
    let path = args{"path"}.getStr("")
    if path.len == 0:
      return "Error: path parameter is required"
    let content = args{"content"}.getStr("")
    let res = writeFileContent(path, content)
    if res.error != FileWriterError.Success:
      return "Error: " & res.errorMessage
    result = "File written successfully."

  of "replace_in_file":
    let path = args{"path"}.getStr("")
    if path.len == 0:
      return "Error: path parameter is required"
    let oldStr = args{"old_string"}.getStr("")
    if oldStr.len == 0:
      return "Error: old_string parameter is required"
    let newStr = args{"new_string"}.getStr("")
    let res = editFile(path, oldStr, newStr)
    if res.error != FileEditError.Success:
      return "Error: " & res.errorMessage
    result = "File updated. " & $res.matchCount & " match(es) replaced."

  of "execute_command":
    let command = args{"command"}.getStr("")
    if command.len == 0:
      return "Error: command parameter is required"
    let res = execCommand(command)
    var output = ""
    if res.stdout.len > 0:
      output.add("STDOUT:\n" & res.stdout & "\n")
    if res.stderr.len > 0:
      output.add("STDERR:\n" & res.stderr & "\n")
    output.add("Exit code: " & $res.exitCode)
    if res.abnormalExit:
      output.add(" (abnormal exit)")
    if res.executionTime > 0:
      output.add("\nExecution time: " & formatFloat(res.executionTime, ffDecimal, 2) & "s")
    result = output

  of "search_files":
    let directory = args{"directory"}.getStr("")
    if directory.len == 0:
      return "Error: directory parameter is required"
    let regex = args{"regex"}.getStr("")
    if regex.len == 0:
      return "Error: regex parameter is required"
    let filePattern = args{"file_pattern"}.getStr("*")
    let res = searchFiles(directory, regex, filePattern)
    if res.error != SearchFilesError.sfeSuccess:
      return "Error: " & res.errorMessage
    if res.matchCount == 0:
      result = "No matches found."
    else:
      result = res.results & "\n(" & $res.matchCount & " matches found)"

  of "list_files":
    let path = args{"path"}.getStr("")
    if path.len == 0:
      return "Error: path parameter is required"
    let res = listFiles(path)
    if res.error != ListFilesError.Success:
      return "Error: " & res.errorMessage
    if res.count == 0:
      result = "(empty directory)"
    else:
      var lines: seq[string] = @[]
      for e in res.entries:
        lines.add("  " & e)
      result = lines.join("\n")
      result.add("\n\n" & $res.count & " entries")
      if res.didHitLimit:
        result.add(" (list truncated at " & $MAX_LIST_ENTRIES & ")")

  of "attempt_completion":
    result = "[COMPLETION]" & args{"result"}.getStr("")

  else:
    result = "Error: Unknown tool: " & name