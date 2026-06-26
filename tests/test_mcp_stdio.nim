import unittest
import mcp/transport_stdio

template skipIfUnavailable(t: StdioTransport) =
  if t.isNil or t.readFd < 0:
    skip()

suite "StdioTransport":
  test "empty command returns error":
    let t = startStdioTransport("")
    check t.lastError.len > 0
    check t.readFd == -1

  test "process starts successfully":
    let t = startStdioTransport("true")
    skipIfUnavailable(t)
    check t.readFd >= 0
    check t.writeFd >= 0
    check t.childPid > 0
    close(t)

  test "read timeout returns teTimeout":
    let t = startStdioTransport("cat")
    skipIfUnavailable(t)
    # no data available, should timeout
    let r = readJsonLine(t, timeoutMs = 50)
    check r.error == teTimeout
    close(t)

  test "close nil does not crash":
    var t: StdioTransport = nil
    close(t)

  test "close cleans up fd resources":
    let t = startStdioTransport("true")
    skipIfUnavailable(t)
    close(t)
    check t.readFd == -1
    check t.writeFd == -1
    check t.stderrFd == -1

  test "process termination (sleep)":
    let t = startStdioTransport("sleep", @["10"])
    skipIfUnavailable(t)
    close(t)
    check t.childPid == -1