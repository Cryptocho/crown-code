import unittest
import std/os
import ignore_rules

suite "loadIgnoreFile":
  test "non-existent file returns empty seq":
    check loadIgnoreFile("/tmp/nonexistent_ignore_file_12345") == newSeq[string]()

  test "# comment lines are skipped":
    let path = "/tmp/test_ignore_comments.tmp"
    writeFile(path, "# this is a comment\n*.nim\n")
    let result = loadIgnoreFile(path)
    check result == @["*.nim"]
    removeFile(path)

  test "trailing whitespace is stripped":
    let path = "/tmp/test_ignore_trim.tmp"
    writeFile(path, "*.nim  \t\n*.log\n")
    let result = loadIgnoreFile(path)
    check result == @["*.nim", "*.log"]
    removeFile(path)

  test "empty lines are skipped":
    let path = "/tmp/test_ignore_empty.tmp"
    writeFile(path, "\n\n*.nim\n\n*.txt\n")
    let result = loadIgnoreFile(path)
    check result == @["*.nim", "*.txt"]
    removeFile(path)

  test "file with only comments and empty lines returns empty":
    let path = "/tmp/test_ignore_only_comments.tmp"
    writeFile(path, "# comment\n\n  \n# another\n")
    let result = loadIgnoreFile(path)
    check result == newSeq[string]()
    removeFile(path)

suite "checkIgnorePath - with project .clineignore":
  test "no .clineignore returns false":
    resetIgnoreRules()
    check not checkIgnorePath("test.nim")

  test "simple glob pattern matches":
    resetIgnoreRules()
    let path = ".clineignore"
    writeFile(path, "*.nim\n")
    check checkIgnorePath("test.nim")
    check not checkIgnorePath("test.txt")
    removeFile(path)

  test "pattern without slash matches one-level subdirectories via */ prefix":
    resetIgnoreRules()
    let path = ".clineignore"
    writeFile(path, "*.nim\n")
    check checkIgnorePath("src/test.nim")
    check not checkIgnorePath("dir/sub/test.nim")
    check not checkIgnorePath("src/test.txt")
    removeFile(path)

  test "pattern with slash does not match deeper subdirectories":
    resetIgnoreRules()
    let path = ".clineignore"
    writeFile(path, "dir/test.nim\n")
    check checkIgnorePath("dir/test.nim")
    check not checkIgnorePath("dir/sub/test.nim")
    removeFile(path)

  test "negation pattern excludes file":
    resetIgnoreRules()
    let path = ".clineignore"
    writeFile(path, "!src/*.nim\n")
    check not checkIgnorePath("src/foo.nim")
    check checkIgnorePath("foo.nim")
    removeFile(path)

  test "empty path returns false":
    resetIgnoreRules()
    check not checkIgnorePath("")

  test "absolute path is converted to relative":
    resetIgnoreRules()
    let path = ".clineignore"
    writeFile(path, "*.log\n")
    let cwd = getCurrentDir()
    check checkIgnorePath(cwd / "test.log")
    check not checkIgnorePath(cwd / "test.nim")
    removeFile(path)
