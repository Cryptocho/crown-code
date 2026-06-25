import std/os
import std/strutils

const MaxPathLength* = 4096

func normalizeSlashes*(path: string): string =
  ## 将所有反斜杠 \ 替换为正斜杠 /
  path.replace('\\', '/')

proc resolveWorkspacePath*(relativePath: string): string =
  ## 解析工作区路径为绝对路径
  ## 如果已经是绝对路径则直接返回，否则前面加上 getCurrentDir()
  if relativePath.len == 0:
    return ""
  if isAbsolute(relativePath):
    return relativePath
  return getCurrentDir() / relativePath

proc toRelPath*(path: string; cwd: string = ""): string =
  ## 将绝对路径转换为相对路径（相对于工作区）
  ## 若 path 以 CWD 开头，去掉前缀；否则返回原路径
  ## 所有反斜杠归一化为正斜杠
  let baseDir = if cwd.len > 0: cwd else: getCurrentDir()
  if path.startsWith(baseDir) and baseDir.len > 0:
    var rest = path[baseDir.len .. ^1]
    while rest.len > 0 and (rest[0] == '/' or rest[0] == '\\'):
      rest = rest[1 .. ^1]
    result = normalizeSlashes(rest)
  else:
    result = normalizeSlashes(path)

proc resolvePath*(relativePath: string): tuple[absolutePath: string, displayPath: string] =
  ## 解析工作区路径，返回 (绝对路径, 显示路径) 元组
  let absPath = resolveWorkspacePath(relativePath)
  result = (absPath, absPath)