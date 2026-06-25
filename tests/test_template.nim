import unittest
import crown_code

suite "project bootstrap":
  test "runApp proc compiles and runs":
    crown_code.runApp()

  test "module can be imported":
    check true
