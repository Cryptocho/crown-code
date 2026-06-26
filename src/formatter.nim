import pathutils

type
  FormatterError* {.pure.} = enum
    Success
    NullPath
    ReadFailed
    MemoryAlloc  ## 仅保留以匹配 C 接口（Nim GC 下不会触发）

  FormatterResult* = object
    error*: FormatterError
    errorMessage*: string

proc processContent*(content: string): string =
  ## 核心格式化逻辑，对应 C 的 process_content()
  ##
  ## 1. 行尾空白（空格/制表符）修剪
  ## 2. 行首空白规范化：
  ##    - 含制表符 → 替换为 4 个空格
  ##    - 只有空格 → 完全移除
  ## 3. 保留原有空行结构

  result = newStringOfCap(content.len)

  var i = 0
  let len = content.len

  while i < len:
    # 硬换行保持原样
    if content[i] == '\n':
      result.add('\n')
      i += 1
      continue

    # 定位行尾
    let lineStart = i
    while i < len and content[i] != '\n':
      i += 1
    let lineEnd = i  # 指向 \n（若存在）或 len

    # 行尾空白修剪
    var trimEnd = lineEnd
    while trimEnd > lineStart and
        (content[trimEnd - 1] == ' ' or content[trimEnd - 1] == '\t'):
      dec trimEnd

    # 扫描行首空白，检测是否存在制表符
    var wp = lineStart
    var hasLeadingTabs = false
    while wp < trimEnd and
        (content[wp] == ' ' or content[wp] == '\t'):
      if content[wp] == '\t':
        hasLeadingTabs = true
      inc wp

    # 输出规范化后的行首空白
    if hasLeadingTabs:
      result.add("    ")  # 4 空格
    # else: 只有空格 → 无输出（完全删除）

    # 输出行内剩余内容（wp ~ trimEnd）
    while wp < trimEnd:
      result.add(content[wp])
      inc wp

    # 若原行以 \n 结尾，输出之
    if i < len and content[i] == '\n':
      result.add('\n')
      inc i

proc formatFile*(path: string): FormatterResult =
  ## 格式化文件
  ##
  ## 流程（对应 C 的 format_file）：
  ## 1. 参数验证 → NullPath
  ## 2. 路径解析
  ## 3. 读文件
  ## 4. processContent 格式化
  ## 5. 写回文件
  ## 6. 返回结果

  # 1. 参数验证
  if path.len == 0:
    result.error = FormatterError.NullPath
    result.errorMessage = "Path parameter is required"
    return

  # 2. 路径解析
  let absolutePath = resolveWorkspacePath(path)
  if absolutePath.len == 0:
    result.error = FormatterError.ReadFailed
    result.errorMessage = "Could not resolve path"
    return

  # 3. 读文件
  var content: string
  try:
    content = readFile(absolutePath)
  except CatchableError as e:
    result.error = FormatterError.ReadFailed
    result.errorMessage = "Error reading file: " & e.msg
    return

  # 4. 格式化
  let processed = processContent(content)

  # 5. 写回
  try:
    writeFile(absolutePath, processed)
  except CatchableError as e:
    result.error = FormatterError.ReadFailed
    result.errorMessage = "Error writing file: " & e.msg
    return

  # 6. 成功
  result.error = FormatterError.Success