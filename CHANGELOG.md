# Changelog

## MCP 客户端核心模块

### Added
- `src/mcp/client.nim`：MCP 客户端核心。公共类型：`McpTransportKind`（mcpStdio/mcpHttp）、`McpConnectionState`（5 状态枚举）、`McpContent`（kind/text/data/mimeType）、`McpCallToolResult`（content/isError）、`McpTool`（name/description）、`McpClientConfig`（transport/command/args/serverUrl/authToken/getToken/refreshToken/timeouts/ping/reconnect/callbacks/protocolVersion/clientName/clientVersion）、`McpClient`（ref object with acyclic 标注，含 config/transportKind/stdio/http/state/lastError/requestIdCounter/initialized/heartbeatThread/heartbeatRunning/stateLock/transportLock）。公共 API：`newMcpClient`（传输初始化 + initialize 握手 + 心跳线程启动）、`callTool`（tools/call 请求，返回 McpCallToolResult）、`listTools`（tools/list 请求，返回 seq[McpTool]）、`getState`（线程安全状态查询）、`getLastError`（线程安全错误查询）、`destroyMcpClient`（清理）。内部实现：`sendJsonRpc`（双通道写/读，HTTP 401 → refreshToken → retry 流程）、`sendNotification`（无响应通知）、`initialize`（initialize + notifications/initialized）、`reconnect`（指数退避 1s→2s→4s→...→60s 上限，最多 3 次）、`heartbeatProc`（可配置间隔 ping 线程，失败触发 disconnect 回调 → reconnect → reconnect 回调）。线程安全：`sendJsonRpc`/`sendNotification` 使用 `transportLock` 串行化传输访问，`destroyMcpClient` 先关闭传输 FD 解除 I/O 阻塞再 joinThread 防止死锁
- `tests/mock_mcp_server.nim`：Nim 编写的 stdin/stdout JSON-RPC 2.0 模拟 MCP 服务器，支持 initialize/tools/list（3 工具）/tools/call（7 工具场景：echo/add/greet/image_tool/error_tool/empty_tool/unknown_tool）/ping，未知方法返回 -32601 错误
- `tests/test_mcp_client.nim`：5 套件 17 个测试用例，覆盖 Null handling（6 测试）、Error state（4 测试）、Default values（2 测试）、Mock server integration（7 测试 + binary 不存在时 skip）、Heartbeat lifecycle（1 测试）

### Changed
- `Makefile`：新增 `MOCK_SERVER` 构建规则，`test` 目标依赖 `$(MOCK_SERVER)` 确保 mock server 二进制可用
- `src/mcp/transport_http.nim`：`bearerToken` 字段改为导出（`*`），供 client.nim 在 401 retry 时更新 token
- `src/mcp/transport_stdio.nim`：`remainingMs` 改用 `inMilliseconds()` API 替代手动 cast 除法
- `tests/test_runner.nim`：注册 `test_mcp_client` 测试模块

### Bug Fixes
- `src/mcp/client.nim`：`destroyMcpClient` 先关闭传输 FD 再 joinThread，防止心跳线程阻塞在 I/O 时死锁；`sendJsonRpc`/`sendNotification` 新增 `transportLock` 互斥，防止应用线程与心跳线程同时操作同一组 FD 导致竞态条件；`initialize` 中 `lastError` 写入增加 `stateLock` 保护

- Affected files: `src/mcp/client.nim`, `tests/mock_mcp_server.nim`, `tests/test_mcp_client.nim`, `Makefile`, `src/mcp/transport_http.nim`, `src/mcp/transport_stdio.nim`, `tests/test_runner.nim`

## SSE 流式响应解析模块

