import std/json
import std/os
import std/strutils
import std/unittest
import agent/tools
import ignore_rules

suite "tool definitions":
  test "getToolDefinitions returns 7 tools":
    let tools = getToolDefinitions()
    check tools.len == 7

  test "each tool has name, description, parameters":
    let tools = getToolDefinitions()
    for t in tools:
      check t.name.len > 0
      check t.description.len > 0
      check t.parameters != nil
      check t.parameters.kind == JObject

  test "read_file tool has correct structure":
    let tools = getToolDefinitions()
    let tool = tools[0]
    check tool.name == "read_file"
    check tool.parameters["required"] == %*["path"]
    check tool.parameters["properties"]["path"] != nil
    check tool.parameters["properties"]["start_line"] != nil
    check tool.parameters["properties"]["end_line"] != nil

  test "write_to_file tool has correct structure":
    let tools = getToolDefinitions()
    let tool = tools[1]
    check tool.name == "write_to_file"
    check tool.parameters["required"] == %*["path", "content"]

  test "replace_in_file tool has correct structure":
    let tools = getToolDefinitions()
    let tool = tools[2]
    check tool.name == "replace_in_file"
    check tool.parameters["required"] == %*["path", "old_string", "new_string"]

  test "execute_command tool has correct structure":
    let tools = getToolDefinitions()
    let tool = tools[3]
    check tool.name == "execute_command"
    check tool.parameters["required"] == %*["command"]

  test "search_files tool has correct structure":
    let tools = getToolDefinitions()
    let tool = tools[4]
    check tool.name == "search_files"
    check tool.parameters["required"] == %*["directory", "regex"]

  test "list_files tool has correct structure":
    let tools = getToolDefinitions()
    let tool = tools[5]
    check tool.name == "list_files"
    check tool.parameters["required"] == %*["path"]

  test "attempt_completion tool has correct structure":
    let tools = getToolDefinitions()
    let tool = tools[6]
    check tool.name == "attempt_completion"
    check tool.parameters["required"] == %*["result"]

suite "executeTool - error handling":
  test "unknown tool returns error":
    let result = executeTool("nonexistent_tool", %*{})
    check startsWith(result, "Error: Unknown tool")

  test "read_file with empty path returns error":
    let result = executeTool("read_file", %*{"path": ""})
    check startsWith(result, "Error: path parameter is required")

  test "write_to_file with empty path returns error":
    let result = executeTool("write_to_file", %*{"path": "", "content": "test"})
    check startsWith(result, "Error: path parameter is required")

  test "replace_in_file with empty path returns error":
    let result = executeTool("replace_in_file", %*{"path": "", "old_string": "a", "new_string": "b"})
    check startsWith(result,"Error: path parameter is required")

  test "replace_in_file with empty old_string returns error":
    let result = executeTool("replace_in_file", %*{"path": "test.txt", "old_string": "", "new_string": "b"})
    check startsWith(result,"Error: old_string parameter is required")

  test "execute_command with empty command returns error":
    let result = executeTool("execute_command", %*{"command": ""})
    check startsWith(result,"Error: command parameter is required")

  test "search_files with empty directory returns error":
    let result = executeTool("search_files", %*{"directory": "", "regex": "pattern"})
    check startsWith(result,"Error: directory parameter is required")

  test "search_files with empty regex returns error":
    let result = executeTool("search_files", %*{"directory": "/tmp", "regex": ""})
    check startsWith(result,"Error: regex parameter is required")

  test "list_files with empty path returns error":
    let result = executeTool("list_files", %*{"path": ""})
    check startsWith(result,"Error: path parameter is required")

