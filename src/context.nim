type
  Context* = ref object
    linesBefore*: seq[string]
    linesAfter*: seq[string]
    beforeCount*: int
    afterCount*: int
    beforeMax*: int
    afterMax*: int

func newContext*(before, after: int): Context =
  let bm = max(before, 0)
  let am = max(after, 0)
  result = Context(
    beforeMax: bm,
    afterMax: am,
  )
  if bm > 0:
    result.linesBefore = newSeq[string](bm)
  if am > 0:
    result.linesAfter = newSeq[string](am)

func addLine*(ctx: Context, line: string) =
  if ctx.isNil:
    return
  if ctx.afterMax > 0 and ctx.afterCount < ctx.afterMax:
    ctx.linesAfter[ctx.afterCount] = line
    inc ctx.afterCount

func clearContext*(ctx: Context) =
  if ctx.isNil:
    return
  if ctx.beforeMax > 0:
    ctx.linesBefore = newSeq[string](ctx.beforeMax)
    ctx.beforeCount = 0
  if ctx.afterMax > 0:
    ctx.linesAfter = newSeq[string](ctx.afterMax)
    ctx.afterCount = 0
