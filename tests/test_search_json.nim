import search_json
import context, search
import std/unittest

suite "jsonEscape":
  test "regular string":
    check jsonEscape("hello") == "\"hello\""

  test "double quote":
    check jsonEscape("a\"b") == "\"a\\\"b\""

  test "backslash":
    check jsonEscape("a\\b") == "\"a\\\\b\""

  test "newline":
    check jsonEscape("a\nb") == "\"a\\nb\""

  test "tab":
    check jsonEscape("a\tb") == "\"a\\tb\""

  test "carriage return":
    check jsonEscape("a\rb") == "\"a\\rb\""

  test "empty string":
    check jsonEscape("") == "\"\""

  test "mixed special characters":
    check jsonEscape("say \"hello\"\n\tbye") == "\"say \\\"hello\\\"\\n\\tbye\""

  test "unicode characters":
    check jsonEscape("中文") == "\"中文\""

suite "formatStartJson":
  test "simple path":
    check formatStartJson("/foo/bar.txt") == "{\"type\":\"start\",\"path\":\"/foo/bar.txt\"}\n"

  test "path with special chars":
    check formatStartJson("/foo/bar\"baz.txt") == "{\"type\":\"start\",\"path\":\"/foo/bar\\\"baz.txt\"}\n"

  test "empty path":
    check formatStartJson("") == "{\"type\":\"start\",\"path\":\"\"}\n"

suite "formatEndJson":
  test "fixed output":
    check formatEndJson() == "{\"type\":\"end\"}\n"

suite "formatMatchJson":
  let sampleMatch = Match(
    lineNumber: 10,
    columnStart: 5,
    columnEnd: 8,
    line: "hello world",
    path: "/path/to/file.txt"
  )

  test "match only, no context":
    let result = formatMatchJson(sampleMatch, nil)
    check result == "{\"type\":\"match\",\"path\":\"/path/to/file.txt\",\"line_number\":10,\"columns\":{\"start\":5,\"end\":8},\"line\":\"hello world\"}\n"

  test "match with contextAfter":
    var ctx = newContext(0, 2)
    ctx.addLine("after line 1")
    ctx.addLine("after line 2")
    let result = formatMatchJson(sampleMatch, ctx)
    check result == "{\"type\":\"match\",\"path\":\"/path/to/file.txt\",\"line_number\":10,\"columns\":{\"start\":5,\"end\":8},\"line\":\"hello world\",\"context_after\":[\"after line 1\",\"after line 2\"]}\n"

  test "match with contextAfter having special chars":
    var ctx = newContext(0, 1)
    ctx.addLine("line with \"quotes\"")
    let result = formatMatchJson(sampleMatch, ctx)
    check result == "{\"type\":\"match\",\"path\":\"/path/to/file.txt\",\"line_number\":10,\"columns\":{\"start\":5,\"end\":8},\"line\":\"hello world\",\"context_after\":[\"line with \\\"quotes\\\"\"]}\n"

  test "match with line having special chars":
    let matchWithSpecial = Match(
      lineNumber: 5,
      columnStart: 0,
      columnEnd: 4,
      line: "a\tb",
      path: "/path.txt"
    )
    let result = formatMatchJson(matchWithSpecial, nil)
    check result == "{\"type\":\"match\",\"path\":\"/path.txt\",\"line_number\":5,\"columns\":{\"start\":0,\"end\":4},\"line\":\"a\\tb\"}\n"

  test "nil match returns empty string":
    check formatMatchJson(nil, nil) == ""

  test "match with nil context (no context fields)":
    var ctx: Context = nil
    let result = formatMatchJson(sampleMatch, ctx)
    check result == "{\"type\":\"match\",\"path\":\"/path/to/file.txt\",\"line_number\":10,\"columns\":{\"start\":5,\"end\":8},\"line\":\"hello world\"}\n"

  test "context_before and context_after both present":
    var ctx = newContext(1, 1)
    # Note: addLine only fills linesAfter, linesBefore remains empty (beforeCount stays 0)
    # So context_before won't be emitted even if beforeMax > 0
    discard ctx.beforeMax  # suppress unused warning
    ctx.addLine("after line")
    let result = formatMatchJson(sampleMatch, ctx)

    # ctx.beforeCount == 0 because addLine only fills linesAfter, so context_before not emitted
    # This matches C behavior: before_count == 0, before array omitted
    check result == "{\"type\":\"match\",\"path\":\"/path/to/file.txt\",\"line_number\":10,\"columns\":{\"start\":5,\"end\":8},\"line\":\"hello world\",\"context_after\":[\"after line\"]}\n"

  test "empty context (zero counts)":
    var ctx = newContext(0, 0)
    let result = formatMatchJson(sampleMatch, ctx)
    check result == "{\"type\":\"match\",\"path\":\"/path/to/file.txt\",\"line_number\":10,\"columns\":{\"start\":5,\"end\":8},\"line\":\"hello world\"}\n"