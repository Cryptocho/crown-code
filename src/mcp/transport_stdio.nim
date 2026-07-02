## MCP stdio 传输层
##
## 对应 C 源码：temp/src/mcp.c 中 internal_io_spawn_child(), internal_io_read_line(),
## internal_io_write_line(), internal_io_kill_child(), internal_io_close()
##
## 手动 fork/exec/pipe 管理子进程，使用 posix 原生 I/O 实现超时感知的行读写。
## 不使用 std/osproc.startProcess()，保证文件描述符完全所有权。

import std/[os, strutils, monotimes, locks]
import std/times except Time
import std/posix
import ../command_exec

export command_exec.CircularBuffer

const
  MCP_LINE_BUF_SIZE* = 1_048_576
  DEFAULT_LINE_TIMEOUT_MS* = 30_000

type
  TransportError* = enum
    teOk
    teTimeout
    teWriteError
    teReadError
    teEof
    teSpawnFailed

  StdioTransport* = ref object
    readFd*: cint
    writeFd*: cint
    stderrFd*: cint
    childPid*: Pid
    stderrThread: Thread[ptr StdioTransport]
    stderrRunning: bool
    stderrBuf*: CircularBuffer
    lastError*: string

proc stderrReader(args: ptr StdioTransport) {.thread.} =
  let t = args[]
  var buf: array[1024, char]
  while true:
    let n = posix.read(t.stderrFd, addr(buf), sizeof(buf))
    if n <= 0: break
    var chunk = newString(n)
    copyMem(chunk.cstring, addr(buf), n)
    for line in chunk.splitLines():
      if line.len > 0:
        pushCircularBuffer(t.stderrBuf, line)

proc startStdioTransport*(command: string, args: openArray[string] = []): StdioTransport {.raises: [].} =
  if command.len == 0:
    return StdioTransport(readFd: -1, writeFd: -1, stderrFd: -1, childPid: -1, lastError: "command is empty")

  var stdinPipe, stdoutPipe, stderrPipe: array[2, cint]
  if posix.pipe(stdinPipe) != 0:
    return StdioTransport(readFd: -1, writeFd: -1, stderrFd: -1, childPid: -1,
                          lastError: "pipe(stdin) failed: " & $strerror(errno))
  if posix.pipe(stdoutPipe) != 0:
    discard posix.close(stdinPipe[0]); discard posix.close(stdinPipe[1])
    return StdioTransport(readFd: -1, writeFd: -1, stderrFd: -1, childPid: -1,
                          lastError: "pipe(stdout) failed: " & $strerror(errno))
  if posix.pipe(stderrPipe) != 0:
    discard posix.close(stdinPipe[0]); discard posix.close(stdinPipe[1])
    discard posix.close(stdoutPipe[0]); discard posix.close(stdoutPipe[1])
    return StdioTransport(readFd: -1, writeFd: -1, stderrFd: -1, childPid: -1,
                          lastError: "pipe(stderr) failed: " & $strerror(errno))

  let pid = posix.fork()
  if pid == -1:
    discard posix.close(stdinPipe[0]); discard posix.close(stdinPipe[1])
    discard posix.close(stdoutPipe[0]); discard posix.close(stdoutPipe[1])
    discard posix.close(stderrPipe[0]); discard posix.close(stderrPipe[1])
    return StdioTransport(readFd: -1, writeFd: -1, stderrFd: -1, childPid: -1,
                          lastError: "fork() failed: " & $strerror(errno))

  if pid == 0:
    discard posix.close(stdinPipe[1])
    discard posix.close(stdoutPipe[0])
    discard posix.close(stderrPipe[0])
    discard posix.dup2(stdinPipe[0], posix.STDIN_FILENO)
    discard posix.dup2(stdoutPipe[1], posix.STDOUT_FILENO)
    discard posix.dup2(stderrPipe[1], posix.STDERR_FILENO)
    discard posix.close(stdinPipe[0]); discard posix.close(stdoutPipe[1]); discard posix.close(stderrPipe[1])
    let c = command.cstring
    case args.len
    of 0: discard posix.execlp(c, c, nil)
    of 1: discard posix.execlp(c, c, args[0].cstring, nil)
    of 2: discard posix.execlp(c, c, args[0].cstring, args[1].cstring, nil)
    of 3: discard posix.execlp(c, c, args[0].cstring, args[1].cstring, args[2].cstring, nil)
    else: discard posix.execlp(c, c, args[0].cstring, args[1].cstring, args[2].cstring, args[3].cstring, nil)
    quit(127)

  discard posix.close(stdinPipe[0]); discard posix.close(stdoutPipe[1]); discard posix.close(stderrPipe[1])
  var buf: CircularBuffer
  initCircularBuffer(buf)
  result = StdioTransport(readFd: stdoutPipe[0], writeFd: stdinPipe[1], stderrFd: stderrPipe[0],
                          childPid: pid, stderrBuf: buf, lastError: "")
  result.stderrRunning = true
  try:
    createThread(result.stderrThread, stderrReader, addr(result))
  except ResourceExhaustedError:
    result.lastError = "failed to create stderr thread: " & getCurrentExceptionMsg()
    discard posix.close(result.readFd); discard posix.close(result.writeFd); discard posix.close(result.stderrFd)
    result.readFd = -1; result.writeFd = -1; result.stderrFd = -1
    result = nil