### Added
- `src/mcp/sse.nim`：W3C Server-Sent Events 协议解析器。公共类型：`SseEvent`（event/data/id）、`SseParser`（ref object with ref count）。公共 API：`newSseParser`、`feed(chunk)`（增量解析，返回 `seq[SseEvent]`）、`flush`（流结束强制输出）、`reset`、`lastEventId`（跨事件持久化）、`reconnectionTime`。协议覆盖：`\n`/`\r\n`/`\r` 三种换行符归一化、BOM 剥离、注释(`:`)忽略、`event`/`data`/`id`/`retry` 字段识别（大小写不敏感，字段冒号后可选空格）、多行 data `\n` 拼接、`id` 含 `\0` 忽略、`retry` 非正整数忽略。依赖 `std/strutils`（仅标准库，无项目内依赖）
- `tests/test_mcp_sse.nim`：33 个测试用例，3 个套件（完整文本解析 18 个 / 流式解析 10 个 / HTTP 集成 5 个），覆盖单事件、多事件、字段类型、多行 data、空白 data 行、注释、BOM、无 data 行不触发、null id、无效 retry、未知字段忽略、CRLF/CR 换行符、trailing space retry、冒号事件名、前置空格 data、分块流式解析（跨 chunk 和跨行切分）、flush 残留事件、reset 重置、lastEventId 跨事件持久化、parseHttpResponse 集成解析 SSE HTTP 响应体

### Changed
- `src/mcp/transport_http.nim`：`HttpResponse` 新增 `events*: seq[SseEvent]` 字段（零值 `@[]`，向下兼容）；新增 `SSE_READ_TIMEOUT_MS` 常量（120s）；`postJson` 内集成分支：检测 `Content-Type: text/event-stream` 后调用 `readSseResponse`（`waitReadable` + `recv` 字节块 + `SseParser.feed`，非 `recvLine`），chunked SSE 返回错误；新增 `import mcp/sse`、`std/monotimes`、`std/times`
- `tests/test_runner.nim`：注册 `test_mcp_sse` 测试模块
- `TODO.md`：Phase 4.3 SSE 流式响应 标记完成

- Affected files: `src/mcp/sse.nim`, `src/mcp/transport_http.nim`, `tests/test_mcp_sse.nim`, `tests/test_runner.nim`, `TODO.md`

## MCP HTTP 传输层模块

### Added
- `src/mcp/transport_http.nim`：MCP HTTP/Streamable 传输层基础。依赖 `std/net`、`std/strutils`、`std/tables`、`std/uri`、`std/posix`。公共类型：`HttpTransport`（ref object，含 socket/host/port/tls/basePath/bearerToken/connected/lastError）、`HttpResponse`（statusCode/headers/body/error）。公共 API：`newHttpTransport`（URL 解析，默认端口 http:80 https:443）、`connect`（TCP → TLS wrapConnectedSocket 握手）、`close`（nil-safe socket 关闭）、`postJson`（构建 HTTP/1.1 POST 请求，header 大小写不敏感查找，Transfer-Encoding 优先 Content-Length）、`isConnected`。内部实现：`buildHttpRequest`、`parseHttpResponse`（状态行解析含无 reason phrase 场景，Content-Type charset 前缀匹配）、`readChunkedBody`（chunk-ext 剥离，hex 大小写混合解析，trailer headers 处理，字节精确读取）、`readFixedBody`（recv 循环读至满 Content-Length 字节）。常量：`DEFAULT_HTTP_TIMEOUT_MS`（30s）、`MAX_RESPONSE_SIZE`（10MB）
- `tests/test_mcp_http.nim`：6 个套件约 28 个测试用例，覆盖 URL 解析、HTTP 请求构建、响应解析（含无 reason phrase/charset/大小写混合 header）、header 大小写不敏感、chunked 解码（Transfer-Encoding 优先规则）、连接生命周期
- `TODO.md`：Phase 4.3 完成，追加 SSE 流式响应 TODO
- `config.nims`：添加 `switch("define", "ssl")` 启用 OpenSSL 支持

### Changed
- `tests/test_runner.nim`：注册 `test_mcp_http` 测试模块

- Affected files: `src/mcp/transport_http.nim`, `tests/test_mcp_http.nim`, `tests/test_runner.nim`, `TODO.md`, `config.nims`

## stdio 传输层模块

