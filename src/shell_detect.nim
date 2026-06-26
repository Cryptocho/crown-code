## Shell 检测模块
##
## 对应 C 源码：temp/src/tools.c `detect_shells()`
##
## 检测系统中可用的 shell，优先使用 $SHELL 环境变量（POSIX），
## 否则在 PATH 中搜索已知 shell 可执行文件。

import std/os

type
  ShellInfo* = object
    name*: string
    path*: string
    found*: bool

proc detectShells*(): seq[ShellInfo] =
  ## 检测系统中可用的 shell
  ##
  ## POSIX: 读取 $SHELL 环境变量
  ## Windows: 在 PATH 中搜索已知 shell 可执行文件
  ##
  ## 返回 seq[ShellInfo]，每个元素包含 shell 名称、路径和存在标记

  when not defined(windows):
    # POSIX 实现：读取 $SHELL 环境变量
    let shellEnv = getEnv("SHELL")
    if shellEnv.len > 0:
      let name = extractFilename(shellEnv)
      result.add(ShellInfo(
        name: name,
        path: shellEnv,
        found: true
      ))
  else:
    # Windows 实现：搜索已知 shell
    const shellNames = ["bash.exe", "pwsh.exe", "powershell.exe", "cmd.exe"]
    const extraBashPaths = [
      "C:\\Program Files\\Git\\bin\\bash.exe",
      "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
      "C:\\msys64\\usr\\bin\\bash.exe",
      "C:\\mingw64\\usr\\bin\\bash.exe",
      "C:\\Git\\bin\\bash.exe"
    ]

    for name in shellNames:
      # 先在 PATH 中搜索
      let foundPath = findExe(name)
      if foundPath.len > 0:
        result.add(ShellInfo(
          name: name,
          path: foundPath,
          found: true
        ))
      elif name == "bash.exe":
        # bash 额外检查固定安装路径
        for extraPath in extraBashPaths:
          if fileExists(extraPath):
            result.add(ShellInfo(
              name: name,
              path: extraPath,
              found: true
            ))
            break