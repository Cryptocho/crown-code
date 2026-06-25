import std/os
import unittest
import pathutils

suite "normalizeSlashes":
  test "forward slashes unchanged":
    check normalizeSlashes("/a/b/c") == "/a/b/c"

  test "backslashes all replaced":
    check normalizeSlashes("\\a\\b\\c") == "/a/b/c"

  test "mixed separators unified":
    check normalizeSlashes("a\\b/c\\d") == "a/b/c/d"

  test "empty string":
    check normalizeSlashes("") == ""

suite "resolveWorkspacePath":
  test "absolute path returned as-is":
    let absPath = "/usr/bin"
    check resolveWorkspacePath(absPath) == absPath

  test "relative path prepends CWD":
    let cwd = getCurrentDir()
    let relPath = "foo/bar"
    check resolveWorkspacePath(relPath) == cwd / relPath

  test "empty string returns empty":
    check resolveWorkspacePath("") == ""

suite "toRelPath":
  let cwd = getCurrentDir()

  test "path starting with CWD strips prefix":
    let full = cwd / "subdir/file.nim"
    check toRelPath(full) == "subdir/file.nim"

  test "path outside CWD returned as-is":
    check toRelPath("/some/other/path") == "/some/other/path"

  test "path exactly equal to CWD returns empty":
    check toRelPath(cwd) == ""

  test "backslashes normalized to forward slashes":
    var fakeWin = cwd
    fakeWin.add("\\subdir\\file.nim")
    check toRelPath(fakeWin) == "subdir/file.nim"

  test "custom cwd parameter":
    let customCwd = "/tmp/test"
    let full = customCwd / "foo/bar"
    check toRelPath(full, customCwd) == "foo/bar"

suite "resolvePath":
  test "tuple contains absolute path twice":
    let (absPath, displayPath) = resolvePath("/usr/bin")
    check absPath == "/usr/bin"
    check displayPath == "/usr/bin"

  test "relative path resolved":
    let cwd = getCurrentDir()
    let (absPath, displayPath) = resolvePath("foo/bar")
    check absPath == cwd / "foo/bar"
    check displayPath == cwd / "foo/bar"
