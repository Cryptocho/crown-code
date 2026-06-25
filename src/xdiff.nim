import std/[strutils]
import experimental/diff

type
  LineChange* = enum
    lcKeep
    lcAdd
    lcDelete

  Hunk* = ref object
    startA*, countA*: int
    startB*, countB*: int
    lines*: seq[(LineChange, string)]

type
  DiffSegment = object
    isChange: bool
    aStart, aEnd: int
    bStart, bEnd: int

func getLines(s: string): seq[string] =
  for line in s.splitLines:
    result.add(line)

func endsWithNewline(s: string): bool =
  s.len > 0 and s[^1] == '\n'

func stripTrailingNewline(s: string): string =
  if s.len > 0 and s[^1] == '\n':
    result = s[0 .. ^2]
  else:
    result = s

func buildHunks(items: seq[Item], linesA, linesB: openArray[string], ctxLen: int): seq[Hunk] =
  if items.len == 0:
    return @[]

  var segments: seq[DiffSegment]
  var aPos = 0
  var bPos = 0

  for item in items:
    if aPos < item.startA:
      segments.add(DiffSegment(
        isChange: false,
        aStart: aPos, aEnd: item.startA,
        bStart: bPos, bEnd: item.startB
      ))
    segments.add(DiffSegment(
      isChange: true,
      aStart: item.startA, aEnd: item.startA + item.deletedA,
      bStart: item.startB, bEnd: item.startB + item.insertedB
    ))
    aPos = item.startA + item.deletedA
    bPos = item.startB + item.insertedB

  if aPos < linesA.len:
    segments.add(DiffSegment(
      isChange: false,
      aStart: aPos, aEnd: linesA.len,
      bStart: bPos, bEnd: linesB.len
    ))

  var i = 0
  while i < segments.len:
    if not segments[i].isChange:
      inc i
      continue

    var changeStart = i
    var changeEnd = i

    var j = i + 2
    while j < segments.len and segments[j].isChange:
      let gapIdx = j - 1
      if not segments[gapIdx].isChange:
        let gapLen = segments[gapIdx].aEnd - segments[gapIdx].aStart
        if gapLen <= 2 * ctxLen:
          changeEnd = j
          j += 2
          continue
      break

    var hunkAStart: int
    var hunkBStart: int
    if changeStart > 0 and not segments[changeStart - 1].isChange:
      let prev = segments[changeStart - 1]
      let ctx = min(ctxLen, prev.aEnd - prev.aStart)
      hunkAStart = prev.aEnd - ctx
      hunkBStart = prev.bEnd - ctx
    else:
      hunkAStart = segments[changeStart].aStart
      hunkBStart = segments[changeStart].bStart

    var hunkAEnd: int
    var hunkBEnd: int
    if changeEnd + 1 < segments.len and not segments[changeEnd + 1].isChange:
      let nxt = segments[changeEnd + 1]
      let ctx = min(ctxLen, nxt.aEnd - nxt.aStart)
      hunkAEnd = nxt.aStart + ctx
      hunkBEnd = nxt.bStart + ctx
    else:
      hunkAEnd = segments[changeEnd].aEnd
      hunkBEnd = segments[changeEnd].bEnd

    var hunkLines: seq[(LineChange, string)]

    for k in hunkAStart ..< segments[changeStart].aStart:
      hunkLines.add((lcKeep, linesA[k]))

    for k in changeStart .. changeEnd:
      if segments[k].isChange:
        for idx in segments[k].aStart ..< segments[k].aEnd:
          hunkLines.add((lcDelete, linesA[idx]))
        for idx in segments[k].bStart ..< segments[k].bEnd:
          hunkLines.add((lcAdd, linesB[idx]))
      else:
        for idx in segments[k].aStart ..< segments[k].aEnd:
          hunkLines.add((lcKeep, linesA[idx]))

    for k in segments[changeEnd].aEnd ..< hunkAEnd:
      hunkLines.add((lcKeep, linesA[k]))

    var countA = 0
    var countB = 0
    for hl in hunkLines:
      if hl[0] != lcAdd: inc countA
      if hl[0] != lcDelete: inc countB

    result.add(Hunk(
      startA: hunkAStart,
      countA: countA,
      startB: hunkBStart,
      countB: countB,
      lines: hunkLines
    ))

    i = changeEnd + 1

