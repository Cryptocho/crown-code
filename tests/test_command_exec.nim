import unittest
import std/[strutils, os]
import command_exec

suite "command_exec: trimWhitespace":
  test "trim_leading_spaces":
    check trimWhitespace("  hello") == "hello"

  test "trim_trailing_spaces":
    check trimWhitespace("hello  ") == "hello"

  test "trim_both_ends":
    check trimWhitespace("  hello world  ") == "hello world"

  test "trim_leading_tab":
    check trimWhitespace("\thello") == "hello"

  test "empty_string":
    check trimWhitespace("") == ""

  test "whitespace_only":
    check trimWhitespace("   ") == ""

suite "command_exec: splitCommands":
  test "single_command":
    let cmds = splitCommands("echo hello")
    check cmds.len == 1
    check cmds[0] == "echo hello"

  test "split_by_double_ampersand":
    let cmds = splitCommands("echo a && echo b")
    check cmds.len == 2
    check cmds[0] == "echo a "
    check cmds[1] == " echo b"

  test "split_by_double_pipe":
    let cmds = splitCommands("false || echo fallback")
    check cmds.len == 2
    check cmds[0] == "false "
    check cmds[1] == " echo fallback"

  test "split_by_semicolon":
    let cmds = splitCommands("echo a; echo b")
    check cmds.len == 2
    check cmds[0] == "echo a"
    check cmds[1] == " echo b"

  test "split_by_pipe":
    let cmds = splitCommands("echo hello | grep hello")
    check cmds.len == 2
    check cmds[0] == "echo hello "
    check cmds[1] == " grep hello"

  test "split_by_single_ampersand":
    let cmds = splitCommands("cmd1 & cmd2")
    check cmds.len == 2
    check cmds[0] == "cmd1 "
    check cmds[1] == " cmd2"

  test "empty_command":
    let cmds = splitCommands("")
    check cmds.len == 0

  test "no_separator":
    let cmds = splitCommands("singlecommand")
    check cmds.len == 1
    check cmds[0] == "singlecommand"

suite "command_exec: requiresApproval":
  test "always_returns_true_for_now":
    check requiresApproval("any command") == true
    check requiresApproval("") == true

suite "command_exec: CircularBuffer":
  test "push_and_join":
    var cb: CircularBuffer
    initCircularBuffer(cb)
    pushCircularBuffer(cb, "line1")
    pushCircularBuffer(cb, "line2")
    pushCircularBuffer(cb, "line3")
    let result = joinCircularBuffer(cb)
    check result == "line1line2line3"

  test "empty_buffer":
    var cb: CircularBuffer
    initCircularBuffer(cb)
    let result = joinCircularBuffer(cb)
    check result == ""

  test "single_element":
    var cb: CircularBuffer
    initCircularBuffer(cb)
    pushCircularBuffer(cb, "only")
    let result = joinCircularBuffer(cb)
    check result == "only"

  test "overflow_keeps_newest":
    var cb: CircularBuffer
    initCircularBuffer(cb)
    for i in 0 ..< CircularBufferSize + 10:
      pushCircularBuffer(cb, "line" & $i)
    let result = joinCircularBuffer(cb)
    # 验证包含最后写入的行
    check "line" & $(CircularBufferSize + 9) in result
    # 验证不包含最早的行
    check "line0" notin result

suite "command_exec: execCommand":
  test "echo_command_returns_output":
    let result = execCommand("echo hello world")
    check result.error == ceOk
    check result.exitCode == 0
    check result.stdout.strip == "hello world"

  test "exit_code_nonzero":
    let result = execCommand("bash -c 'exit 42'")
    check result.error == ceExecutionFailed
    check result.exitCode == 42

  test "stderr_captured":
    let result = execCommand("bash -c 'echo error >&2'")
    check result.stderr.strip == "error"

  test "empty_command_returns_error":
    let result = execCommand("")
    check result.error == ceExecutionFailed

  test "whitespace_command_returns_error":
    let result = execCommand("   ")
    check result.error == ceExecutionFailed

  test "execution_time_is_measured":
    let result = execCommand("echo quick")
    check result.executionTime > 0.0

  test "blacklist_approval_check":
    # 当前 requiresApproval 总是返回 true，所以黑名单匹配不会导致拒绝
    let result = execCommand("echo safe", blacklist = ["echo"])
    # 不会 ceApprovalDenied（因为 requiresApproval 返回 true）
    check result.error == ceOk

  test "command_not_found_handled":
    let result = execCommand("nonexistentcommand12345xyz")
    # 命令不存在时 exitCode 应为 127 或非零
    check result.exitCode != 0