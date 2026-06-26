import unittest
import std/[os, strutils]
import search_files
import ignore_rules

suite "searchFiles - error handling":
  test "empty directory returns sfeNullParam":
    let r = searchFiles("", "pattern", "*.nim")
    check r.error == sfeNullParam

  test "empty regex returns sfeNullParam":
    let r = searchFiles("/tmp", "", "*.nim")
    check r.error == sfeNullParam

  test "empty filePattern returns sfeNullParam":
    let r = searchFiles("/tmp", "pattern", "")
    check r.error == sfeNullParam

  test "non-existent directory returns sfeDirNotFound":
    let r = searchFiles("/tmp/nonexistent_dir_abc123", "pattern", "*.nim")
    check r.error == sfeDirNotFound

  test "invalid regex returns sfeRegexError":
    let r = searchFiles("/tmp", "[", "*.nim")
    check r.error == sfeRegexError

suite "searchFiles - directory based tests":
  var TestDir: string

  setup:
    TestDir = getCurrentDir() / ".crown_test_search_files"
    createDir(TestDir)
    resetIgnoreRules()
    writeFile(TestDir / "hello.nim", "echo \"hello world\"\nlet x = 42\necho x\n")
    writeFile(TestDir / "goodbye.nim", "echo \"goodbye\"\nlet y = 99\n")

  teardown:
    removeDir(TestDir)

  test "basic single match in one file":
    let r = searchFiles(TestDir, "hello", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 1
    check r.results.contains("hello.nim")
    check r.results.contains("│----")
    check r.results.contains("│echo \"hello world\"")

  test "no match returns empty results":
    let r = searchFiles(TestDir, "zzz_notfound_zzz", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 0
    check r.results == ""

  test "multiple matches in one file":
    let r = searchFiles(TestDir, "echo", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 3
    check r.results.contains("hello.nim")
    check r.results.contains("goodbye.nim")

  test "matches across multiple files":
    let r = searchFiles(TestDir, "echo", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 3
    check r.results.contains("hello.nim")
    check r.results.contains("goodbye.nim")

  test "glob pattern filters matching files":
    writeFile(TestDir / "data.txt", "hello world\n")
    let r = searchFiles(TestDir, "hello", "*.txt")
    check r.error == sfeSuccess
    check r.matchCount == 1
    check r.results.contains("data.txt")
    check not r.results.contains("hello.nim")

  test "context lines shown around match":
    writeFile(TestDir / "context.nim", "line before\nline match\nline after\n")
    let r = searchFiles(TestDir, "match", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 1
    check r.results.contains("│line before")
    check r.results.contains("│line match")
    check r.results.contains("│line after")

  test "first line match has no previous context":
    writeFile(TestDir / "firstline.nim", "first match\nsecond line\n")
    let r = searchFiles(TestDir, "first", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 1
    check r.results.contains("│first match")
    check r.results.contains("│second line")

  test "last line match has no next context":
    writeFile(TestDir / "lastline.nim", "first line\nlast match\n")
    let r = searchFiles(TestDir, "last", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 1
    check r.results.contains("│first line")
    check r.results.contains("│last match")

  test "empty file does not crash":
    writeFile(TestDir / "empty.nim", "")
    let r = searchFiles(TestDir, "anything", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 0

  test "ignore rules prevent file from being searched":
    # 创建 .clineignore 排除 ignored.nim
    writeFile(getCurrentDir() / ".clineignore", "ignored.nim\n")
    writeFile(TestDir / "wanted.nim", "secret = 42\n")
    writeFile(TestDir / "ignored.nim", "secret = 99\n")
    let r = searchFiles(TestDir, "secret", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 1
    check r.results.contains("wanted.nim")
    check not r.results.contains("ignored.nim")
    # cleanup .clineignore
    removeFile(getCurrentDir() / ".clineignore")

suite "searchFiles - depth limiting":
  var TestDir: string

  setup:
    TestDir = getCurrentDir() / ".crown_test_search_files_depth"
    createDir(TestDir)
    resetIgnoreRules()
    var deepDir = TestDir
    for i in 1..12:
      deepDir = deepDir / $i
      createDir(deepDir)
    writeFile(deepDir / "deep.nim", "found me\n")

  teardown:
    removeDir(TestDir)

  test "depth beyond MAX_SEARCH_DEPTH is not searched":
    let r = searchFiles(TestDir, "found", "*.nim")
    check r.error == sfeSuccess
    check r.matchCount == 0

suite "searchFiles - truncation":
  var TestDir: string

  setup:
    TestDir = getCurrentDir() / ".crown_test_search_files_trunc"
    createDir(TestDir)
    resetIgnoreRules()
    var content = ""
    for i in 1..5000:
      content.add("line " & $i & " hello\n")
    writeFile(TestDir / "huge.nim", content)

  teardown:
    removeDir(TestDir)

  test "output is truncated when exceeding MAX_SEARCH_OUTPUT":
    let r = searchFiles(TestDir, "hello", "*.nim")
    check r.error == sfeSuccess
    check r.results.contains("[Results truncated...]")
    check r.results.len <= MAX_SEARCH_OUTPUT + 200

suite "searchFiles - output format":
  var TestDir: string

  setup:
    TestDir = getCurrentDir() / ".crown_test_search_files_fmt"
    createDir(TestDir)
    resetIgnoreRules()
    writeFile(TestDir / "format.nim", "line one\nline two target\nline three\n")

  teardown:
    removeDir(TestDir)

  test "output format matches expected pattern":
    let r = searchFiles(TestDir, "target", "*.nim")
    check r.error == sfeSuccess
    check r.results.contains("\nformat.nim\n│----\n")
    check r.results.contains("│line one\n")
    check r.results.contains("│line two target\n")
    check r.results.contains("│line three\n")
    check r.results.contains("│----\n")