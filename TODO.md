# C → Nim 迁移 TODO

> 目标：将 `temp/` 中的 C 代码全部迁移到 Nim，项目代码中不使用 FFI（`std/` 底层封装不在此限）。

---

## Phase 1: 基础工具（无依赖）

### 1.1 Context 上下文缓冲
- **C 对应**：`temp/src/context.c`
- **依赖**：无
- [x] 创建 `src/context.nim`
- [x] `context_t` → `ref object`（`linesBefore: seq[string]`, `linesAfter: seq[string]`）
- [x] `context_create(before, after)` → `newContext(before, after: int)`
- [x] `context_add_line(ctx, line)` → `addLine(ctx, line: string)`
- [x] `context_reset(ctx)` → `clearContext(ctx)`（避免与 `system.reset` 冲突）
- [x] `context_free(ctx)` → ORC 自动回收
- [x] 创建 `tests/test_context.nim`
- [x] 更新 `tests/test_runner.nim`

### 1.2 Glob 通配符匹配
- **C 对应**：`temp/src/glob.c`
- **依赖**：无
- [x] 创建 `src/glob.nim`
- [x] `glob_match(filename, pattern)` → `matchGlob(filename, pattern: string): bool`
- [x] `glob_matches(filename, patterns, count)` → `matchAnyGlob(filename: string, patterns: openArray[string]): bool`
- [x] 支持 `!` 否定前缀
- [x] 创建 `tests/test_glob.nim`
- [x] 更新 `tests/test_runner.nim`

### 1.3 XDiff Diff 引擎
- **C 对应**：`temp/include/xdiff.h`, `temp/src/xdiff.c`
- **依赖**：`std/experimental/diff`（Myers O(ND) 算法）
- [x] 创建 `src/xdiff.nim`（避免与 `std/diff` 模块名冲突）
- [x] 评估 `std/diff` 是否满足需求 → 使用 `experimental/diff.diffText` 作为算法引擎
- [x] `mmfile_t` / `memallocator_t` → `string` + ORC 自动管理替代
- [x] `xdl_diff(mf1, mf2, xpp, xecfg, ecb)` → `diff*(a, b: string; ctxLen: int = 3): string`
- [x] 输出 unified diff 格式（含 `\ No newline` 标记，0 计数 hunk header，上下文窗口合并）
- [x] 创建 `tests/test_diff.nim`（19 个测试用例，5 个测试套件）
- [x] 更新 `tests/test_runner.nim`

---

## Phase 2: 搜索系统

### 2.1 Search 正则搜索
- **C 对应**：`temp/include/search.h`, `temp/src/search.c`
- **依赖**：无（`std/re` 封装 PCRE，不直接 FFI）
- [x] 创建 `src/search.nim`
- [x] `Match` ref object（`lineNumber`, `columnStart`, `columnEnd`, `line`, `path`）
- [x] `Search` ref object，用 `std/re` 的 `re()` 编译
- [x] `matchFirst(s, text, offset=0)` → `Option[Match]`
- [x] `matchAll(s, text)` → `seq[Match]`
- [x] `calcLineNumber(text, offset)` → `int`
- [x] `getLine(text, lineNumber)` → `Option[string]`
- [x] ORC 自动回收
- [x] `SearchOption` enum（`soCaseInsensitive`, `soMultiLine`, `soDotAll`）
- [x] 创建 `tests/test_search.nim`（29 个用例）

### 2.2 JSON 搜索输出格式化
- **C 对应**：`temp/src/json.c` 中 `json_print_start()`, `json_print_end()`, `json_print_match()`, `json_escape()`
- **依赖**：2.1 Search（`Match`, `Context` 类型）
- [x] 创建 `src/search_json.nim`
- [x] `json_escape()` → `jsonEscape(str: string): string`（避免与 `std/json.escapeJson` 冲突）
- [x] `json_print_match(out, match, ctx)` → `formatMatchJson(match: Match, ctx: Context): string`
- [x] `json_print_start(out, path)` → `formatStartJson(path: string): string`
- [x] `json_print_end(out)` → `formatEndJson(): string`
- [x] 创建 `tests/test_search_json.nim`
- [x] 更新 `tests/test_runner.nim`

---

## Phase 3: 文件工具（依赖 search / xdiff / glob）

### 3.1 路径解析
- **C 对应**：`tools.c` 中 `resolve_workspace_path()`, `to_rel_path()`, `resolve_path()`
- **依赖**：无
- [x] 创建 `src/pathutils.nim`
- [x] 绝对路径 / 相对路径解析
- [x] 相对路径计算
- [x] 跨平台路径分隔符标准化
- [x] 创建 `tests/test_pathutils.nim`
- [x] 更新 `tests/test_runner.nim`

### 3.2 clineignore 规则
- **C 对应**：`tools.c` 中 `IgnoreRules`, `load_ignore_file()`, `init_ignore_rules()`, `check_ignore_path()`, `match_ignore_pattern()`
- **依赖**：3.1 路径解析
- [x] 创建 `src/ignore_rules.nim`
- [x] 加载全局 `~/.cline/data/.clineignore`
- [x] 加载项目级 `.clineignore`
- [x] fnmatch 匹配逻辑（`fnmatchPathname` 支持 `FNM_PATHNAME` 语义）
- [x] `checkIgnorePath(path: string): bool`
- [x] 创建 `tests/test_ignore_rules.nim`
- [x] 更新 `tests/test_runner.nim`

