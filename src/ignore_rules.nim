import std/os
import std/strutils
import pathutils
import glob

const
  MaxIgnorePatterns* = 256

type
  IgnoreRules* = object
    patterns: seq[string]

var
  globalRules: IgnoreRules
  projectRules: IgnoreRules
  rulesInitialized: bool

proc loadIgnoreFile*(path: string): seq[string] =
  ## 读取 .clineignore 文件
  ## 跳过空行和 # 注释，去除尾部空白
  if not fileExists(path):
    return @[]

  var patterns: seq[string] = @[]

  for line in lines(path):
    var trimmed = line
    # 去除尾部空白 (\n \r space tab)
    while trimmed.len > 0 and (trimmed[^1] in {' ', '\t', '\r', '\n'}):
      trimmed.setLen(trimmed.len - 1)

    if trimmed.len == 0:
      continue
    if trimmed[0] == '#':
      continue

    patterns.add(trimmed)

  return patterns

proc initIgnoreRules() =
  if rulesInitialized:
    return

  let home = getEnv("HOME")
  if home.len > 0:
    let globalPath = home / ".cline" / "data" / ".clineignore"
    globalRules.patterns = loadIgnoreFile(globalPath)

  projectRules.patterns = loadIgnoreFile(".clineignore")

  rulesInitialized = true

proc matchIgnorePattern(pattern, relPath: string): bool =
  ## 匹配忽略规则
  ## 使用 fnmatchPathname 直接匹配
  ## 若 pattern 以 '!' 开头，匹配时表示"不忽略"（否定规则）
  ## 先直接匹配，若 pattern 不含 '/' 则加 "*/" 前缀后再试
  var negate = false
  var pat = pattern
  if pattern.len > 0 and pattern[0] == '!':
    negate = true
    pat = pattern[1..^1]

  var matched = fnmatchPathname(pat, relPath)

  if not matched and find(pat, '/') == -1:
    matched = fnmatchPathname("*/" & pat, relPath)

  return if negate: not matched else: matched

proc resetIgnoreRules*() =
  ## 重置忽略规则状态（测试用）
  globalRules.patterns = @[]
  projectRules.patterns = @[]
  rulesInitialized = false

proc checkIgnorePath*(path: string): bool =
  ## 检查路径是否被 .clineignore 规则忽略
  if path.len == 0:
    return false

  initIgnoreRules()

  if globalRules.patterns.len == 0 and projectRules.patterns.len == 0:
    return false

  let relPath = toRelPath(path)

  for pattern in globalRules.patterns:
    if matchIgnorePattern(pattern, relPath):
      return true

  for pattern in projectRules.patterns:
    if matchIgnorePattern(pattern, relPath):
      return true

  return false