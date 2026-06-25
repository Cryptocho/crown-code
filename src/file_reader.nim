import std/os
import std/times
import std/strutils
import pathutils
import ignore_rules

const CACHE_SIZE* = 256
const MAX_PATH_LENGTH* = 4096
const DEFAULT_MAX_LINES* = 1000

type
  FileReaderError* {.pure.} = enum
    Success
    NullPath
    FileNotFound
    PermissionDenied
    ReadFailed
    MemoryAlloc

  LineRange* = object
    startLine*: int
    endLine*: int
    totalLines*: int
    truncated*: int

  FileReaderResult* = object
    content*: string
    range*: LineRange
    error*: FileReaderError
    errorMessage*: string

  FileCacheEntry* = object
    key*: string
    readCount*: int
    mtime*: Time

var cacheTable*: array[CACHE_SIZE, FileCacheEntry]

proc cacheHash(path: string): int =
  ## 计算路径的哈希值（大小写不敏感）
  var h: uint = 0
  for c in path:
    h = h * 31'u + ord(c.toLowerAscii()).uint
  result = (h mod CACHE_SIZE.uint).int

proc cacheGet*(absolutePath: string): var FileCacheEntry =
  ## 在缓存中查找路径对应的缓存项
  ## 返回缓存项的引用，调用方通过 readCount == 0 / key.len == 0 判断未命中
  let idx = cacheHash(absolutePath)
  if cacheTable[idx].readCount > 0 and cacheTable[idx].key == absolutePath:
    return cacheTable[idx]
  # 未命中，返回该槽位（不做任何修改）
  return cacheTable[idx]

proc cacheSet*(absolutePath: string, mtime: Time, readCount: int) =
  ## 将文件路径及元信息写入缓存槽位
  let idx = cacheHash(absolutePath)
  cacheTable[idx].key = absolutePath
  cacheTable[idx].readCount = readCount
  cacheTable[idx].mtime = mtime

proc cacheInvalidate*(absolutePath: string) =
  ## 使指定路径的缓存失效（清空 readCount）
  let idx = cacheHash(absolutePath)
  if cacheTable[idx].key == absolutePath:
    cacheTable[idx].readCount = 0

proc getFileMtime*(path: string): Time =
  ## 获取文件的最后修改时间
  ## 如果文件不存在，返回 epoch 时间（0）
  try:
    result = getLastModificationTime(path)
  except:
    result = fromUnix(0)

proc countLines*(content: string): int =
  ## 统计字符串中的换行符数量
  for c in content:
    if c == '\n':
      inc result

proc parseLineRange*(requestedStart, requestedEnd: int): LineRange =
  ## 解析行范围参数
  ## - 如果 requestedStart <= 0，从第 1 行开始
  ## - 如果 requestedEnd <= 0，默认读取 DEFAULT_MAX_LINES 行
  ## - 如果 requestedEnd > 0 且 endLine < startLine，自动交换
  result.startLine = if requestedStart > 0: requestedStart else: 1
  if requestedEnd > 0:
    result.endLine = requestedEnd
  else:
    result.endLine = result.startLine + DEFAULT_MAX_LINES - 1
  if requestedEnd > 0 and result.endLine < result.startLine:
    swap(result.startLine, result.endLine)
  result.truncated = 0

proc formatContentWithLineNumbers*(content: string, range: LineRange, totalLines: int): string =
  ## 格式化文件内容为带行号的输出
  ## 格式："{lineNum} | {content}"
  ## 尾部添加文件统计信息
  var
    pos = 0
    currentLine = 1
    startPos = 0
    endPos = content.len

  # 定位到起始行
  while currentLine < range.startLine and pos < content.len:
    if content[pos] == '\n':
      inc currentLine
    inc pos
  startPos = pos

  # 定位到结束行
  while currentLine <= range.endLine and pos < content.len:
    if content[pos] == '\n':
      inc currentLine
    inc pos
  endPos = pos

  # 去掉末尾换行
  if endPos > startPos and endPos > 0 and content[endPos - 1] == '\n':
    dec endPos

  result = newStringOfCap(endPos - startPos + (range.endLine - range.startLine + 1) * 15 + 200)

  var outLineNum = range.startLine
  var outPos = startPos

  while outPos < endPos:
    result.add($outLineNum)
    result.add(" | ")
    while outPos < endPos and content[outPos] != '\n':
      result.add(content[outPos])
      inc outPos
    if outPos < endPos and content[outPos] == '\n':
      result.add('\n')
      inc outPos
    inc outLineNum

  result.add("\n\n")
  if range.endLine < totalLines:
    result.add("(Showing lines ")
    result.add($range.startLine)
    result.add("-")
    result.add($range.endLine)
    result.add(" of ")
    result.add($totalLines)
    result.add(" total. Use start_line=")
    result.add($(range.endLine + 1))
    result.add(" to continue reading.)")
  else:
    result.add("(File has ")
    result.add($totalLines)
    result.add(" lines total.)")