### 3.3 文件读取
- **C 对应**：`tools.c` 中 `file_read()`, `read_file_content()`, `count_lines()`, `format_content_with_line_numbers()`, `parse_line_range()`, `FileReadCacheEntry`
- **依赖**：3.1 路径, 3.2 ignore
- [x] 创建 `src/file_reader.nim`
- [x] `readFileRange(path, startLine, endLine): FileReaderResult`（避免与 `os.readFile` 冲突）
- [x] 行号格式化输出（`$lineNum | content`）
- [x] 重复读取缓存（256 槽哈希表 + mtime 检测）
- [x] 重复读取警告（第 2 次"[File already read]"，第 3 次起"[DUPLICATE READ]"）
- [x] 创建 `tests/test_file_reader.nim`（15 个测试用例）
- [x] 更新 `tests/test_runner.nim`

### 3.4 文件写入
- **C 对应**：`tools.c` 中 `file_write()`
- **依赖**：3.1 路径, 3.2 ignore
- [x] 创建 `src/file_writer.nim`
- [x] `writeFileContent(path, content: string): FileWriterResult`（避免与 `os.writeFile` 冲突）
- [x] 写入后缓存失效（`cacheInvalidate`）
- [x] 创建 `tests/test_file_writer.nim`（8 个测试用例）
- [x] 更新 `tests/test_runner.nim`

### 3.5 文件编辑（精确替换）
- **C 对应**：`tools.c` 中 `file_edit()`, `split_into_lines()`, `join_lines()`
- **依赖**：3.1 路径, 3.4 写入
- [x] 创建 `src/file_edit.nim`
- [x] `editFile(path, oldStr, newStr: string, multiple: bool): FileEditResult`
- [x] 行级精确匹配：先找 `oldStr`，确认匹配次数，替换
- [x] 错误码：未找到 / 多次匹配 / 读写失败
- [x] 创建 `tests/test_file_edit.nim`（15 个测试用例）
- [x] 更新 `tests/test_runner.nim`

### 3.6 代码格式化
- **C 对应**：`tools.c` 中 `format_file()`, `process_content()`
- **依赖**：3.1 路径
- [x] 创建 `src/formatter.nim`
- [x] Tab 转 4 空格
- [x] 行尾空白修剪
- [x] 保留原有空行
- [x] 创建 `tests/test_formatter.nim`（10 个测试用例）
- [x] 更新 `tests/test_runner.nim`

### 3.7 Shell 检测
- **C 对应**：`tools.c` 中 `detect_shells()`
- **依赖**：无
- [x] 创建 `src/shell_detect.nim`
- [x] POSIX: `getEnv("SHELL")`
- [x] Windows: PATH 中查找 bash.exe / pwsh.exe / powershell.exe / cmd.exe + 额外路径
- [x] 创建 `tests/test_shell_detect.nim`
- [x] 更新 `tests/test_runner.nim`

### 3.8 命令执行
- **C 对应**：`tools.c` 中 `execute_command()`, `split_commands()`, `trim_whitespace()`, `CircularBuffer`
- **依赖**：3.7 Shell 检测
- [x] 创建 `src/command_exec.nim`
- [x] `execCommand(command: string, blacklist: openArray[string]): CommandResult`
- [x] 命令拆分（支持 `&&`, `||`, `|`, `;`, `&`）
- [x] 审批检查（黑名单匹配 + 用户确认 TODO）
- [x] 子进程启动 → `std/osproc.startProcess()`
- [x] stdout/stderr 流式捕获（环形缓冲区 `CircularBuffer`）
- [x] Timeout（`DEFAULT_TIMEOUT_SECONDS = 300`）
- [x] 最大输出限制（`MAX_FULL_OUTPUT_SIZE = 1MB`）
- [x] 执行时间统计
- [x] 创建 `tests/test_command_exec.nim`
- [x] 更新 `tests/test_runner.nim`

### 3.9 目录列表
- **C 对应**：`tools.c` 中 `list_files()`
- **依赖**：3.1 路径, 3.2 ignore
- [x] 创建 `src/list_files.nim`
- [x] `listFiles(path: string): ListFilesResult`
- [x] 目录遍历 → `os.walkDir()`
- [x] `.` / `..` 过滤
- [x] 排序：目录优先 → 字母序
- [x] 限制：`MAX_LIST_ENTRIES = 200`
- [x] 创建 `tests/test_list_files.nim`
- [x] 更新 `tests/test_runner.nim`

