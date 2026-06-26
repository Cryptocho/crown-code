import std/[os, re, options]
import glob
import search
import ignore_rules

const
  MAX_SEARCH_DEPTH* = 10
  MAX_SEARCH_OUTPUT* = 256 * 1024
  MAX_CONTEXT_LINES* = 1

type
  SearchFilesError* = enum
    sfeSuccess = 0
    sfeNullParam
    sfeDirNotFound
    sfeRegexError

  SearchFilesResult* = object
    results*: string
    matchCount*: int
    error*: SearchFilesError
    errorMessage*: string

proc searchFile(filePath, relPath: string; srch: Search;
                result: var SearchFilesResult) =
  ## 在单个文件中搜索所有正则匹配并格式化输出。
  let content = try:
    readFile(filePath)
  except IOError:
    return

  if content.len == 0:
    return

  let matches = matchAll(srch, content)
  if matches.len == 0:
    return

  # 在循环前累加该文件所有匹配计数（与 C 版本逐次累加等价）
  result.matchCount += matches.len

  for match in matches:
    let matchLine = match.lineNumber

    # 估算所需空间用于截断检查
    let needed = relPath.len + match.line.len + 100
    if result.results.len + needed >= MAX_SEARCH_OUTPUT:
      # 截断标记，直接追加（C 版本也做相同操作）
      result.results.add("\n[Results truncated...]\n")
      return

    # 输出头部: \n{rel_path}\n│----\n
    result.results.add("\n" & relPath & "\n│----\n")

    # 输出前一行（如果存在）
    if matchLine > 1:
      let prevLine = getLine(content, matchLine - 1)
      if prevLine.isSome:
        result.results.add("│" & prevLine.get() & "\n")

    # 输出匹配行
    result.results.add("│" & match.line & "\n")

    # 输出后一行（如果存在）
    if matchLine > 0:
      let nextLine = getLine(content, matchLine + 1)
      if nextLine.isSome:
        result.results.add("│" & nextLine.get() & "\n")

    # 输出尾部
    result.results.add("│----\n")

proc searchDir(dir, baseDir, filePattern: string; srch: Search;
               depth: int; result: var SearchFilesResult) =
  ## 递归搜索目录，受 `MAX_SEARCH_DEPTH` 限制。
  if depth > MAX_SEARCH_DEPTH:
    return

  for (kind, path) in walkDir(dir):
    let filename = extractFilename(path)
    if filename == "." or filename == "..":
      continue

    let absPath = absolutePath(path)

    # glob 过滤：仅匹配 pattern 的条目才处理（C 版本对目录也做 glob 过滤）
    if not matchGlob(filename, filePattern):
      continue

    # ignore 过滤：所有条目（目录和文件）都需要检查
    if checkIgnorePath(absPath):
      continue

    case kind
    of pcDir:
      searchDir(path, baseDir, filePattern, srch, depth + 1, result)
    of pcFile:
      let relPath = relativePath(absPath, baseDir)
      searchFile(absPath, relPath, srch, result)
    else:
      discard

proc searchFiles*(directory, regex, filePattern: string): SearchFilesResult =
  ## 在 `directory` 中递归搜索匹配 `regex` 的文件内容。
  ## 只搜索文件名匹配 `filePattern`（glob 模式）的文件。
  ## 返回包含匹配行格式化的结果字符串。
  if directory.len == 0 or regex.len == 0 or filePattern.len == 0:
    return SearchFilesResult(error: sfeNullParam, errorMessage: "parameters cannot be empty")

  if not dirExists(directory):
    return SearchFilesResult(error: sfeDirNotFound, errorMessage: "directory not found: " & directory)

  var s: Search
  try:
    s = newSearch(regex)
  except RegexError as e:
    return SearchFilesResult(error: sfeRegexError, errorMessage: "invalid regex: " & e.msg)

  var res = SearchFilesResult(results: "", matchCount: 0, error: sfeSuccess)

  let baseDir = absolutePath(directory)
  searchDir(baseDir, baseDir, filePattern, s, 0, res)

  res