import std/os
import std/algorithm
import pathutils
import ignore_rules

const MAX_LIST_ENTRIES* = 200

type
  ListFilesError* {.pure.} = enum
    Success
    NullPath
    DirNotFound
    PermissionDenied
    ReadFailed

  ListFilesResult* = object
    entries*: seq[string]
    count*: int
    didHitLimit*: bool
    error*: ListFilesError
    errorMessage*: string

proc cmpEntry(a, b: tuple[name: string, isDir: bool]): int =
  ## 排序比较器：目录优先，同组按字母序（区分大小写）
  if a.isDir and not b.isDir:
    return -1
  if not a.isDir and b.isDir:
    return 1
  result = cmp(a.name, b.name)

proc listFiles*(path: string): ListFilesResult =
  ## 列出目录内容
  ## 返回排序后的条目列表，目录优先，字母序排列
  ## 安全限制：/ 和 $HOME 返回空结果（非错误）
  ## 上限：MAX_LIST_ENTRIES = 200 条
  if path.len == 0:
    return ListFilesResult(error: ListFilesError.NullPath, errorMessage: "Path is empty")

  # clineignore 检查
  if checkIgnorePath(path):
    return ListFilesResult(
      error: ListFilesError.PermissionDenied,
      errorMessage: "Access denied by .clineignore rules"
    )

  let absPath = resolveWorkspacePath(path)
  if absPath.len == 0:
    return ListFilesResult(error: ListFilesError.DirNotFound, errorMessage: "Path not found")

  # 安全限制：阻止列出根目录和 HOME 目录
  let homeDir = getHomeDir()
  if absPath == "/" or absPath == homeDir:
    return ListFilesResult()

  # 检查目录是否存在
  if not dirExists(absPath):
    return ListFilesResult(
      error: ListFilesError.DirNotFound,
      errorMessage: "Directory not found: " & absPath
    )

  # 目录遍历
  var entries: seq[tuple[name: string, isDir: bool]] = @[]
  var didHitLimit = false

  try:
    for kind, relPath in walkDir(absPath, relative = true):
      if relPath == "." or relPath == "..":
        continue

      let entryAbsPath = absPath / relPath
      if checkIgnorePath(entryAbsPath):
        continue

      entries.add((name: relPath, isDir: kind == pcDir))

      if entries.len > MAX_LIST_ENTRIES:
        didHitLimit = true
        entries.setLen(MAX_LIST_ENTRIES)
        break
  except CatchableError as e:
    return ListFilesResult(
      error: ListFilesError.ReadFailed,
      errorMessage: "Failed to read directory: " & e.msg
    )

  # 排序
  sort(entries, cmpEntry)

  # 构建结果
  var resultEntries: seq[string] = @[]
  for e in entries:
    resultEntries.add(e.name)

  result = ListFilesResult(
    entries: resultEntries,
    count: resultEntries.len,
    didHitLimit: didHitLimit
  )