proc readFileRange*(path: string; startLine, endLine: int): FileReaderResult =
  ## 读取文件的指定行范围，返回带行号格式化的内容
  ## - path: 文件路径（相对或绝对）
  ## - startLine: 起始行号（从 1 开始，<=0 表示第 1 行）
  ## - endLine: 结束行号（<=0 表示 DEFAULT_MAX_LINES 行）
  ##
  ## 流程（对应 C 的 file_read）：
  ## 1. 参数验证 → ERROR_NULL_PATH
  ## 2. clineignore 检查 → ERROR_PERMISSION_DENIED
  ## 3. 路径解析 → ERROR_FILE_NOT_FOUND
  ## 4. 缓存查找 + mtime 检测
  ## 5. 重复读取警告
  ## 6. 文件读取 → ERROR_READ_FAILED
  ## 7. 行统计 + 范围解析
  ## 8. 格式化输出
  ## 9. 拼接警告 + 返回结果

  # 1. 参数验证
  if path.len == 0:
    result.error = FileReaderError.NullPath
    result.errorMessage = "Path parameter is required"
    return

  # 2. clineignore 检查
  if checkIgnorePath(path):
    result.error = FileReaderError.PermissionDenied
    result.errorMessage = "Access denied by .clineignore rules"
    return

  # 3. 路径解析
  let absolutePath = resolveWorkspacePath(path)
  if absolutePath.len == 0:
    result.error = FileReaderError.FileNotFound
    result.errorMessage = "Could not resolve path"
    return

  # 4. 缓存查找 + mtime 检测
  var cached = addr(cacheGet(absolutePath))
  if cached.readCount > 0:
    let currentMtime = getFileMtime(absolutePath)
    if currentMtime != cached.mtime:
      cached.readCount = 0

  cached = addr(cacheGet(absolutePath))

  # 5. 重复读取警告
  var dupWarning = ""
  if cached.readCount > 0:
    inc cached.readCount
    if cached.readCount >= 3:
      dupWarning = "[DUPLICATE READ] You have already read '" & path & "' " & $cached.readCount & " times in this conversation. The content has not changed since your last read. Please use the information you already have and proceed with your task.\n\n"
    elif cached.readCount == 2:
      dupWarning = "[File already read] The file '" & path & "' was already read earlier in this conversation. Returning content:\n\n"

  # 6. 文件读取
  var content: string
  try:
    content = readFile(absolutePath)
  except CatchableError:
    if not fileExists(absolutePath):
      result.error = FileReaderError.ReadFailed
      result.errorMessage = "Error reading file: File not found"
    elif getFilePermissions(absolutePath) == {}:
      result.error = FileReaderError.ReadFailed
      result.errorMessage = "Error reading file: Permission denied"
    else:
      result.error = FileReaderError.ReadFailed
      result.errorMessage = "Error reading file: Read failed"
    return

  # 7. 行统计 + 范围解析
  let totalLines = countLines(content)
  var range = parseLineRange(startLine, endLine)
  range.totalLines = totalLines

  # 边界裁剪
  if range.startLine > totalLines:
    range.startLine = totalLines
  if range.endLine > totalLines:
    range.endLine = totalLines

  # 写入缓存（首次读取）
  if cached.readCount == 0:
    let mtime = getFileMtime(absolutePath)
    cacheSet(absolutePath, mtime, 1)

  # 8. 格式化输出
  let formattedContent = formatContentWithLineNumbers(content, range, totalLines)

  # 9. 拼接警告 + 返回结果
  if dupWarning.len > 0:
    result.content = dupWarning & formattedContent
  else:
    result.content = formattedContent
  result.range = range
  result.error = FileReaderError.Success