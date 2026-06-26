import std/os
import std/strutils
import std/unittest
import list_files
import ignore_rules
import pathutils

suite "list files - error handling":
  test "null path returns NullPath":
    let result = listFiles("")
    check result.error == ListFilesError.NullPath
    check result.count == 0

  test "nonexistent path returns DirNotFound":
    let result = listFiles("/nonexistent_dir_for_test_xyz")
    check result.error == ListFilesError.DirNotFound
    check result.count == 0

  test "root directory returns empty result (security)":
    let result = listFiles("/")
    check result.count == 0
    check result.error == ListFilesError.Success

  test "home directory returns empty result (security)":
    let home = getHomeDir()
    if home.len > 0:
      let result = listFiles(home)
      check result.count == 0
      check result.error == ListFilesError.Success

suite "list files - basic functionality":
  test "empty directory returns no error and count 0":
    let testDir = getTempDir() / "test_list_files_empty_nim"
    removeDir(testDir)
    createDir(testDir)

    let result = listFiles(testDir)
    check result.error == ListFilesError.Success
    check result.count == 0
    check result.didHitLimit == false

    removeDir(testDir)

  test "normal directory lists files and directories":
    let testDir = getTempDir() / "test_list_files_normal_nim"
    removeDir(testDir)
    createDir(testDir)
    createDir(testDir / "subdir")
    createDir(testDir / "another_dir")
    writeFile(testDir / "file_c.txt", "")
    writeFile(testDir / "file_a.txt", "")
    writeFile(testDir / "file_b.txt", "")

    let result = listFiles(testDir)
    check result.error == ListFilesError.Success
    check result.count == 5

    if result.count >= 5:
      # 目录排在前面
      check result.entries[0] == "another_dir" or result.entries[0] == "subdir"
      check result.entries[1] == "another_dir" or result.entries[1] == "subdir"
      # 文件按字母序
      check result.entries[2] == "file_a.txt"
      check result.entries[3] == "file_b.txt"
      check result.entries[4] == "file_c.txt"

    removeDir(testDir)

  test "sort order: dirs first then alphabetically":
    let testDir = getTempDir() / "test_list_files_sort_nim"
    removeDir(testDir)
    createDir(testDir)
    createDir(testDir / "zzz_dir")
    createDir(testDir / "aaa_dir")
    writeFile(testDir / "zzz_file.txt", "")
    writeFile(testDir / "aaa_file.txt", "")

    let result = listFiles(testDir)
    check result.error == ListFilesError.Success
    check result.count == 4

    if result.count >= 4:
      check result.entries[0] == "aaa_dir"
      check result.entries[1] == "zzz_dir"
      check result.entries[2] == "aaa_file.txt"
      check result.entries[3] == "zzz_file.txt"

    removeDir(testDir)

  test "hidden files are listed":
    let testDir = getTempDir() / "test_list_files_hidden_nim"
    removeDir(testDir)
    createDir(testDir)
    writeFile(testDir / ".hidden", "")
    writeFile(testDir / "visible.txt", "")

    let result = listFiles(testDir)
    check result.error == ListFilesError.Success
    check result.count == 2

    var foundHidden = false
    var foundVisible = false
    for entry in result.entries:
      if entry == ".hidden": foundHidden = true
      if entry == "visible.txt": foundVisible = true
    check foundHidden == true
    check foundVisible == true

    removeDir(testDir)

  test "special characters in filenames":
    let testDir = getTempDir() / "test_list_files_special_nim"
    removeDir(testDir)
    createDir(testDir)
    writeFile(testDir / "file with spaces.txt", "")
    writeFile(testDir / "file-with-dashes.txt", "")

    let result = listFiles(testDir)
    check result.error == ListFilesError.Success
    check result.count == 2

    var foundSpaces = false
    var foundDashes = false
    for entry in result.entries:
      if entry == "file with spaces.txt": foundSpaces = true
      if entry == "file-with-dashes.txt": foundDashes = true
    check foundSpaces == true
    check foundDashes == true

    removeDir(testDir)

suite "list files - limit":
  test "limit truncation at MAX_LIST_ENTRIES":
    let testDir = getTempDir() / "test_list_files_limit_nim"
    removeDir(testDir)
    createDir(testDir)

    for i in 0 ..< 205:
      writeFile(testDir / ("file_" & ($i).align(3, '0') & ".txt"), "")

    let result = listFiles(testDir)
    check result.error == ListFilesError.Success
    check result.count == 200
    check result.didHitLimit == true

    removeDir(testDir)

suite "list files - ignore rules":
  test "ignore rules filter out matching files":
    let testDir = getTempDir() / "test_list_files_ignore_nim"
    removeDir(testDir)
    createDir(testDir)
    createDir(testDir / "sub")
    writeFile(testDir / "allowed.txt", "")
    writeFile(testDir / "secret.log", "")

    # 在临时目录创建 .clineignore，切换到该目录，测试后恢复
    let origDir = getCurrentDir()
    setCurrentDir(testDir)
    writeFile(testDir / ".clineignore", "secret.log\n")
    resetIgnoreRules()

    let result = listFiles(testDir)
    check result.error == ListFilesError.Success

    var foundSecret = false
    for entry in result.entries:
      if entry == "secret.log":
        foundSecret = true
    check foundSecret == false

    # 清理
    removeFile(testDir / ".clineignore")
    resetIgnoreRules()
    setCurrentDir(origDir)
    removeDir(testDir)

  test "ignored directory returns PermissionDenied":
    let testDir = getTempDir() / "test_list_files_blocked_nim"
    removeDir(testDir)
    createDir(testDir)

    let origDir = getCurrentDir()
    setCurrentDir(getTempDir())
    writeFile(getTempDir() / ".clineignore", "test_list_files_blocked_nim\n")
    resetIgnoreRules()

    let result = listFiles(testDir)
    check result.error == ListFilesError.PermissionDenied
    check result.errorMessage.contains(".clineignore")

    # 清理
    removeFile(getTempDir() / ".clineignore")
    resetIgnoreRules()
    setCurrentDir(origDir)
    removeDir(testDir)