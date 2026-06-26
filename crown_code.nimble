# Package

version       = "0.1.0"
author        = "crown-code"
description   = "A vibe coding TUI tool"
license       = "MIT"
srcDir        = "src"
bin           = @["crown_code"]

# Dependencies

requires "nim >= 2.0.0"

requires "notcurses#head"

# Tasks

task test, "Run all tests":
  exec "nim c -r -o:build/test/test_runner tests/test_runner.nim"
