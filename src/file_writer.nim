import std/os
import pathutils
import ignore_rules
import file_reader

type
  FileWriterError* {.pure.} = enum
    Success
    NullPath
    FileNotFound
    PermissionDenied
    WriteFailed

  FileWriterResult* = object
    error*: FileWriterError
    errorMessage*: string

proc writeFileContent*(path: string; content: string = ""): FileWriterResult =
  ## 写入文件内容
  ## - path: 文件路径（相对或绝对）
  ## - content: 要写入的内容（默认空字符串）
  ##
  ## 流程（对应 C 的 file_write）：
  ## 1. 参数验证 → NullPath
  ## 2. clineignore 检查 → PermissionDenied
  ## 3. 路径解析 → FileNotFound
  ## 4. 文件写入 → WriteFailed
  ## 5. 缓存失效
  ## 6. 返回 Success

  # 1. 参数验证
  if path.len == 0:
    result.error = FileWriterError.NullPath
    result.errorMessage = "Path parameter is required"
    return

  # 2. clineignore 检查
  if checkIgnorePath(path):
    result.error = FileWriterError.PermissionDenied
    result.errorMessage = "Access denied by .clineignore rules"
    return

  # 3. 路径解析
  let absolutePath = resolveWorkspacePath(path)
  if absolutePath.len == 0:
    result.error = FileWriterError.FileNotFound
    result.errorMessage = "Could not resolve path"
    return

  # 4. 文件写入
  try:
    writeFile(absolutePath, content)
  except CatchableError as e:
    result.error = FileWriterError.WriteFailed
    result.errorMessage = "Error writing file: " & e.msg
    return

  # 5. 缓存失效
  cacheInvalidate(absolutePath)

  # 6. 返回成功
  result.error = FileWriterError.Success