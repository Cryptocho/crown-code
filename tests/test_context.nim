import unittest
import context

suite "context":
  test "create":
    let ctx = newContext(3, 2)
    check ctx != nil
    check ctx.beforeMax == 3
    check ctx.afterMax == 2

  test "create edge cases":
    let ctx0 = newContext(0, 0)
    check ctx0 != nil
    check ctx0.beforeMax == 0
    check ctx0.afterMax == 0

    let ctxNeg = newContext(-5, -10)
    check ctxNeg != nil
    check ctxNeg.beforeMax == 0
    check ctxNeg.afterMax == 0

  test "add line":
    let ctx = newContext(2, 2)
    check ctx != nil
    ctx.addLine("line1")
    ctx.addLine("line2")
    check ctx.afterCount == 2

  test "reset":
    let ctx = newContext(2, 2)
    ctx.addLine("line1")
    ctx.addLine("line2")
    check ctx.afterCount == 2
    ctx.clearContext()
    check ctx.afterCount == 0

  test "free null":
    nil.clearContext()
