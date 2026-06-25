import std/[re, options]

type
  SearchOption* = enum
    soCaseInsensitive
    soMultiLine
    soDotAll

  Match* = ref object
    lineNumber*: int      ## 1-based 行号
    columnStart*: int     ## 0-based 字节偏移，匹配起始位置
    columnEnd*: int       ## 0-based 字节偏移，匹配结束位置（不含）
    line*: string         ## 匹配所在的完整行内容
    path*: string         ## 文件路径，由调用方赋值

  Search* = ref object
    regex: Regex

func reFlags(options: set[SearchOption]): set[RegexFlag] =
  for opt in options:
    case opt:
    of soCaseInsensitive: result.incl(reIgnoreCase)
    of soMultiLine: result.incl(reMultiLine)
    of soDotAll: result.incl(reDotAll)

func newSearch*(pattern: string, options: set[SearchOption] = {}): Search =
  Search(regex: re(pattern, reFlags(options)))

func findLineRange(text: string, offset: int): (int, int) =
  ## 找到 offset 所在行的起始和结束位置
  var start = offset
  while start > 0 and text[start - 1] != '\n':
    dec start
  var finish = offset
  while finish < text.len and text[finish] != '\n' and text[finish] != '\r':
    inc finish
  result = (start, finish)

func calcLineNumber*(text: string, offset: int): int =
  if text.len == 0:
    return 1
  var line = 1
  for i in 0 ..< min(offset, text.len):
    if text[i] == '\n':
      inc line
  result = line

func getLine*(text: string, lineNumber: int): Option[string] =
  if text.len == 0 or lineNumber < 1:
    return none(string)
  var currentLine = 1
  var lineStart = 0
  for i in 0 ..< text.len:
    if currentLine == lineNumber:
      var lineEnd = i
      while lineEnd < text.len and text[lineEnd] != '\n' and text[lineEnd] != '\r':
        inc lineEnd
      return some(text[lineStart ..< lineEnd])
    if text[i] == '\n':
      inc currentLine
      lineStart = i + 1
  if currentLine == lineNumber and lineStart < text.len:
    return some(text[lineStart ..< text.len])
  return none(string)

func matchFirst*(s: Search, text: string, offset: int = 0): Option[Match] =
  if s.isNil or text.len == 0 or offset < 0 or offset > text.len:
    return none(Match)
  var matches = newSeq[string](1)
  let (first, last) = findBounds(text, s.regex, matches, start = offset)
  if first < 0:
    return none(Match)
  let absEnd = last + 1  # findBounds returns last = endPos - 1
  let (lineStart, lineEnd) = findLineRange(text, first)
  result = some(Match(
    lineNumber: calcLineNumber(text, first),
    columnStart: first,
    columnEnd: absEnd,
    line: text[lineStart ..< lineEnd]
  ))

func matchAll*(s: Search, text: string, offset: int = 0): seq[Match] =
  if s.isNil or text.len == 0 or offset < 0 or offset >= text.len:
    return @[]
  var pos = offset
  while pos <= text.len:
    var matches = newSeq[string](1)
    let (first, last) = findBounds(text, s.regex, matches, start = pos)
    if first < 0:
      break
    let absEnd = last + 1
    let (lineStart, lineEnd) = findLineRange(text, first)
    result.add(Match(
      lineNumber: calcLineNumber(text, first),
      columnStart: first,
      columnEnd: absEnd,
      line: text[lineStart ..< lineEnd]
    ))
    pos = max(absEnd, pos + 1)
    if pos >= text.len:
      break