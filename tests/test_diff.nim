import std/[unittest, strutils]
import xdiff

suite "diff - basic":
  test "identical strings":
    check diff("hello\n", "hello\n") == ""
    check diff("line1\nline2\n", "line1\nline2\n") == ""

  test "empty a":
    let result = diff("", "hello\n")
    check result.len > 0
    check result.contains("+hello")
    check result.contains("@@ -0,0 +1,1 @@")

  test "empty b":
    let result = diff("hello\n", "")
    check result.len > 0
    check result.contains("-hello")
    check result.contains("@@ -1,1 +0,0 @@")

  test "both empty":
    check diff("", "") == ""

suite "diff - single change":
  test "single line added":
    let result = diff("line1\n", "line1\nline2\n")
    check result.contains("+line2")
    check result.contains("@@")

  test "single line deleted":
    let result = diff("line1\nline2\n", "line1\n")
    check result.contains("-line2")
    check result.contains("@@")

  test "single line modified":
    let result = diff("old\n", "new\n")
    check result.contains("-old")
    check result.contains("+new")

suite "diff - context window":
  test "ctxLen = 0":
    let a = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n"
    let b = "line1\nline2\nCHG3\nCHG4\nline5\nline6\nline7\nline8\nline9\nline10\n"
    let result = diff(a, b, ctxLen = 0)
    check not result.contains(" line1")
    check not result.contains(" line5")
    check result.contains("-line3")
    check result.contains("+CHG3")

  test "ctxLen = 3 default":
    let a = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n"
    let b = "line1\nline2\nCHG3\nCHG4\nline5\nline6\nline7\nline8\nline9\nline10\n"
    let result = diff(a, b)
    check result.contains(" line1")
    check result.contains(" line2")
    check result.contains("-line3")
    check result.contains("+CHG3")
    check result.contains(" line5")

  test "ctxLen = 1":
    let a = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n"
    let b = "line1\nline2\nCHG3\nCHG4\nline5\nline6\nline7\nline8\nline9\nline10\n"
    let result = diff(a, b, ctxLen = 1)
    check not result.contains(" line1")
    check result.contains(" line2")
    check result.contains(" line5")
    check not result.contains(" line6")

  test "merge adjacent hunks":
    let a = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\n"
    let b = "line1\nCHG2\nline3\nCHG4\nline5\nline6\nline7\nline8\n"
    let result = diff(a, b, ctxLen = 1)
    check result.contains("-line2")
    check result.contains("+CHG2")
    check result.contains("-line4")
    check result.contains("+CHG4")
    check result.find("@@ -") == result.rfind("@@ -")

suite "diff - edge cases":
  test "single line no newline":
    let result = diff("hello", "world")
    check result.contains("-hello")
    check result.contains("+world")
    check result.contains("\\ No newline at end of file")

  test "mixed trailing newline":
    let result = diff("hello\n", "hello")
    check result.contains("-hello")
    check result.contains("+hello")
    check result.contains("\\ No newline at end of file")

  test "multiline with different lengths":
    let a = "line1\nline2\nline3\n"
    let b = "line1\nadded1\nadded2\nline2\nline3\n"
    let result = diff(a, b)
    check result.contains("+added1")
    check result.contains("+added2")
    check result.contains(" line1")

  test "hunk header format":
    let result = diff("line1\nline2\nline3\n", "line1\nmodified\nline3\n")
    check result.contains("@@ -1,3 +1,3 @@")

  test "hunk header zero count - pure addition":
    let result = diff("", "line1\nline2\n")
    check result.contains("@@ -0,0 +1,2 @@")

  test "hunk header zero count - pure deletion":
    let result = diff("line1\nline2\n", "")
    check result.contains("@@ -1,2 +0,0 @@")

  test "large identical prefix and suffix":
    var a = ""
    var b = ""
    for i in 1 .. 20:
      a.add("line" & $i & "\n")
      b.add("line" & $i & "\n")
    b = b.replace("line10\n", "modified10\n")
    let result = diff(a, b)
    check result.contains("-line10")
    check result.contains("+modified10")
    check not result.contains(" line6\n")
    check not result.contains(" line14\n")
