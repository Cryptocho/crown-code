import unittest
import std/os
import std/strutils
import file_edit
import ignore_rules

suite "file_edit: error handling":
  test "null_path_returns_file_not_found":
    let result = editFile("", "old", "new")
    check result.error == FileEditError.FileNotFound
    check result.errorMessage == "Could not resolve path"

  test "file_not_found_returns_read_failed":
    let result = editFile("/nonexistent_dir_12345/file.txt", "old", "new")
    check result.error == FileEditError.ReadFailed

  test "old_string_not_found_returns_old_string_not_found":
    let path = "/tmp/test_edit_notfound_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "line1\nline2\nline3\n")
    let result = editFile(path, "notfound", "new")
    check result.error == FileEditError.OldStringNotFound
    check result.errorMessage == "Could not find exact match for oldStr in file"
    removeFile(path)

  test "multiple_matches_without_multiple_flag":
    let path = "/tmp/test_edit_multi_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "hello\nworld\nhello\n")
    let result = editFile(path, "hello", "hi")
    check result.error == FileEditError.MultipleMatches
    check result.errorMessage.contains("multiple")
    check result.matchCount == 2
    # 验证文件内容未改变
    check readFile(path) == "hello\nworld\nhello\n"
    removeFile(path)

suite "file_edit: basic functionality":
  test "single_match_replaces_correctly":
    let path = "/tmp/test_edit_single_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "line1\nold_line\nline3\n")
    let result = editFile(path, "old_line", "new_line")
    check result.error == FileEditError.Success
    check result.matchCount == 1
    check readFile(path) == "line1\nnew_line\nline3\n"
    removeFile(path)

  test "all_matches_replaced_when_multiple_true":
    let path = "/tmp/test_edit_all_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "hello\nworld\nhello\nworld\nhello\n")
    let result = editFile(path, "hello", "hi", multiple = true)
    check result.error == FileEditError.Success
    check result.matchCount == 3
    check readFile(path) == "hi\nworld\nhi\nworld\nhi\n"
    removeFile(path)

  test "multiple_matches_rejected_when_multiple_false":
    let path = "/tmp/test_edit_first_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "hello\nworld\nhello\n")
    let result = editFile(path, "hello", "hi", multiple = false)
    check result.error == FileEditError.MultipleMatches
    check result.matchCount == 2
    # 文件内容未改变
    check readFile(path) == "hello\nworld\nhello\n"
    removeFile(path)

  test "first_line_match":
    let path = "/tmp/test_edit_firstline_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "target\nmiddle\nend\n")
    let result = editFile(path, "target", "replaced")
    check result.error == FileEditError.Success
    check readFile(path) == "replaced\nmiddle\nend\n"
    removeFile(path)

  test "last_line_match":
    let path = "/tmp/test_edit_lastline_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "first\nmiddle\ntarget")
    let result = editFile(path, "target", "replaced")
    check result.error == FileEditError.Success
    check readFile(path) == "first\nmiddle\nreplaced"
    removeFile(path)

  test "single_line_file_match":
    let path = "/tmp/test_edit_oneline_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "only")
    let result = editFile(path, "only", "replaced")
    check result.error == FileEditError.Success
    check readFile(path) == "replaced"
    removeFile(path)

suite "file_edit: edge cases":
  test "empty_old_string_matches_empty_lines":
    let path = "/tmp/test_edit_emptyold_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "a\n\nc\n")
    let result = editFile(path, "", "FILLED", multiple = true)
    check result.error == FileEditError.Success
    check result.matchCount == 2
    check readFile(path) == "a\nFILLED\nc\nFILLED"
    removeFile(path)

  test "new_string_with_newlines_replaces_as_single_line":
    let path = "/tmp/test_edit_newlinestr_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "before\ntarget\nafter\n")
    let result = editFile(path, "target", "line1\nline2")
    check result.error == FileEditError.Success
    # C 的 strdup 行为：newStr 按原样替换整行，不拆分成多行
    check readFile(path) == "before\nline1\nline2\nafter\n"
    removeFile(path)

  test "file_with_trailing_newline":
    let path = "/tmp/test_edit_trailnl_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "abc\n")
    let result = editFile(path, "abc", "xyz")
    check result.error == FileEditError.Success
    # splitIntoLines("abc\n") → ["abc", ""]，匹配 "abc" 成功
    check readFile(path) == "xyz\n"
    removeFile(path)

  test "empty_file_returns_old_string_not_found":
    let path = "/tmp/test_edit_emptyfile_" & $getCurrentProcessId() & ".txt"
    writeFile(path, "")
    let result = editFile(path, "anything", "new")
    check result.error == FileEditError.OldStringNotFound
    removeFile(path)

suite "file_edit: access control":
  test "clineignore_blocks_edit":
    resetIgnoreRules()
    let ignorePath = ".clineignore"
    writeFile(ignorePath, "*.secret\n")
    let path = "test_edit.secret"
    writeFile(path, "old_data\n")
    let result = editFile(path, "old_data", "new_data")
    check result.error == FileEditError.ReadFailed
    check result.errorMessage == "Access denied by .clineignore rules"
    removeFile(ignorePath)
    removeFile(path)