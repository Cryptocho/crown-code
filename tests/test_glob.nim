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
