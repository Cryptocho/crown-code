import std/unittest
import std/strutils
import agent/prompt

suite "buildSystemPrompt":
  test "contains role description":
    let prompt = buildSystemPrompt("/test/cwd")
    check prompt.contains("Crown Code")
    check prompt.contains("software engineer")

  test "contains cwd":
    let prompt = buildSystemPrompt("/test/cwd")
    check prompt.contains("/test/cwd")

  test "contains OS info":
    let prompt = buildSystemPrompt("/test/cwd")
    check prompt.contains("Operating System:")

  test "contains shell info":
    let prompt = buildSystemPrompt("/test/cwd")
    check prompt.contains("Default Shell:")

  test "contains TOOL USE section":
    let prompt = buildSystemPrompt("/test/cwd")
    check prompt.contains("TOOL USE")
    check prompt.contains("attempt_completion")

  test "contains RULES section":
    let prompt = buildSystemPrompt("/test/cwd")
    check prompt.contains("RULES")
    check prompt.contains("read_file")
    check prompt.contains("replace_in_file")

  test "handle empty cwd":
    let prompt = buildSystemPrompt("")
    check prompt.contains("Current Working Directory:")

  test "all sections present in order":
    let prompt = buildSystemPrompt("/cwd")
    check prompt.startsWith("You are Crown Code")
    check prompt.contains("\n\nTOOL USE\n")
    check prompt.contains("\n\nRULES\n")
    check prompt.contains("\n\nSYSTEM INFORMATION\n")