### Added
- `src/mcp/transport_stdio.nim`：MCP stdio 传输层。手动 `fork()` + `pipe()` × 3 + `dup2()` + `execlp()` 管理子进程（不使用 `osproc.startProcess`，确保 fd 完全所有权）。核心 API：`startStdioTransport(command, args)`（返回 `StdioTransport` ref object，包含 readFd/writeFd/stderrFd/childPid/stderrBuf），`readJsonLine(t, timeoutMs)`（select 轮询 + monotonic deadline 实现超时，逐字节读取跳过 `\r`，遇 `\n` 返回，超时返回 `teTimeout`，EOF 返回 `teEof`），`writeJsonLine(t, line, timeoutMs)`（追加 `\n` 后 write，EINTR 重试），`close(t)`（SIGTERM → WNOHANG 循环 5 秒 → SIGKILL → waitpid 回收 + 关闭所有 fd），`getStderr(t)`（返回环形缓冲区中缓存的 stderr 内容）。公共常量：`MCP_LINE_BUF_SIZE`（1MB）、`DEFAULT_LINE_TIMEOUT_MS`（30 秒）
- `tests/test_mcp_stdio.nim`：8 个测试用例，覆盖空命令错误、进程启动（true）、超时读取、nil 关闭、资源清理、进程强制终止（sleep 10）

### Changed
- `tests/test_runner.nim`：注册 `test_mcp_stdio` 测试模块

- Affected files: `src/mcp/transport_stdio.nim`, `tests/test_mcp_stdio.nim`, `tests/test_runner.nim`

## JSON-RPC 通信层模块

### Added
- `src/mcp/jsonrpc.nim`：JSON-RPC 2.0 通信层基础模块。依赖 `std/json`（无项目内依赖）。公共 API：`buildRequest`（构建请求字符串，含 `jsonrpc`/`method`/`params`/`id` 字段，`params` 为 `newJNull()` 时省略该字段）、`buildNotification`（构建通知字符串，无 `id` 字段，`params` 省略规则同上）、`parseResponse`（反序列化响应 JSON，仅做语法解析，不校验 `jsonrpc` 版本或 `id` 匹配）。所有构建函数使用 `%*` 宏类型安全构造 JSON
- `tests/test_mcp_jsonrpc.nim`：14 个测试用例，3 个套件（buildRequest / buildNotification / parseResponse），覆盖正常参数、null params 省略、空 object/array 保留 params、method 特殊字符转义、valid result/error 响应、空对象/数组 JSON、空字符串/非法 JSON 异常

### Changed
- `tests/test_runner.nim`：注册 `test_mcp_jsonrpc` 测试模块

- Affected files: `src/mcp/jsonrpc.nim`, `tests/test_mcp_jsonrpc.nim`, `tests/test_runner.nim`

## 文件内容搜索模块

### Added
- `src/search_files.nim`：文件内容正则搜索模块。依赖 `search`（正则搜索）、`glob`（文件名过滤）、`ignore_rules`（clineignore 检测）、`pathutils`（路径处理）。公共类型：`SearchFilesError`（枚举，sfeSuccess/sfeNullParam/sfeDirNotFound/sfeRegexError）、`SearchFilesResult`（results/matchCount/error/errorMessage）。公共常量：`MAX_SEARCH_DEPTH`（10）、`MAX_SEARCH_OUTPUT`（256KB）、`MAX_CONTEXT_LINES`（1）。核心流程：`searchFiles`（参数校验 → 正则编译 → `absolutePath` 标准化根路径 → 调用 `searchDir` 递归搜索）。内部 proc `searchDir`（深度 ≥ MAX_SEARCH_DEPTH 返回 → `walkDir` 遍历 → glob 过滤条目名 → clineignore 检查 → 目录递归 / 文件调用 `searchFile`）。内部 proc `searchFile`（`readFile` 读取 → `matchAll` 获取所有匹配 → 循环输出：`{rel_path}\n│----\n` 头部 → 前一行上下文（matchLine > 1 时 `getLine(matchLine-1)`）→ `│{match_line}\n` → 后一行上下文（`getLine(matchLine+1)`）→ `│----\n` 尾部 → 截断检查追加 `[Results truncated...]`）
- `tests/test_search_files.nim`：19 个测试用例，6 个套件（error handling / directory based tests / depth limiting / truncation / output format），覆盖空参数/nullParam、目录不存在/dirNotFound、无效正则/regexError、单文件单/多匹配、跨文件匹配、glob 文件名过滤、上下文行显示（前后各 1 行 + 边界限幅）、空文件、clineignore 过滤、深度限制（12 层嵌套，depth=0→10 共 11 层可达，第 12 层不可达）、256KB 输出截断（含 `[Results truncated...]` 标记）、输出格式（`│` 前缀 + `│----\n` 分隔符）