func formatHunk(hunk: Hunk, aNoNewline, bNoNewline: bool, isLastHunk: bool): string =
  let startADisp = if hunk.countA == 0: hunk.startA else: hunk.startA + 1
  let startBDisp = if hunk.countB == 0: hunk.startB else: hunk.startB + 1

  var parts: seq[string]
  parts.add("@@ -$1,$2 +$3,$4 @@\n" % [$startADisp, $hunk.countA, $startBDisp, $hunk.countB])

  var lastDeleteIdx = -1
  var lastAddIdx = -1

  for i, (change, line) in hunk.lines:
    case change:
    of lcKeep:
      parts.add(" " & line & "\n")
    of lcAdd:
      lastAddIdx = parts.len
      parts.add("+" & line & "\n")
    of lcDelete:
      lastDeleteIdx = parts.len
      parts.add("-" & line & "\n")

  if isLastHunk:
    if aNoNewline and lastDeleteIdx >= 0 and bNoNewline and lastAddIdx >= 0:
      if lastDeleteIdx > lastAddIdx:
        parts.insert("\\ No newline at end of file\n", lastDeleteIdx + 1)
        parts.insert("\\ No newline at end of file\n", lastAddIdx + 1)
      else:
        parts.insert("\\ No newline at end of file\n", lastAddIdx + 1)
        parts.insert("\\ No newline at end of file\n", lastDeleteIdx + 1)
    elif aNoNewline and lastDeleteIdx >= 0:
      parts.insert("\\ No newline at end of file\n", lastDeleteIdx + 1)
    elif bNoNewline and lastAddIdx >= 0:
      parts.insert("\\ No newline at end of file\n", lastAddIdx + 1)

  result = parts.join("")

func diff*(a, b: string; ctxLen: int = 3): string =
  if a == b:
    return ""

  let aNoNL = a.len > 0 and not endsWithNewline(a)
  let bNoNL = b.len > 0 and not endsWithNewline(b)

  let normA = stripTrailingNewline(a)
  let normB = stripTrailingNewline(b)

  if normA.len == 0 and normB.len == 0:
    return ""

  var linesA: seq[string]
  var linesB: seq[string]
  if normA.len > 0:
    linesA = getLines(normA)
  if normB.len > 0:
    linesB = getLines(normB)

  if normA.len == 0:
    let hunk = Hunk(
      startA: 0, countA: 0,
      startB: 0, countB: linesB.len,
      lines: newSeq[(LineChange, string)](linesB.len)
    )
    for i, line in linesB:
      hunk.lines[i] = (lcAdd, line)
    return formatHunk(hunk, false, bNoNL, true)

  if normB.len == 0:
    let hunk = Hunk(
      startA: 0, countA: linesA.len,
      startB: 0, countB: 0,
      lines: newSeq[(LineChange, string)](linesA.len)
    )
    for i, line in linesA:
      hunk.lines[i] = (lcDelete, line)
    return formatHunk(hunk, aNoNL, false, true)

  let items = diffText(normA, normB)

  if items.len == 0 and a != b:
    if linesA.len == linesB.len and linesA.len > 0:
      var sameExceptLast = true
      for k in 0 .. linesA.len - 2:
        if linesA[k] != linesB[k]:
          sameExceptLast = false
          break
      if sameExceptLast and linesA[^1] == linesB[^1] and aNoNL != bNoNL:
        let lastIdx = linesA.len - 1
        let hunk = Hunk(
          startA: lastIdx, countA: 1,
          startB: lastIdx, countB: 1,
          lines: @[(lcDelete, linesA[lastIdx]), (lcAdd, linesB[lastIdx])]
        )
        return formatHunk(hunk, aNoNL, bNoNL, true)
    return ""

  let hunks = buildHunks(items, linesA, linesB, ctxLen)

  result = ""
  for i, hunk in hunks:
    let isLast = i == hunks.len - 1
    result.add(formatHunk(hunk, aNoNL, bNoNL, isLast))
