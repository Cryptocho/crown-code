import std/[unittest, options]
import re
import search

suite "newSearch":
  test "compile valid pattern":
    let s = newSearch("hello")
    check s != nil

  test "compile with invalid pattern raises":
    expect(RegexError):
      discard newSearch("[")

  test "compile with options":
    let s = newSearch("hello", {soCaseInsensitive})
    check s != nil

suite "matchFirst":
  test "basic match":
    let s = newSearch("world")
    let m = s.matchFirst("hello world")
    let has = m.isSome()
    check has
    let v = m.get
    check v.lineNumber == 1
    check v.columnStart == 6
    check v.columnEnd == 11
    check v.line == "hello world"

  test "no match returns none":
    let s = newSearch("xyz")
    let has = s.matchFirst("hello world").isSome()
    check not has

  test "empty text returns none":
    let s = newSearch("hello")
    let has = s.matchFirst("").isSome()
    check not has

  test "match at start":
    let s = newSearch("hello")
    let m = s.matchFirst("hello world")
    let has = m.isSome()
    check has
    check m.get.columnStart == 0
    check m.get.columnEnd == 5

  test "match with offset":
    let s = newSearch("test")
    let text = "test test test"
    let m = s.matchFirst(text, offset = 5)
    let has = m.isSome()
    check has
    check m.get.columnStart == 5
    check m.get.columnEnd == 9

  test "offset past end returns none":
    let s = newSearch("test")
    let text = "test"
    let has = s.matchFirst(text, offset = 10).isSome()
    check not has

  test "match across lines":
    let s = newSearch("line2")
    let text = "line1\nline2\nline3"
    let m = s.matchFirst(text)
    let has = m.isSome()
    check has
    let v = m.get
    check v.lineNumber == 2
    check v.line == "line2"

suite "matchAll":
  test "single match same as matchFirst":
    let s = newSearch("test")
    let text = "this is a test"
    let all = s.matchAll(text)
    check all.len == 1
    check all[0].columnStart == 10

  test "multiple matches":
    let s = newSearch("ab")
    let text = "ab ab ab"
    let all = s.matchAll(text)
    check all.len == 3
    check all[0].columnStart == 0
    check all[1].columnStart == 3
    check all[2].columnStart == 6

  test "no match returns empty seq":
    let s = newSearch("xyz")
    check s.matchAll("hello world").len == 0

  test "empty text returns empty seq":
    let s = newSearch("hello")
    check s.matchAll("").len == 0

suite "calcLineNumber":
  test "offset 0 is line 1":
    check calcLineNumber("hello world", 0) == 1

  test "first line offset":
    check calcLineNumber("hello world", 3) == 1

  test "second line":
    let text = "line1\nline2\nline3"
    check calcLineNumber(text, 7) == 2

  test "empty text":
    check calcLineNumber("", 0) == 1

  test "exact at newline":
    let text = "a\nb\nc"
    check calcLineNumber(text, 1) == 1
    check calcLineNumber(text, 2) == 2

suite "getLine":
  test "get existing line":
    let text = "line1\nline2\nline3"
    let line = getLine(text, 2)
    let has = line.isSome()
    check has
    check line.get == "line2"

  test "get first line":
    let text = "hello\nworld"
    let line = getLine(text, 1)
    let has = line.isSome()
    check has
    check line.get == "hello"

  test "get last line (no trailing newline)":
    let text = "hello\nworld"
    let line = getLine(text, 2)
    let has = line.isSome()
    check has
    check line.get == "world"

  test "line number too high returns none":
    let text = "hello"
    let has = getLine(text, 99).isSome()
    check not has

  test "line number 0 returns none":
    let text = "hello"
    let has = getLine(text, 0).isSome()
    check not has

  test "empty text returns none":
    let has = getLine("", 1).isSome()
    check not has

  test "single line no newline":
    let text = "just one line"
    let line = getLine(text, 1)
    let has = line.isSome()
    check has
    check line.get == "just one line"

suite "options":
  test "soCaseInsensitive":
    let s = newSearch("HELLO", {soCaseInsensitive})
    let m = s.matchFirst("hello world")
    let has = m.isSome()
    check has

  test "without soCaseInsensitive no match":
    let s = newSearch("HELLO")
    let has = s.matchFirst("hello world").isSome()
    check not has

  test "soMultiLine ^ anchor":
    let s = newSearch("^hello", {soMultiLine})
    let text = "foo\nhello\nbar"
    let m = s.matchFirst(text)
    let has = m.isSome()
    check has
    check m.get.lineNumber == 2

  test "soDotAll . matches newline":
    let s = newSearch("a.b", {soDotAll})
    let text = "a\nb"
    let m = s.matchFirst(text)
    let has = m.isSome()
    check has

  test "without soDotAll . does not match newline":
    let s = newSearch("a.b")
    let text = "a\nb"
    let has = s.matchFirst(text).isSome()
    check not has