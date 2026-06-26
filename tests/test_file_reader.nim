import unittest
import std/os
import std/strutils
import file_reader

suite "file_reader: error handling":
  test "null_path returns NullPath error":
    let result = readFileRange("", 1, 0)
    check result.error == FileReaderError.NullPath
    check result.errorMessage == "Path parameter is required"

  test "empty_path returns NullPath error":
    let result = readFileRange("", 1, 0)
    check result.error == FileReaderError.NullPath
    check result.errorMessage == "Path parameter is required"

  test "file_not_found returns ReadFailed error":
    let result = readFileRange("/nonexistent/file.txt", 1, 0)
    check result.error == FileReaderError.ReadFailed

suite "file_reader: basic functionality":
  test "basic_read formats line numbers correctly":
    let path = "/tmp/test_basic_read.txt"
    writeFile(path, "line1\nline2\nline3\n")
    let result = readFileRange(path, 1, 2000)
    check result.error == FileReaderError.Success
    check result.content.contains("1 | line1")
    check result.content.contains("2 | line2")
    check result.content.contains("3 | line3")
    check result.content.contains("(File has 3 lines total.)")
    removeFile(path)

  test "line_range shows correct range":
    let path = "/tmp/test_line_range.txt"
    writeFile(path, "line1\nline2\nline3\nline4\nline5\n")
    let result = readFileRange(path, 2, 4)
    check result.error == FileReaderError.Success
    check result.content.contains("2 | line2")
    check result.content.contains("3 | line3")
    check result.content.contains("4 | line4")
    check result.content.contains("start_line=5")
    removeFile(path)

  test "line_range_swap exchanges start and end":
    let path = "/tmp/test_line_range_swap.txt"
    writeFile(path, "line1\nline2\nline3\nline4\nline5\n")
    let result = readFileRange(path, 5, 2)
    check result.error == FileReaderError.Success
    check result.content.contains("5 | line5")
    removeFile(path)

  test "line_range_beyond_file handles range past EOF":
    let path = "/tmp/test_beyond_eof.txt"
    writeFile(path, "line1\nline2\nline3\n")
    let result = readFileRange(path, 1, 100)
    check result.error == FileReaderError.Success
    check result.content.contains("(File has 3 lines total.)")
    removeFile(path)

  test "single_line_file reads correctly":
    let path = "/tmp/test_single_line.txt"
    writeFile(path, "single line\n")
    let result = readFileRange(path, 1, 0)
    check result.error == FileReaderError.Success
    check result.content.contains("1 | single line")
    removeFile(path)

suite "file_reader: caching":
  test "cache_warning_third_read_shows_duplicate":
    let path = "/tmp/test_cache_dup.txt"
    writeFile(path, "content\n")
    # 第1次读 - 无警告
    let r1 = readFileRange(path, 1, 0)
    check r1.error == FileReaderError.Success
    check not(r1.content.contains("[DUPLICATE READ]"))
    # 第2次读 - "[File already read]"
    let r2 = readFileRange(path, 1, 0)
    check r2.error == FileReaderError.Success
    check r2.content.contains("[File already read]")
    # 第3次读 - "[DUPLICATE READ]"
    let r3 = readFileRange(path, 1, 0)
    check r3.error == FileReaderError.Success
    check r3.content.contains("[DUPLICATE READ]")
    removeFile(path)

  test "mtime_eviction_detects_modification":
    let path = "/tmp/test_mtime_evict.txt"
    writeFile(path, "version1\n")
    # 第1次读 - 建立缓存
    let r1 = readFileRange(path, 1, 0)
    check r1.error == FileReaderError.Success
    # 修改文件
    sleep(1001)  # 确保 mtime 变化（至少 1 秒）
    writeFile(path, "version2\n")
    # 再次读取 - 应检测到变化，readCount 重置
    let r2 = readFileRange(path, 1, 0)
    check r2.error == FileReaderError.Success
    # 第3次读 - 因为 mtime 重置了 readCount，所以这是第2次（无 DUPLICATE 警告）
    let r3 = readFileRange(path, 1, 0)
    check r3.error == FileReaderError.Success
    check r3.content.contains("[File already read]")
    removeFile(path)

suite "file_reader: large files":
  test "large_file_reports_correct_total":
    let path = "/tmp/test_large_file.txt"
    var content = ""
    for i in 1..2000:
      content.add("line" & $i & "\n")
    writeFile(path, content)
    let result = readFileRange(path, 1, 2000)
    check result.error == FileReaderError.Success
    check result.content.contains("(File has 2000 lines total.)")
    removeFile(path)

suite "file_reader: path resolution":
  test "absolute_path_resolves_correctly":
    let path = "/tmp/test_absolute_path.txt"
    writeFile(path, "content\n")
    let result = readFileRange(path, 1, 0)
    check result.error == FileReaderError.Success
    removeFile(path)

  test "relative_path_resolves_correctly":
    let path = "test_relative_path.txt"
    writeFile(path, "content\n")
    let result = readFileRange(path, 1, 0)
    check result.error == FileReaderError.Success
    removeFile(path)