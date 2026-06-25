func matchClass(pattern: string, pi: var int, ch: char): bool =
  inc pi  # skip '['
  var negate = false
  if pi < pattern.len and pattern[pi] == '!':
    negate = true
    inc pi

  var matched = false

  if pi < pattern.len and pattern[pi] == ']':
    if ch == ']':
      matched = true
    inc pi

  while pi < pattern.len and pattern[pi] != ']':
    if pi + 2 < pattern.len and pattern[pi + 1] == '-' and pattern[pi + 2] != ']':
      let rangeStart = pattern[pi]
      let rangeEnd = pattern[pi + 2]
      if ch >= rangeStart and ch <= rangeEnd:
        matched = true
      inc pi, 3
    else:
      if ch == pattern[pi]:
        matched = true
      inc pi

  if pi < pattern.len:
    inc pi  # skip ']'

  return if negate: not matched else: matched

func fnmatch(pattern, str: string): bool =
  var pi = 0
  var si = 0
  var starPi = -1
  var matchSi = 0

  while si < str.len:
    if pi < pattern.len and (pattern[pi] == '?' or pattern[pi] == str[si]):
      inc pi
      inc si
    elif pi < pattern.len and pattern[pi] == '[':
      if matchClass(pattern, pi, str[si]):
        inc si
      elif starPi >= 0:
        pi = starPi + 1
        inc matchSi
        si = matchSi
      else:
        return false
    elif pi < pattern.len and pattern[pi] == '*':
      starPi = pi
      matchSi = si
      inc pi
    elif starPi >= 0:
      pi = starPi + 1
      inc matchSi
      si = matchSi
    else:
      return false

  while pi < pattern.len and pattern[pi] == '*':
    inc pi

  return pi == pattern.len

func fnmatchPathname*(pattern, str: string): bool =
  ## Like fnmatch, but with FNM_PATHNAME semantics:
  ## - '*' does not match '/'
  ## - '?' does not match '/'
  var pi = 0
  var si = 0
  var starPi = -1
  var matchSi = 0

  while si < str.len:
    if pi < pattern.len and (pattern[pi] == '?' and str[si] != '/' or pattern[pi] == str[si]):
      inc pi
      inc si
    elif pi < pattern.len and pattern[pi] == '[':
      if str[si] != '/' and matchClass(pattern, pi, str[si]):
        inc si
      elif starPi >= 0:
        pi = starPi + 1
        inc matchSi
        si = matchSi
      else:
        return false
    elif pi < pattern.len and pattern[pi] == '*':
      starPi = pi
      matchSi = si
      inc pi
    elif starPi >= 0:
      pi = starPi + 1
      inc matchSi
      # '*' 不能跨越路径分隔符
      if matchSi <= si or (matchSi > 0 and str[matchSi - 1] == '/'):
        return false
      si = matchSi
    else:
      return false

  while pi < pattern.len and pattern[pi] == '*':
    inc pi

  return pi == pattern.len

func matchGlob*(filename, pattern: string): bool =
  if pattern.len == 0:
    return false
  if pattern[0] == '!':
    return not fnmatch(pattern[1..^1], filename)
  return fnmatch(pattern, filename)

func matchGlobPathname*(filename, pattern: string): bool =
  if pattern.len == 0:
    return false
  if pattern[0] == '!':
    return not fnmatchPathname(pattern[1..^1], filename)
  return fnmatchPathname(pattern, filename)

func matchAnyGlob*(filename: string, patterns: openArray[string]): bool =
  if filename.len == 0 or patterns.len == 0:
    return false

  var matched = false
  for pattern in patterns:
    if pattern.len == 0:
      continue
    if pattern[0] == '!':
      if fnmatch(pattern[1..^1], filename):
        return false
    else:
      if fnmatch(pattern, filename):
        matched = true
  return matched
