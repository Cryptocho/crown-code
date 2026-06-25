import unittest
import std/os
import std/strutils
import file_writer
import file_reader
import ignore_rules

suite "file_writer: error handling":
  test "null_path returns NullPath error":
    let result = writeFileContent("")
    check result.error == FileWriterError.NullPath
    check result.errorMessage == "Path parameter is required"

  test "empty_path returns NullPath error":
    let result = writeFileContent("")
    check result.error == FileWriterError.NullPath
    check result.errorMessage == "Path parameter is required"

suite "file_writer: basic functionality":
  test "write_file_creates_content":
    let path = "/tmp/test_write_basic.txt"
    let result = writeFileContent(path, "hello world")
    check result.error == FileWriterError.Success
    check readFile(path) == "hello world"
    removeFile(path)

  test "write_empty_content_creates_empty_file":
    let path = "/tmp/test_write_empty.txt"
    let result = writeFileContent(path, "")
    check result.error == FileWriterError.Success
    check fileExists(path)
    check readFile(path) == ""
    removeFile(path)

suite "file_writer: caching":
  test "cache_invalidation_after_write_resets_read_count":
    let path = "/tmp/test_cache_inval.txt"
    # 先写入内容
    discard writeFileContent(path, "version1\n")
    # 读取一次 → 建立缓存
    let r1 = readFileRange(path, 1, 0)
    check r1.error == FileReaderError.Success
    # 验证缓存已建立
    let cached = cacheGet(path)
    check cached.readCount > 0
    # 写入 → 触发缓存失效
    discard writeFileContent(path, "version2\n")
    # 验证缓存已失效
    let cachedAfter = cacheGet(path)
    check cachedAfter.readCount == 0
    removeFile(path)

  test "repeated_writes_invalidate_cache_each_time":
    let path = "/tmp/test_repeat_write.txt"
    # 第一次写入
    discard writeFileContent(path, "v1\n")
    # 读取建立缓存
    discard readFileRange(path, 1, 0)
    check cacheGet(path).readCount > 0
    # 第二次写入 → 缓存失效
    discard writeFileContent(path, "v2\n")
    check cacheGet(path).readCount == 0
    # 读取重建缓存
    discard readFileRange(path, 1, 0)
    check cacheGet(path).readCount > 0
    # 第三次写入 → 再次失效
    discard writeFileContent(path, "v3\n")
    check cacheGet(path).readCount == 0
    removeFile(path)

suite "file_writer: access control":
  test "clineignore_returns_permission_denied":
    resetIgnoreRules()
    let ignorePath = ".clineignore"
    writeFile(ignorePath, "*.secret\n")
    let result = writeFileContent("test.secret", "secret data")
    check result.error == FileWriterError.PermissionDenied
    check result.errorMessage == "Access denied by .clineignore rules"
    removeFile(ignorePath)

  test "write_to_nonexistent_directory_fails":
    let result = writeFileContent("/nonexistent_dir_12345/file.txt", "content")
    check result.error == FileWriterError.WriteFailed
    check result.errorMessage.startsWith("Error writing file:")

suite "file_writer: write failure":
  test "write_to_readonly_directory_returns_write_failed":
    let roDir = "/tmp/test_ro_dir_write"
    createDir(roDir)
    # 去除写权限
    setFilePermissions(roDir, {})
    let result = writeFileContent(roDir / "test.txt", "content")
    check result.error == FileWriterError.WriteFailed
    check result.errorMessage.startsWith("Error writing file:")
    # 恢复权限以便删除
    setFilePermissions(roDir, {fpOthersRead, fpOthersWrite, fpOthersExec, fpGroupRead, fpGroupWrite, fpGroupExec, fpUserRead, fpUserWrite, fpUserExec})
    removeDir(roDir)