suite "executeTool - basic tools":
  test "execute_command returns formatted output":
    let result = executeTool("execute_command", %*{"command": "echo hello"})
    check result.contains("STDOUT:")
    check result.contains("hello")
    check result.contains("Exit code: 0")

  test "execute_command captures stderr":
    let result = executeTool("execute_command", %*{"command": "echo error_message >&2"})
    check result.contains("STDERR:")
    check result.contains("error_message")

  test "execute_command captures non-zero exit code":
    let result = executeTool("execute_command", %*{"command": "false"})
    check result.contains("Exit code: 1")

  test "read_file reads existing file":
    let testDir = getTempDir() / "test_agent_tools_read"
    createDir(testDir)
    let testFile = testDir / "test.txt"
    writeFile(testFile, "line1\nline2\nline3\n")
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    resetIgnoreRules()

    let result = executeTool("read_file", %*{"path": "test.txt"})
    check result.contains("1 | line1")
    check result.contains("2 | line2")
    check result.contains("3 | line3")

    setCurrentDir(origDir)
    removeDir(testDir)

  test "write_to_file creates file":
    let testDir = getTempDir() / "test_agent_tools_write"
    createDir(testDir)
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    resetIgnoreRules()

    let result = executeTool("write_to_file", %*{"path": "newfile.txt", "content": "hello world"})
    check result == "File written successfully."
    check readFile("newfile.txt") == "hello world"

    setCurrentDir(origDir)
    removeDir(testDir)

  test "list_files lists directory contents":
    let testDir = getTempDir() / "test_agent_tools_list"
    createDir(testDir)
    writeFile(testDir / "a.txt", "")
    writeFile(testDir / "b.txt", "")
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    resetIgnoreRules()

    let result = executeTool("list_files", %*{"path": testDir})
    check result.contains("  a.txt")
    check result.contains("  b.txt")
    check result.contains("2 entries")

    setCurrentDir(origDir)
    removeDir(testDir)

  test "search_files finds pattern":
    let testDir = getTempDir() / "test_agent_tools_search"
    createDir(testDir)
    writeFile(testDir / "search.nim", "proc hello*\n")
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    resetIgnoreRules()

    let result = executeTool("search_files", %*{"directory": testDir, "regex": "hello", "file_pattern": "*.nim"})
    check result.contains("hello")
    check result.contains("1 matches found")

    setCurrentDir(origDir)
    removeDir(testDir)

  test "search_files returns no matches message":
    let testDir = getTempDir() / "test_agent_tools_nomatch"
    createDir(testDir)
    writeFile(testDir / "test.nim", "some content\n")
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    resetIgnoreRules()

    let result = executeTool("search_files", %*{"directory": testDir, "regex": "zzz_nonexistent", "file_pattern": "*.nim"})
    check result == "No matches found."

    setCurrentDir(origDir)
    removeDir(testDir)

  test "attempt_completion returns special prefix":
    let result = executeTool("attempt_completion", %*{"result": "Done!"})
    check result == "[COMPLETION]Done!"

  test "attempt_completion with empty result":
    let result = executeTool("attempt_completion", %*{"result": ""})
    check result == "[COMPLETION]"

suite "executeTool - replace_in_file":
  test "replace_in_file edits content":
    let testDir = getTempDir() / "test_agent_tools_replace"
    createDir(testDir)
    let testFile = testDir / "edit.txt"
    writeFile(testFile, "old line\nkeep line\n")
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    resetIgnoreRules()

    let result = executeTool("replace_in_file", %*{"path": "edit.txt", "old_string": "old line", "new_string": "new line"})
    check result.contains("File updated")
    let content = readFile("edit.txt")
    check content.contains("new line")
    check content.contains("keep line")

    setCurrentDir(origDir)
    removeDir(testDir)

  test "replace_in_file returns error for nonexistent file":
    let result = executeTool("replace_in_file", %*{"path": "/nonexistent_file_xyz", "old_string": "a", "new_string": "b"})
    check startsWith(result,"Error:")

  test "replace_in_file returns error for non-matching old_string":
    let testDir = getTempDir() / "test_agent_tools_replace_nomatch"
    createDir(testDir)
    let testFile = testDir / "edit.txt"
    writeFile(testFile, "some content\n")
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    resetIgnoreRules()

    let result = executeTool("replace_in_file", %*{"path": "edit.txt", "old_string": "nonexistent line", "new_string": "replacement"})
    check startsWith(result,"Error:")

    setCurrentDir(origDir)
    removeDir(testDir)