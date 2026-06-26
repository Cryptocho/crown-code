import unittest
import std/os
import shell_detect

suite "shell_detect: basic detection":
  test "detect_shells_returns_non_empty":
    let shells = detectShells()
    check shells.len > 0

  test "shell_name_is_non_empty":
    let shells = detectShells()
    for s in shells:
      check s.name.len > 0

  test "shell_path_is_non_empty":
    let shells = detectShells()
    for s in shells:
      check s.path.len > 0

  test "shell_is_marked_found":
    let shells = detectShells()
    for s in shells:
      check s.found == true

  test "shell_path_exists":
    let shells = detectShells()
    for s in shells:
      check fileExists(s.path)

suite "shell_detect: common shells":
  test "finds_common_shell":
    let shells = detectShells()
    let names = @["bash", "zsh", "sh", "fish"]
    var foundCommon = false
    for s in shells:
      if s.name in names:
        foundCommon = true
        break
    check foundCommon