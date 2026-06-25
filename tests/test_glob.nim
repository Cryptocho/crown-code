import unittest
import glob

suite "fnmatch - basic wildcards":
  test "exact match":
    check matchGlob("hello", "hello")
    check not matchGlob("hello", "world")

  test "star matches everything":
    check matchGlob("anything", "*")
    check matchGlob("", "*")
    check matchGlob("file.txt", "*.txt")
    check matchGlob("file.txt", "f*")
    check matchGlob("file.txt", "*e*")
    check not matchGlob("file.txt", "*.md")

  test "question mark matches single char":
    check matchGlob("a", "?")
    check matchGlob("ab", "a?")
    check matchGlob("ab", "?b")
    check not matchGlob("ab", "?")
    check not matchGlob("", "?")

  test "star and question mark combined":
    check matchGlob("file.txt", "f?le.*")
    check matchGlob("file.txt", "f?*e.*")
    check not matchGlob("file.txt", "?z*")

suite "fnmatch - character classes":
  test "positive class single chars":
    check matchGlob("a", "[abc]")
    check matchGlob("b", "[abc]")
    check matchGlob("c", "[abc]")
    check not matchGlob("d", "[abc]")

  test "positive class with range":
    check matchGlob("a", "[a-z]")
    check matchGlob("m", "[a-z]")
    check matchGlob("z", "[a-z]")
    check matchGlob("5", "[0-9]")
    check not matchGlob("A", "[a-z]")

  test "negated class":
    check matchGlob("d", "[!abc]")
    check not matchGlob("a", "[!abc]")
    check matchGlob("A", "[!a-z]")
    check not matchGlob("m", "[!a-z]")

  test "class with literal bracket":
    check matchGlob("]", "[]]")
    check matchGlob("a]", "[a]]")
    check not matchGlob("b", "[a]]")

  test "negated class with literal bracket":
    check matchGlob("a", "[!]]")
    check not matchGlob("]", "[!]]")

  test "class with literal dash at start":
    check matchGlob("-", "[-a]")
    check matchGlob("a", "[-a]")
    check not matchGlob("b", "[-a]")

  test "class with literal dash at end":
    check matchGlob("-", "[a-]")
    check matchGlob("a", "[a-]")
    check not matchGlob("b", "[a-]")

suite "matchGlob - negation prefix":
  test "! prefix negates match":
    check matchGlob("hello", "!world")
    check not matchGlob("hello", "!hello")
    check matchGlob("anything", "!")

  test "negate with wildcards":
    check matchGlob("file.txt", "!*.md")
    check not matchGlob("file.txt", "!*.txt")

suite "matchGlob - edge cases":
  test "empty pattern returns false":
    check not matchGlob("anything", "")
    check not matchGlob("", "")

  test "empty string matching":
    check matchGlob("", "*")
    check not matchGlob("", "!")
    check not matchGlob("", "?")
    check matchGlob("a", "!")

suite "fnmatchPathname - pathname semantics":
  test "star does not match path separator":
    check not matchGlobPathname("dir/file.nim", "*.nim")
    check matchGlobPathname("file.nim", "*.nim")

  test "*/ prefix trick matches single-level subdir (not deep)":
    check matchGlobPathname("dir/file.nim", "*/file.nim")
    check not matchGlobPathname("deep/dir/file.nim", "*/file.nim")
    check not matchGlobPathname("dir/other.nim", "*/file.nim")

  test "question mark does not match path separator":
    check not matchGlobPathname("/a.txt", "?.txt")
    check matchGlobPathname("a.txt", "?.txt")

  test "star matches within path segment":
    check matchGlobPathname("a/b/c.nim", "a/*/c.nim")
    check not matchGlobPathname("a/b/d/c.nim", "a/*/c.nim")

  test "character class does not match slash":
    check not matchGlobPathname("/a.nim", "[abc].nim")
    check matchGlobPathname("a.nim", "[abc].nim")

  test "negation with pathname":
    check matchGlobPathname("file.nim", "!*.txt")
    check not matchGlobPathname("file.nim", "!*.nim")

  test "exact path match":
    check matchGlobPathname("dir/sub/file.nim", "dir/sub/file.nim")
    check not matchGlobPathname("dir/sub/file.nim", "dir/other.nim")

  test "trailing star in pathname":
    check matchGlobPathname("dir/file", "dir/*")
    check not matchGlobPathname("dir/sub/file", "dir/*")

  test "empty pattern returns false":
    check not matchGlobPathname("anything", "")
    check not matchGlobPathname("", "")

  test "star matches empty string (without slash)":
    check matchGlobPathname("", "*")
    check not matchGlobPathname("/", "*")

suite "matchAnyGlob - multi pattern":
  test "single positive pattern":
    check matchAnyGlob("hello", ["hello"])
    check not matchAnyGlob("hello", ["world"])

  test "multiple positive patterns":
    check matchAnyGlob("hello", ["world", "hello", "foo"])
    check matchAnyGlob("test.txt", ["*.md", "*.txt", "*.nim"])
    check not matchAnyGlob("test.rs", ["*.md", "*.txt", "*.nim"])

  test "negation pattern short-circuits to false":
    check not matchAnyGlob("hello", ["*", "!hello"])
    check not matchAnyGlob("test.txt", ["*.txt", "!t*t.txt"])
    check not matchAnyGlob("test.txt", ["!t*t.txt", "*.txt"])

  test "negation does not apply to unmatched files":
    check matchAnyGlob("hello", ["*", "!world"])

  test "only negation patterns match nothing":
    check not matchAnyGlob("hello", ["!hello"])
    check not matchAnyGlob("hello", ["!*"])

  test "empty patterns array returns false":
    check not matchAnyGlob("hello", [])
    check not matchAnyGlob("", [])

  test "empty string filename returns false":
    check not matchAnyGlob("", ["*"])
    check not matchAnyGlob("", ["!*"])

  test "empty individual patterns are skipped":
    check matchAnyGlob("hello", ["", "hello", ""])
    check not matchAnyGlob("hello", ["", "world", ""])
