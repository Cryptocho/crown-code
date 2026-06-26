## 命令执行模块
##
## 对应 C 源码：temp/src/tools.c `execute_command()`, `split_commands()`,
## `trim_whitespace()`, `CircularBuffer`
##
## 提供安全的命令执行能力：命令拆分、审批检查、shell 子进程启动、
## stdout/stderr 流式捕获、超时终止、输出截断。

import std/[os, osproc, strutils, times, locks, streams]
import shell_detect

type
  CommandError* = enum
    ceOk = 0
    ceApprovalDenied
    ceExecutionFailed
    ceTimeout

  CommandResult* = object
    stdout*: string
    stderr*: string
    exitCode*: int
    executionTime*: float
    abnormalExit*: bool
    error*: CommandError

const
  MaxFullOutputSize* = 1024 * 1024   ## 1MB 最大输出
  DefaultTimeoutMs* = 300_000        ## 300 秒超时（毫秒）
  CircularBufferSize* = 2000          ## 环形缓冲区槽数

# ---------------------------------------------------------------------------
# CircularBuffer — 行级环形缓冲区
# ---------------------------------------------------------------------------

type
  CircularBuffer* = object
    lines: array[CircularBufferSize, string]
    head: int
    count: int
    lock: Lock

proc initCircularBuffer*(cb: var CircularBuffer) =
  ## 初始化环形缓冲区
  cb.head = 0
  cb.count = 0
  for i in 0 ..< CircularBufferSize:
    cb.lines[i] = ""
  initLock(cb.lock)

proc pushCircularBuffer*(cb: var CircularBuffer, line: string) =
  ## 向环形缓冲区追加一行。缓冲区满时覆盖最旧的行。
  acquire(cb.lock)
  cb.lines[cb.head] = line
  cb.head = (cb.head + 1) mod CircularBufferSize
  if cb.count < CircularBufferSize:
    cb.count += 1
  release(cb.lock)

proc joinCircularBuffer*(cb: var CircularBuffer): string =
  ## 将环形缓冲区中所有行拼接为单个字符串，行间无换行符。
  ## （与 C 代码行为一致：由调用方处理换行符）
  acquire(cb.lock)
  var total = 0
  for i in 0 ..< cb.count:
    let idx = (cb.head - cb.count + i + CircularBufferSize) mod CircularBufferSize
    total += cb.lines[idx].len
  result = newString(total)
  var pos = 0
  for i in 0 ..< cb.count:
    let idx = (cb.head - cb.count + i + CircularBufferSize) mod CircularBufferSize
    if cb.lines[idx].len > 0:
      copyMem(result[pos].addr, cb.lines[idx].cstring, cb.lines[idx].len)
      pos += cb.lines[idx].len
  release(cb.lock)

# ---------------------------------------------------------------------------
# 命令字符串处理
# ---------------------------------------------------------------------------

proc trimWhitespace*(s: string): string =
  ## 去除字符串两端的空白字符（空格、制表符）
  result = s.strip(chars = {' ', '\t'})

proc splitCommands*(command: string): seq[string] =
  ## 将命令字符串按分隔符拆分为子命令序列。
  ##
  ## 支持的分隔符（优先级）：
  ## - `&&` 逻辑与
  ## - `||` 逻辑或
  ## - `&`  后台执行（仅语法识别）
  ## - `|`  管道
  ## - `;`  顺序执行
  ##
  ## 与 C 代码 `split_commands()` 逻辑一致。
  if command.len == 0:
    return

  result = @[]
  var start = 0
  var i = 0
  let cmd = command

  while i < cmd.len:
    var sepLen = 0

    if i + 1 < cmd.len:
      if cmd[i] == '&' and cmd[i+1] == '&':
        sepLen = 2
      elif cmd[i] == '&' and cmd[i+1] == '|':
        sepLen = 2
      elif cmd[i] == '|' and cmd[i+1] == '|':
        sepLen = 2
    if sepLen == 0:
      if cmd[i] == '&':
        sepLen = 1
      elif cmd[i] == '|':
        sepLen = 1
      elif cmd[i] == ';':
        sepLen = 1

    if sepLen > 0:
      let tokenLen = i - start
      if tokenLen > 0:
        result.add(cmd[start ..< i])
      i += sepLen
      start = i
    else:
      i += 1

  let tokenLen = i - start
  if tokenLen > 0:
    result.add(cmd[start ..< i])

proc requiresApproval*(command: string): bool =
  ## 检查命令是否需要用户审批。
  ## 当前实现与 C 代码相同：始终返回 true。
  ## TODO: 通过配置确定审批策略
  discard command
  return true

