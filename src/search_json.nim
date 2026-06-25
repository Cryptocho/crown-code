import context, search

func jsonEscape*(s: string): string =
  ## 将字符串转义为 JSON 字符串字面量，用双引号包裹。
  ## 转义字符: " → \", \ → \\, \n → \\n, \t → \\t, \r → \\r。
  ## 空字符串输出 ""。
  result = "\""
  for c in s:
    case c:
    of '"': result.add("\\\"")
    of '\\': result.add("\\\\")
    of '\n': result.add("\\n")
    of '\t': result.add("\\t")
    of '\r': result.add("\\r")
    else: result.add(c)
  result.add("\"")

func formatStartJson*(path: string): string =
  ## 输出 JSON 搜索起始标记: {"type":"start","path":"..."}\n
  result = "{\"type\":\"start\",\"path\":"
  result.add(jsonEscape(path))
  result.add("}\n")

func formatEndJson*(): string =
  ## 输出 JSON 搜索结束标记: {"type":"end"}\n
  result = "{\"type\":\"end\"}\n"

func formatMatchJson*(match: Match, ctx: Context): string =
  ## 输出 JSON 搜索匹配结果: {"type":"match","path":...,"line_number":...,"columns":{...},"line":...}\n
  ## 若 ctx 非空且 count > 0，则附加 context_before / context_after 数组。
  if match.isNil:
    return ""

  result = "{\"type\":\"match\","
  result.add("\"path\":")
  result.add(jsonEscape(match.path))
  result.add(",\"line_number\":")
  result.add($match.lineNumber)
  result.add(",\"columns\":{\"start\":")
  result.add($match.columnStart)
  result.add(",\"end\":")
  result.add($match.columnEnd)
  result.add("},\"line\":")
  result.add(jsonEscape(match.line))

  if not ctx.isNil and ctx.beforeCount > 0:
    result.add(",\"context_before\":[")
    for i in 0 ..< ctx.beforeCount:
      if i > 0:
        result.add(",")
      result.add(jsonEscape(ctx.linesBefore[i]))
    result.add("]")

  if not ctx.isNil and ctx.afterCount > 0:
    result.add(",\"context_after\":[")
    for i in 0 ..< ctx.afterCount:
      if i > 0:
        result.add(",")
      result.add(jsonEscape(ctx.linesAfter[i]))
    result.add("]")

  result.add("}\n")