### Changed
- `tests/test_runner.nim`：注册 `test_search_files` 测试模块

- Affected files: `src/search_files.nim`, `tests/test_search_files.nim`, `tests/test_runner.nim`

## 目录列表模块

### Added
- `src/list_files.nim`：目录列表模块。依赖 `pathutils`（路径解析）和 `ignore_rules`（clineignore 检测）。公共类型：`ListFilesError`（枚举，Success/NullPath/DirNotFound/PermissionDenied/ReadFailed）、`ListFilesResult`（entries/count/didHitLimit/error/errorMessage）。内部 `cmpEntry` 排序比较器（目录优先，同组按字母序）。主入口 `listFiles`（10 步流程：参数验证 → clineignore 目录检查 → 路径解析 → `/` 和 `$HOME` 安全限制返回空结果 → `dirExists` 存在性检查 → `walkDir` 遍历（`relative=true`，跳过 `.`/`..`）→ 逐条目 clineignore 过滤 → 达到 `MAX_LIST_ENTRIES=200` 截断 → `sort` 排序 → 返回结果）。目录不可读时 `try/except CatchableError` 返回 `ReadFailed`
- `tests/test_list_files.nim`：12 个测试用例，4 个套件（error handling / basic functionality / limit / ignore rules），覆盖空路径、不存在路径、根目录安全限制、家目录安全限制、空目录、混合文件/目录列表、目录优先排序、字母序、隐藏文件、特殊字符文件名、200 条目截断、clineignore 文件过滤、clineignore 目录阻止

### Changed
- `tests/test_runner.nim`：注册 `test_list_files` 测试模块

- Affected files: `src/list_files.nim`, `tests/test_list_files.nim`, `tests/test_runner.nim`

## 命令执行模块

### Added
- `src/command_exec.nim`：命令执行模块。依赖 `shell_detect`（Shell 检测）。公共类型：`CommandError`（枚举，ceOk/ceApprovalDenied/ceExecutionFailed/ceTimeout）、`CommandResult`（stdout/stderr/exitCode/executionTime/abnormalExit/error）。公共常量：`MaxFullOutputSize`（1MB 输出截断）、`DefaultTimeoutMs`（300 秒超时）、`CircularBufferSize`（2000 槽）。辅助函数：`trimWhitespace`（两端空白去除）、`splitCommands`（按 `&&`/`||`/`&`/`|`/`;` 拆分子命令）、`requiresApproval`（审批检查存根，始终返回 `true`）。`CircularBuffer` 环形缓冲区（`initCircularBuffer`/`pushCircularBuffer`/`joinCircularBuffer`，线程安全锁定）。核心 `execCommand`（10 步流程：trim → splitCommands → 黑名单审批检查 → `detectShells` 获取 shell → `startProcess` 启动子进程（POSIX: `bash -l -c`，Windows: `cmd.exe /c`）→ 双线程流式读取 stdout/stderr → `waitForExit` 带超时 → 超时 `terminate`+`kill` → 拼接输出 → 执行时间统计）
- `tests/test_command_exec.nim`：27 个测试用例，5 个套件（trimWhitespace / splitCommands / requiresApproval / CircularBuffer / execCommand），覆盖各种分隔符拆分、缓冲区溢出覆盖、echo 输出、exit code、stderr 捕获、空/空白命令、执行时间测量、黑名单审批检查、命令未找到处理

### Changed
- `tests/test_runner.nim`：注册 `test_command_exec` 测试模块

- Affected files: `src/command_exec.nim`, `tests/test_command_exec.nim`, `tests/test_runner.nim`

## Shell 检测模块