# ---------------------------------------------------------------------------
# 输出流读取线程
# ---------------------------------------------------------------------------

type
  StreamReaderArgs = object
    stream: Stream
    buffer: ptr CircularBuffer
    totalSize: ptr int

proc streamReaderThread(args: StreamReaderArgs) {.thread.} =
  ## 线程函数：从流中逐行读取数据，写入环形缓冲区。
  ## 达到最大输出限制后丢弃后续行。
  if args.stream == nil:
    return
  try:
    var line = ""
    while args.stream.readLine(line):
      if args.totalSize[] < MaxFullOutputSize:
        pushCircularBuffer(args.buffer[], line)
        args.totalSize[] += line.len + 1
      else:
        # 达到输出限制，丢弃后续内容
        discard
      line = ""
  except IOError:
    # 流正常关闭
    discard
  except:
    discard

# ---------------------------------------------------------------------------
# 核心执行
# ---------------------------------------------------------------------------

proc execCommand*(command: string, blacklist: openArray[string] = []): CommandResult =
  ## 执行命令并捕获输出。
  ##
  ## 流程：
  ## 1. 去除命令两端空白
  ## 2. 拆分为子命令序列
  ## 3. 检查黑名单（匹配则要求审批）
  ## 4. 检测可用 shell
  ## 5. 启动 shell 子进程
  ## 6. 双线程流式捕获 stdout/stderr
  ## 7. 超时终止
  ## 8. 返回执行结果
  result = CommandResult(
    exitCode: -1,
    error: ceOk
  )
  result.executionTime = 0.0

  let trimmed = trimWhitespace(command)
  if trimmed.len == 0:
    result.error = ceExecutionFailed
    return

  # 检查黑名单
  let subCommands = splitCommands(trimmed)
  for sub in subCommands:
    let subTrimmed = trimWhitespace(sub)
    if subTrimmed.len == 0:
      continue
    for blocked in blacklist:
      if subTrimmed == blocked:
        if not requiresApproval(subTrimmed):
          result.error = ceApprovalDenied
          return
        break

  # 检测 shell
  let shells = detectShells()
  if shells.len == 0 or not shells[0].found:
    result.error = ceExecutionFailed
    return

  let shellPath = shells[0].path
  let startTime = cpuTime()

  when defined(windows):
    # Windows: cmd.exe /c command
    let process = startProcess(
      shellPath,
      args = ["/c", command],
      options = {poUsePath}
    )
  else:
    # POSIX: shell -l -c "command"（login shell 模式，与 C 代码一致）
    let process = startProcess(
      shellPath,
      args = ["-l", "-c", command],
      options = {poUsePath}
    )

  # 准备输出缓冲区
  var outBuffer: CircularBuffer
  var errBuffer: CircularBuffer
  initCircularBuffer(outBuffer)
  initCircularBuffer(errBuffer)
  var totalOutSize: int = 0
  var totalErrSize: int = 0

  # 启动读取线程
  var outThread: Thread[StreamReaderArgs]
  var errThread: Thread[StreamReaderArgs]

  let outArgs = StreamReaderArgs(
    stream: process.outputStream,
    buffer: addr(outBuffer),
    totalSize: addr(totalOutSize)
  )
  let errArgs = StreamReaderArgs(
    stream: process.errorStream,
    buffer: addr(errBuffer),
    totalSize: addr(totalErrSize)
  )

  createThread(outThread, streamReaderThread, outArgs)
  createThread(errThread, streamReaderThread, errArgs)

  # 等待进程结束（带超时）
  # waitForExit 返回 -1 表示超时，否则返回退出码
  let timeoutMs = DefaultTimeoutMs
  let exitResult = process.waitForExit(timeoutMs)

  if exitResult == -1:
    # 超时：终止进程
    result.error = ceTimeout
    result.abnormalExit = true
    process.terminate()
    # 给进程一点时间退出
    os.sleep(100)
    if process.running:
      process.kill()
    # 等待读取线程结束
    outThread.joinThread()
    errThread.joinThread()
  else:
    # 等待读取线程完成剩余数据
    outThread.joinThread()
    errThread.joinThread()
    result.exitCode = exitResult

  result.executionTime = cpuTime() - startTime

  # 拼接输出
  result.stdout = joinCircularBuffer(outBuffer)
  result.stderr = joinCircularBuffer(errBuffer)

  process.close()

  # 错误状态判断
  if result.error == ceOk and result.exitCode != 0:
    result.error = ceExecutionFailed

  deinitLock(outBuffer.lock)
  deinitLock(errBuffer.lock)