### 3.10 文件内容搜索
- **C 对应**：`tools.c` 中 `search_files()`, `search_dir()`, `search_file()`
- **依赖**：2.1 Search, 1.2 Glob, 3.1 路径, 3.2 ignore
- [x] 创建 `src/search_files.nim`
- [x] `searchFiles(directory, regex, filePattern: string): SearchFilesResult`
- [x] 递归目录搜索（深度限制 `MAX_SEARCH_DEPTH = 10`）
- [x] clineignore 检查
- [x] 匹配结果格式化（路径标题 + 上下文行）
- [x] 输出截断（`MAX_SEARCH_OUTPUT = 256KB`）
- [x] 创建 `tests/test_search_files.nim`
- [x] 更新 `tests/test_runner.nim`

---

## Phase 4: MCP 客户端（独立协议栈，无项目内依赖）

### 4.1 JSON-RPC 通信层
- **依赖**：无（用 `std/json`）
- [x] 创建 `src/mcp/jsonrpc.nim`
- [x] `buildRequest(meth, params: JsonNode, id: int64): string`
- [x] `buildNotification(meth, params: JsonNode): string`
- [x] `parseResponse(jsonStr: string): JsonNode`（仅反序列化，不校验语义）
- [x] 创建 `tests/test_mcp_jsonrpc.nim`（14 个用例）
- [x] 更新 `tests/test_runner.nim`

### 4.2 stdio 传输
- **C 对应**：`mcp.c` 中 `internal_io_spawn_child()`, `internal_io_read_line()`, `internal_io_write_line()`
- **依赖**：4.1 JSON-RPC
- [x] 创建 `src/mcp/transport_stdio.nim`
- [x] 子进程启动：手动 `fork()` + `pipe()` × 3 + `dup2()` + `execlp()`
- [x] 行读取/写入（JSON-RPC newline-delimited，select 轮询 + timeout）
- [x] stderr 转发（独立线程 + 环形缓冲区）
- [x] 进程关闭（SIGTERM → WNOHANG 5s → SIGKILL → waitpid）
- [x] 创建 `tests/test_mcp_stdio.nim`
- [x] 更新 `tests/test_runner.nim`

### 4.3 HTTP/Streamable 传输
- **C 对应**：`mcp.c` 中 `internal_tls_connect()`, `internal_http_post()`
- **依赖**：4.1 JSON-RPC, `std/net`, `std/httpclient`
- [ ] 创建 `src/mcp/transport_http.nim`
- [ ] DNS 解析 → `net.getAddrInfo()`
- [ ] TLS socket 连接 → `net.newContext()` + `wrapSocket()`
- [ ] HTTP POST 请求构建 / 响应解析
- [ ] Chunked transfer encoding
- [ ] Bearer token 认证 + 401 处理
- [ ] 创建 `tests/test_mcp_http.nim`
- [ ] 更新 `tests/test_runner.nim`

### 4.4 MCP 客户端核心
- **C 对应**：`mcp.c` 中 `mcp_client_t` 及对外 API
- **依赖**：4.2 stdio 传输, 4.3 HTTP 传输
- [ ] 创建 `src/mcp/client.nim`
- [ ] `McpClientConfig` 配置对象
- [ ] `McpClient` ref object（状态、IO、requestId、锁）
- [ ] `newMcpClient(config): McpClient`
- [ ] `initialize()` — JSON-RPC `initialize` + `notifications/initialized`
- [ ] `callTool(name, arguments): McpCallToolResult`
- [ ] `listTools(): seq[McpTool]`
- [ ] 心跳线程：定期 `ping`，断线自动重连
- [ ] 自动重连（指数退避：`maxReconnect`, `maxReconnectDelay`）
- [ ] 连接状态管理（`McpConnectionState` 枚举）
- [ ] 线程安全（`std/locks`）
- [ ] 创建 `tests/test_mcp_client.nim`（需 mock MCP server）
- [ ] 更新 `tests/test_runner.nim`
- [ ] 参照 `temp/tests/mock_mcp_server.py` 创建 `tests/mock_mcp_server.nim`

### 4.5 MCP Registry（多 server 管理）
- **C 对应**：`temp/include/mcp_registry.h`
- **依赖**：4.4 MCP 客户端
- [ ] 创建 `src/mcp/registry.nim`
- [ ] `McpRegistry` ref object：`Table[string, McpClient]`
- [ ] `loadJsonConfig(configJson: string)`
- [ ] `getClient(name: string): McpClient`
- [ ] 状态回调
- [ ] 错误处理
- [ ] 创建 `tests/test_mcp_registry.nim`
- [ ] 更新 `tests/test_runner.nim`

---

## Phase 5: 组装入口

- [ ] 更新 `src/crown_code.nim` 导入所有功能模块
- [ ] 实现主流程逻辑
- [ ] `make debug` 构建验证
- [ ] `make test` 全部测试通过

---

## Phase 6: 检查测试代码和C测试代码功能是否一致

---

## 完成度追踪

| Phase | 内容 | 依赖 | 预计工时 |
|-------|------|------|----------|
| 1: 基础工具 | context, glob, xdiff | 无 | 中 |
| 2: 搜索系统 | search + json 输出 | 1 | 小 |
| 3: 文件工具 | 10 个子模块 | 1, 2 | 大 |
| 4: MCP 客户端 | 5 个子模块 | 无项目内依赖 | 大 |
| 5: 组装 | main + 验证 | 1-4 | 小 |