### Added
- `src/shell_detect.nim`：Shell 检测模块。无项目内依赖。公共类型：`ShellInfo`（name/path/found）。核心 proc `detectShells*(): seq[ShellInfo]`，POSIX 分支读取 `$SHELL` 环境变量并用 `extractFilename` 提取 basename，Windows 分支在 PATH 中搜索 `bash.exe`/`pwsh.exe`/`powershell.exe`/`cmd.exe`（`findExe`），bash 额外检查 5 个已知安装路径
- `tests/test_shell_detect.nim`：6 个测试用例，2 个套件（basic detection / common shells），覆盖非空结果、名称非空、路径非空、found 标记、路径存在性、常见 shell 检测（bash/zsh/sh/fish）

### Changed
- `tests/test_runner.nim`：注册 `test_shell_detect` 测试模块

- Affected files: `src/shell_detect.nim`, `tests/test_shell_detect.nim`, `tests/test_runner.nim`

## 代码格式化模块

### Added
- `src/formatter.nim`：文件代码格式化模块。依赖 `pathutils`（路径解析）。公共类型：`FormatterError`（枚举，Success/NullPath/ReadFailed/MemoryAlloc）、`FormatterResult`（error/errorMessage）。内部 `processContent`（逐字符迭代，行尾空白修剪 + 行首空白规范化：含制表符替换为 4 空格、只有空格完全移除，保留原有空行结构）。主入口 `formatFile`（6 步流程：参数验证 → 路径解析 → 文件读取 → processContent → 文件覆盖写入 → 返回结果）
- `tests/test_formatter.nim`：10 个测试用例，2 个套件（error handling / content formatting），覆盖空路径、文件不存在、行尾空格修剪、行首 Tab 替换、混合空白处理、空格+Tab 混合、纯空格行首移除、无末尾换行保留、空文件

### Changed
- `tests/test_runner.nim`：注册 `test_formatter` 测试模块

- Affected files: `src/formatter.nim`, `tests/test_formatter.nim`, `tests/test_runner.nim`

## 文件编辑模块

### Added
- `src/file_edit.nim`：文件行级精确替换模块。依赖 `pathutils`（路径解析）、`ignore_rules`（clineignore 检测）、`file_writer`（文件写入）。公共类型：`FileEditError`（枚举，Success/FileNotFound/OldStringNotFound/MultipleMatches/ReadFailed/WriteFailed）、`FileEditResult`（error/errorMessage/matchCount）。内部辅助函数 `splitIntoLines`（按 `\n` 拆分，保留末尾空行）和 `joinLines`（行间插入 `\n`，末尾不加 `\n`）。主入口 `editFile`（10 步流程：路径解析 → clineignore → 文件读取 → 按行拆分 → 精确匹配计数 → 未找到/多次匹配检测 → 行替换 → 按行合并 → 写入文件 → 返回结果）。`editFile` 的 `multiple` 参数控制是否替换所有匹配行
- `tests/test_file_edit.nim`：15 个测试用例，5 个套件（error handling / basic functionality / edge cases / access control），覆盖空路径、文件不存在、未找到 oldStr、多次匹配、单次精确替换、全部替换、首行/末行/单行替换、空 oldStr 匹配空行、newStr 含换行、尾随换行符、空文件、clineignore 拦截

### Changed
- `tests/test_runner.nim`：注册 `test_file_edit` 测试模块

- Affected files: `src/file_edit.nim`, `tests/test_file_edit.nim`, `tests/test_runner.nim`

## 文件写入模块

### Added
- `src/file_writer.nim`：文件写入与缓存失效模块。依赖 `pathutils`（路径解析）、`ignore_rules`（clineignore 检测）和 `file_reader`（`cacheInvalidate`）。公共类型：`FileWriterError`（枚举，Success/NullPath/FileNotFound/PermissionDenied/WriteFailed）、`FileWriterResult`（error/errorMessage）。主入口 `writeFileContent`（6 步流程：参数验证 → clineignore → 路径解析 → 文件写入 → 缓存失效 → 返回成功）。`writeFileContent` 使用 `content: string = ""` 默认参数，对应 C 的 NULL content 处理
- `tests/test_file_writer.nim`：8 个测试用例，5 个套件（error handling / basic functionality / caching / access control / write failure），覆盖空路径、文件写入、空内容写入、缓存失效验证、clineignore 拦截、不存在目录写入、只读目录写入、重复写入缓存行为

