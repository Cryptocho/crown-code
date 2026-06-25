import unittest
import std/os
import formatter

suite "formatter: error handling":
  test "null_path returns NullPath error":
    let result = formatFile("")
    check result.error == FormatterError.NullPath
    check result.errorMessage == "Path parameter is required"

  test "empty_path returns NullPath error":
    let result = formatFile("")
    check result.error == FormatterError.NullPath
    check result.errorMessage == "Path parameter is required"

  test "file_not_found returns ReadFailed error":
    let result = formatFile("/nonexistent/format_test.txt")
    check result.error == FormatterError.ReadFailed

suite "formatter: content formatting":
  test "trailing_spaces_trimmed":
    let path = "/tmp/test_format_trailing.txt"
    writeFile(path, "hello   \nworld  \n")
    let result = formatFile(path)
    check result.error == FormatterError.Success
    let formatted = readFile(path)
    check formatted == "hello\nworld\n"
    removeFile(path)

  test "leading_tabs_replaced_with_4_spaces":
    let path = "/tmp/test_format_tab.txt"
    writeFile(path, "\t\tline1\n\tline2\n")
    let result = formatFile(path)
    check result.error == FormatterError.Success
    let formatted = readFile(path)
    check formatted == "    line1\n    line2\n"
    removeFile(path)

  test "mixed_tabs_spaces_and_trailing_whitespace":
    let path = "/tmp/test_format_mixed.txt"
    writeFile(path, "\t  hello   \n  world  \n")
    let result = formatFile(path)
    check result.error == FormatterError.Success
    let formatted = readFile(path)
    # Tab 行首 → 4 空格；空格行首 → 移除；行尾空格 → 修剪
    check formatted == "    hello\nworld\n"
    removeFile(path)

  test "spaces_and_tabs_mixed_leading":
    let path = "/tmp/test_format_st.txt"
    writeFile(path, "\t \t hello\n")
    let result = formatFile(path)
    check result.error == FormatterError.Success
    let formatted = readFile(path)
    check formatted == "    hello\n"
    removeFile(path)

  test "only_spaces_leading_removed":
    let path = "/tmp/test_format_spaces.txt"
    writeFile(path, "  hello\n")
    let result = formatFile(path)
    check result.error == FormatterError.Success
    let formatted = readFile(path)
    check formatted == "hello\n"
    removeFile(path)

  test "no_trailing_newline_preserved":
    let path = "/tmp/test_format_nonl.txt"
    writeFile(path, "line1\nline2")
    let result = formatFile(path)
    check result.error == FormatterError.Success
    let formatted = readFile(path)
    check formatted == "line1\nline2"
    removeFile(path)

  test "empty_file_returns_success":
    let path = "/tmp/test_format_empty.txt"
    writeFile(path, "")
    let result = formatFile(path)
    check result.error == FormatterError.Success
    let formatted = readFile(path)
    check formatted == ""
    removeFile(path)