proc remainingMs(deadline: MonoTime): int64 =
  inMilliseconds(deadline - getMonoTime())

type
  ReadLineResult* = object
    line*: string
    error*: TransportError

proc readJsonLine*(t: StdioTransport, timeoutMs: int = DEFAULT_LINE_TIMEOUT_MS): ReadLineResult {.raises: [].} =
  if t.readFd < 0: return ReadLineResult(error: teReadError)
  var lineBuf = newStringOfCap(MCP_LINE_BUF_SIZE)
  let deadline = getMonoTime() + initDuration(milliseconds = timeoutMs)
  while lineBuf.len < MCP_LINE_BUF_SIZE:
    let remMs = remainingMs(deadline)
    if remMs <= 0: return ReadLineResult(error: teTimeout)
    var readfds: TFDSet
    FD_ZERO(readfds); FD_SET(t.readFd, readfds)
    var tv: Timeval
    tv.tv_sec = cast[Time](clong(remMs div 1000))
    tv.tv_usec = ((remMs mod 1000) * 1000).clong
    let selRet = posix.select(t.readFd.cint + 1, addr(readfds), nil, nil, addr(tv))
    if selRet < 0:
      if errno == EINTR: continue
      return ReadLineResult(error: teReadError)
    if selRet == 0: return ReadLineResult(error: teTimeout)
    var ch: char
    let n = posix.read(t.readFd, addr(ch), 1)
    if n < 0:
      if errno == EINTR: continue
      return ReadLineResult(error: teReadError)
    if n == 0:
      if lineBuf.len == 0: return ReadLineResult(error: teEof)
      return ReadLineResult(line: lineBuf, error: teOk)
    if ch == '\r': continue
    if ch == '\n': return ReadLineResult(line: lineBuf, error: teOk)
    lineBuf.add(ch)
  return ReadLineResult(line: lineBuf, error: teOk)

proc writeJsonLine*(t: StdioTransport, line: string, timeoutMs: int = DEFAULT_LINE_TIMEOUT_MS): TransportError {.raises: [].} =
  if t.writeFd < 0: return teWriteError
  let payload = line & "\n"
  let deadline = getMonoTime() + initDuration(milliseconds = timeoutMs)
  let totalLen = payload.len
  if remainingMs(deadline) <= 0: return teTimeout
  let n = posix.write(t.writeFd, payload.cstring, totalLen)
  if n < 0:
    if errno == EINTR:
      # retry once
      if remainingMs(deadline) <= 0: return teTimeout
      discard posix.write(t.writeFd, payload.cstring, totalLen)
    else:
      return teWriteError
  return teOk

proc close*(t: StdioTransport) {.raises: [].} =
  if t.isNil: return
  if t.childPid > 0:
    discard posix.kill(t.childPid, posix.SIGTERM)
    var status: cint = 0
    var waited = 0
    while waited < 50:
      if posix.waitpid(t.childPid, status, posix.WNOHANG) == t.childPid:
        t.childPid = -1; break
      if errno != EINTR: discard posix.kill(t.childPid, 0)
      os.sleep(100); waited.inc
    if t.childPid > 0:
      discard posix.kill(t.childPid, posix.SIGKILL)
      var status2: cint = 0
      discard posix.waitpid(t.childPid, status2, 0.cint)
      t.childPid = -1
  if t.writeFd >= 0: discard posix.close(t.writeFd); t.writeFd = -1
  if t.readFd >= 0: discard posix.close(t.readFd); t.readFd = -1
  if t.stderrFd >= 0:
    t.stderrRunning = false
    discard posix.close(t.stderrFd); t.stderrFd = -1
  if t.stderrThread.running: t.stderrThread.joinThread()

proc getStderr*(t: StdioTransport): string =
  if t.isNil: return ""
  joinCircularBuffer(t.stderrBuf)