### Changed
- `tests/test_runner.nim`：注册 `test_file_writer` 测试模块

- Affected files: `src/file_writer.nim`, `tests/test_file_writer.nim`, `tests/test_runner.nim`

## 文件读取模块

### Added
- `src/file_reader.nim`：文件读取与格式化输出模块。依赖 `pathutils`（路径解析）和 `ignore_rules`（clineignore 检测）。公共类型：`FileReaderError`（枚举，SUCCESS/NULL_PATH/FILE_NOT_FOUND/PERMISSION_DENIED/READ_FAILED/MEMORY_ALLOC）、`LineRange`（startLine/endLine/totalLines/truncated）、`FileReaderResult`（content/range/error/errorMessage）、`FileCacheEntry`。缓存子系统：256 槽位哈希表（大小写不敏感 hash），`cacheGet`/`cacheSet`/`cacheInvalidate` 三接口。辅助函数：`getFileMtime`（`getLastModificationTime` 封装）、`countLines`（`\n` 计数）、`parseLineRange`（范围解析与自动交换）、`formatContentWithLineNumbers`（`行号 | 内容` 格式化 + 尾部文件统计）。主入口 `readFileRange`（12 步流程：参数验证 → clineignore → 路径解析 → 缓存 + mtime 检测 → 重复读取警告([File already read]/[DUPLICATE READ]) → 文件读取 → 行统计 → 格式化 → 拼接输出）
- `tests/test_file_reader.nim`：15 个测试用例，5 个套件（error handling / basic functionality / caching / large files / path resolution），覆盖 null 路径、空路径、文件不存在、基础行号格式化、行范围读取、范围交换、超出 EOF、单行文件、缓存重复读取警告（3 次语义）、mtime 变化驱逐、大文件（2000 行）、绝对路径、相对路径

- Affected files: `src/file_reader.nim`, `tests/test_file_reader.nim`, `tests/test_runner.nim`

## clineignore 忽略规则模块

### Added
- `src/ignore_rules.nim`：导入 `pathutils` 和 `glob`，依赖 3.1 路径解析。导出 3 个公共 API：`loadIgnoreFile`（读取 `.clineignore` 文件，跳过空行/`#` 注释，去除尾部空白）、`resetIgnoreRules`（测试用重置）、`checkIgnorePath`（主入口：绝对路径转相对 → `fnmatchPathname` 匹配 → 无 `/` 的 pattern 加 `*/` 前缀 → 全局规则 > 项目规则）。采用全局懒初始化状态（`initIgnoreRules`），与 C 代码一致
- `tests/test_ignore_rules.nim`：12 个测试用例，2 个套件（loadIgnoreFile / checkIgnorePath），覆盖非存在文件、注释跳过、尾部空白去除、空行跳过、简单 glob 匹配、`*/` 前缀子目录匹配（一级）、含 `/` pattern 精确路径匹配、`!` 否定规则、空路径安全、绝对路径转相对路径
- `src/glob.nim` 新增 `fnmatchPathname`（FNM_PATHNAME 语义：`*`/`?`/`[...]` 不匹配 `/`，`*` 回溯不跨越路径分隔符）和 `matchGlobPathname`（基于 `fnmatchPathname`）
- `tests/test_glob.nim` 新增 fnmatchPathname 测试套件（10 个测试用例），覆盖 star 不匹配 `/`、`*/` 前缀单级子目录、`?` 不匹配 `/`、段内 `*` 匹配、字符类不匹配 `/`、否定、精确路径、尾部 `*`、空 pattern、`*` 匹配空字符串

### Changed
- `tests/test_runner.nim`：注册 `test_ignore_rules` 测试模块

- Affected files: `src/glob.nim`, `src/ignore_rules.nim`, `tests/test_glob.nim`, `tests/test_ignore_rules.nim`, `tests/test_runner.nim`

## 路径解析模块

### Added
- `src/pathutils.nim`：三个路径解析 proc：`resolveWorkspacePath`（相对/绝对路径解析）、`toRelPath`（相对路径计算，反斜杠归一化为正斜杠）、`resolvePath`（返回 `(absolutePath, displayPath)` 元组）
- `tests/test_pathutils.nim`：14 test cases, 4 suites（normalizeSlashes / resolveWorkspacePath / toRelPath / resolvePath），covers separator normalization, CWD prefix stripping, backslash conversion, custom cwd parameter

