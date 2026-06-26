import pathutils
import ignore_rules
import file_writer

type
  FileEditError* {.pure.} = enum
    ## 文件编辑操作的结果状态码
    Success
    FileNotFound
    OldStringNotFound
    MultipleMatches
    ReadFailed
    WriteFailed

  FileEditResult* = object
    ## 文件编辑操作的结果
    error*: FileEditError
    errorMessage*: string
    matchCount*: int

proc splitIntoLines(content: string): seq[string] =
  ## 将字符串按 \n 拆分成行
  ## 与 C 的 split_into_lines 行为一致：
  ## - 每行去掉换行符
  ## - 末尾 \n 会产生一个空串行（如 "abc\n" → ["abc", ""]）
  result = newSeq[string]()
  var start = 0
  for i, c in content:
    if c == '\n':
      result.add(content[start ..< i])
      start = i + 1
  # 处理最后一段（包括文件末尾没有 \n 的最后一段）
  result.add(content[start ..< content.len])

proc joinLines(lines: seq[string]): string =
  ## 将行序列用 \n 连接
  ## 与 C 的 join_lines 行为一致：
  ## - 行间插入 \n
  ## - 末尾不加 \n
  if lines.len == 0:
    return ""
  result = lines[0]
  for i in 1 ..< lines.len:
    result.add('\n')
    result.add(lines[i])

proc editFile*(path: string; oldStr, newStr: string; multiple: bool = false): FileEditResult =
  ## 文件行级精确替换
  ## - path: 文件路径（相对或绝对）
  ## - oldStr: 要查找的整行内容（精确匹配整行）
  ## - newStr: 替换后的整行内容（按原样替换）
  ## - multiple: 是否替换所有匹配行（false 时只替换第一处）
  ##
  ## 流程（对应 C 的 file_edit）：
  ## 1. 路径解析 → FileNotFound
  ## 2. clineignore 检查 → ReadFailed
  ## 3. 读取文件 → ReadFailed
  ## 4. 按行拆分
  ## 5. 精确匹配计数
  ## 6. 未找到 → OldStringNotFound
  ## 7. 多次匹配（!multiple）→ MultipleMatches
  ## 8. 替换行内容
  ## 9. 按行合并
  ## 10. 写入文件 → 转换 FileWriterError
  ## 11. 返回 Success

  # 1. 路径解析
  let absolutePath = resolveWorkspacePath(path)
  if absolutePath.len == 0:
    result.error = FileEditError.FileNotFound
    result.errorMessage = "Could not resolve path"
    return

  # 2. clineignore 检查
  if checkIgnorePath(path):
    result.error = FileEditError.ReadFailed
    result.errorMessage = "Access denied by .clineignore rules"
    return

  # 3. 读取文件
  var content: string
  try:
    content = readFile(absolutePath)
  except CatchableError as e:
    result.error = FileEditError.ReadFailed
    result.errorMessage = "Error reading file: " & e.msg
    return

  # 4. 按行拆分
  var lines = splitIntoLines(content)

  # 5. 精确匹配计数
  result.matchCount = 0
  for line in lines:
    if line == oldStr:
      inc result.matchCount

  # 6. 未找到 oldStr
  if result.matchCount == 0:
    result.error = FileEditError.OldStringNotFound
    result.errorMessage = "Could not find exact match for oldStr in file"
    return

  # 7. 多次匹配但 multiple=false
  if not multiple and result.matchCount > 1:
    result.error = FileEditError.MultipleMatches
    result.errorMessage = "Found multiple matches (" & $result.matchCount &
                          "), but multiple is false"
    return

  # 8. 替换匹配行
  var replaced = 0
  for i, line in lines:
    if line == oldStr:
      lines[i] = newStr
      inc replaced
      if not multiple:
        break

  # 9. 按行合并
  let newContent = joinLines(lines)

  # 10. 写入文件
  let writeResult = writeFileContent(path, newContent)
  case writeResult.error
  of FileWriterError.Success:
    result.error = FileEditError.Success
  of FileWriterError.WriteFailed:
    result.error = FileEditError.WriteFailed
    result.errorMessage = writeResult.errorMessage
  else:
    result.error = FileEditError.ReadFailed
    result.errorMessage = writeResult.errorMessage