### Changed
- `tests/test_runner.nim`：注册 `test_pathutils` 测试模块

- Affected files: `src/pathutils.nim`, `tests/test_pathutils.nim`, `tests/test_runner.nim`

## JSON 搜索输出格式化模块

### Added
- `src/search_json.nim`：JSON 搜索输出格式化，导出 4 个公共 API：`jsonEscape`（JSON 字符串转义，处理 `"`/`\`/`\n`/`\t`/`\r`）、`formatStartJson`（搜索起始标记）、`formatEndJson`（搜索结束标记）、`formatMatchJson`（匹配结果 JSON，支持 `context_before`/`context_after` 数组）
- `tests/test_search_json.nim`：25 个测试用例，4 个套件（jsonEscape / formatStartJson / formatEndJson / formatMatchJson），覆盖特殊字符转义、上下文输出、nil 安全

### Changed
- `tests/test_runner.nim`：注册 `test_search_json` 测试模块

- Affected files: `src/search_json.nim`, `tests/test_search_json.nim`, `tests/test_runner.nim`

## Search 正则搜索模块

### Added
- `src/search.nim`：使用 `std/re`（PCRE 封装）替代 C 的 PCRE2 FFI，导出 5 个公共 API：`newSearch`（编译正则，支持 `soCaseInsensitive`/`soMultiLine`/`soDotAll` 选项）、`matchFirst`（单次匹配，返回 `Option[Match]`）、`matchAll`（全部匹配，返回 `seq[Match]`）、`calcLineNumber`（偏移量 → 1-based 行号）、`getLine`（行号 → 行内容）
- `tests/test_search.nim`：29 个测试用例，7 个套件（newSearch / matchFirst / matchAll / calcLineNumber / getLine / options），覆盖无效正则、偏移匹配、跨行匹配、选项标志、边界情况

### Changed
- `tests/test_runner.nim`：注册 `test_search` 测试模块

- Affected files: `src/search.nim`, `tests/test_search.nim`, `tests/test_runner.nim`

## XDiff Unified Diff 引擎

### Added
- `src/xdiff.nim`：基于 `experimental/diff.diffText`（Myers O(ND)）的 unified diff 引擎，导出 `diff*` 公共 API（`diff*(a, b: string; ctxLen: int = 3): string`）。支持上下文窗口合并（间距 ≤ 2×ctxLen）、0 计数 hunk header（pure addition/deletion）、`\ No newline at end of file` 内联标记、尾部换行符差异检测
- `tests/test_diff.nim`：19 个测试用例，5 个套件（basic / single change / context window / edge cases），覆盖空输入、换行符边界、上下文窗口、hunk 合并、header 格式

### Changed
- `tests/test_runner.nim`：注册 `test_diff` 测试模块

- Affected files: `src/xdiff.nim`, `tests/test_diff.nim`, `tests/test_runner.nim`

## Glob 通配符匹配模块

### Added
- `src/glob.nim`：手动实现 fnmatch 算法（支持 `*`/`?`/`[...]` 回溯匹配），导出 `matchGlob`（单模式，`!` 前缀取反）和 `matchAnyGlob`（多模式，`!` 否定优先短路）
- `tests/test_glob.nim`：32 个测试用例，覆盖通配符、字符类、否定义前缀、多模式组合、边界情况

### Changed
- `tests/test_runner.nim`：注册 `test_glob` 测试模块

- Affected files: `src/glob.nim`, `tests/test_glob.nim`, `tests/test_runner.nim`

## Context 上下文缓冲模块

### Added
- `src/context.nim`：`Context` ref object 类型，提供 `newContext`、`addLine`、`clearContext` 三个 proc
- `tests/test_context.nim`：5 个测试用例，覆盖 create / edge cases / add line / reset / nil safety

### Changed
- `tests/test_runner.nim`：注册 `test_context` 测试模块

- Affected files: `src/context.nim`, `tests/test_context.nim`, `tests/test_runner.nim`
