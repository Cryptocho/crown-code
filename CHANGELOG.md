# Changelog

## Agent Loop 核心模块

### Added
- `core/src/agent/tools.rs`：工具定义与执行调度模块（Rust 重写）。7 个工具（read_file/write_to_file/replace_in_file/execute_command/search_files/list_files/attempt_completion），OpenAI function calling JSON Schema 格式（`serde_json::json!` 宏）。公共 API：`get_tool_definitions() -> Vec<Tool>`（返回 7 个 Tool 定义，每个含 name/description/parameters）、`execute_tool(name: &str, args: &Value) -> String`（按名称 match 分发执行，attempt_completion 返回 `[COMPLETION]` 前缀标记）。错误处理：必需参数缺失返回 `"Error: ..."`，底层模块异常通过 error enum 转换为可读消息。输出格式：execute_command（`STDOUT:\n`/`STDERR:\n`/Exit code/abnormal exit/execution time）、list_files（`  ` 前缀 + `entries` 计数 + truncation 标记）、search_files（结果 + matches found 计数）
- `core/src/agent/prompt.rs`：System prompt 构建模块（Rust 重写）。公共 API：`build_system_prompt(cwd: &str) -> String`（生成包含角色描述、TOOL USE、AVAILABLE TOOLS 列表、RULES、SYSTEM INFORMATION 五个 section 的完整 prompt）。复用 `crate::shell_detect::detect_shells()` 检测默认 shell，使用 `std::env::consts::OS` 检测操作系统
- `core/src/agent/loop.rs`：Agent Loop 核心调度模块（Rust 重写）。公共 API：`run_agent_loop(config: ApiClientConfig)`（双层 while 循环：外层读取 stdin 用户输入，内层流式 API 调用→工具执行→结果反馈）。流式回调使用闭包 `impl FnMut(ApiStreamChunk) -> bool`，`use std::io::Write` 用于 `stdout.flush()`。支持 `[TOOL_CALL]`/`[TOOL_RESULT]`/`[PROMPT]` stderr 日志。JSON 解析失败处理：`serde_json::from_str` 捕获无效参数，记录错误到 history 并 `continue` 跳过执行
- `core/src/agent/mod.rs`：模块导出 `pub mod tools; pub mod prompt; pub mod r#loop;`（`r#loop` raw identifier 避免 Rust 关键字冲突）
- 测试：`agent/tools.rs` 31 个单元测试（工具定义 9 个 + 错误处理 9 个 + 基本功能 11 个 + replace_in_file 3 个），`agent/prompt.rs` 8 个单元测试（section 存在性 + 顺序）

### Changed
- `core/src/lib.rs`：新增 `pub mod agent;` 导出
- `core/src/main.rs`：从占位符 `println!("crown-core: ready")` 替换为调用 `run_agent_loop`，使用 ollama 默认配置（baseUrl: localhost:11434/v1, model: gemma4:e4b, temperature: 0.0, maxTokens: 4096）

- Affected files: `core/src/agent/tools.rs`, `core/src/agent/prompt.rs`, `core/src/agent/loop.rs`, `core/src/agent/mod.rs`, `core/src/lib.rs`, `core/src/main.rs`, `core/src/api/openai.rs`

## MCP Registry 多 server 管理模块

### Added
- `core/src/mcp/registry.rs`：MCP Registry 多 server 管理模块（Rust 重写）。公共类型：`McpRegistryError`（Ok/ServerNotFound/ServerDisabled/NotConnected/ConfigError）、`McpServerConfig`（transport/command/args/serverUrl/authToken/enabled）、`McpStatusCallback`（Box<dyn Fn> 类型）。公共 API：`McpRegistry::new`（构造函数）、`destroy`（先销毁所有 client→清空配置→清空回调）、`load_json_config`（JSON 解析，验证 transport/command/url/enabled）、`get_client`（懒创建+缓存复用，Weak/Arc 回调桥接替代 cast[pointer]）、`set_status_callback`、`server_names`、`server_count`、`last_error`。回调桥接：`Weak<Mutex<RegistryInner>>` + 闭包捕获
- 测试：30 个测试用例，9 个套件（Nil safety 7 / Config parsing 11 / Server names 2 / Server count 2 / Get client 4 / Status callback 1 / Error handling 1 / Lifecycle 2）

### Changed
- `core/src/mcp/mod.rs`：新增 `pub mod client; pub mod registry;` 导出

- Affected files: `core/src/mcp/registry.rs`, `core/src/mcp/mod.rs`

## MCP 客户端核心模块

### Added
- `core/src/mcp/client.rs`：MCP 客户端核心（Rust 重写）。公共类型：`McpTransportKind`（Stdio/Http）、`McpConnectionState`（5 状态枚举）、`McpContent`、`McpCallToolResult`、`McpTool`、`McpClientConfig`、`McpClient`。公共 API：`McpClient::new`、`call_tool`、`list_tools`、`state`、`last_error`、`destroy`。内部实现：`send_json_rpc`（双 Mutex 串行化写/读，HTTP 401 → refresh_token → retry）、`send_notification`、`initialize`、`reconnect`（指数退避 1s→2s→4s→60s 上限，最多 3 次）、`heartbeat_proc`（AtomicBool + 100ms polling loop）。线程安全：`ClientInner` 内 `transport_lock` 串行化传输访问，`destroy` 先关闭传输再 join 心跳线程防止死锁。回调：`Option<Box<dyn Fn() + Send + Sync>>`
- `core/src/bin/mock_mcp_server.rs`：Rust 重写的 stdin/stdout JSON-RPC 2.0 模拟 MCP 服务器（~70 行）。覆盖 initialize / tools/list（3 工具）/ tools/call（7 个场景：echo/add/greet/image_tool/error_tool/empty_tool/unknown_tool）/ ping，未知方法返回 -32601
- `core/build.rs`：构建脚本，支持 mock 服务器二进制路径暴露
- 测试：17 个测试用例，5 个套件（Null handling 4 / Error state 4 / Default values 2 / Mock server integration 8 / Heartbeat lifecycle 1）

### Changed
- `core/src/mcp/mod.rs`：新增 `pub mod client;` 导出

- Affected files: `core/src/mcp/client.rs`, `core/src/bin/mock_mcp_server.rs`, `core/build.rs`, `core/src/mcp/mod.rs`

## OpenAI Compatible API 模块

### Added
- `core/src/api/types.rs`：OpenAI Compatible API 核心类型定义（Rust 重写）。公共类型：`MessageRole`（System/User/Assistant/Tool/Developer，带 Display/FromStr）、`Message`（role/content/tool_calls/tool_call_id/name，带 `to_json_value` 处理 content null/tool_call_id 边缘情况）、`Tool`（name/description/parameters）、`ToolCall`（id/function_name/arguments/tc_index）、`ApiStreamChunk`（Rust enum：Text/Reasoning/Usage/ToolCall/Done）、`ApiError`（code/message）、`ApiUsage`（input_tokens/output_tokens/cache_read_tokens）、`ApiResponse`（content/tool_calls/usage/error/finish_reason）、`ApiClientConfig`（base_url/api_key/model/temperature/max_tokens/stream_options）、`ApiClient`（config/http: HttpTransport）
- `core/src/api/openai.rs`：OpenAI Compatible API 客户端（Rust 重写）。公共 API：`new_client`（工厂函数，自动追加 `/chat/completions`）、`build_chat_request`（非流式 JSON 请求体构建，含 messages/tools/stream/temperature/max_tokens）、`parse_chat_response`（非流式 JSON 响应解析，含 error/choices/tool_calls/usage/finish_reason）、`create_message`（非流式请求全流程：buildChatRequest→postJson→parseChatResponse）、`parse_stream_event`（单行 SSE data 解析，返回 Vec<ApiStreamChunk>，支持 text/reasoning/tool_calls/usage/DONE/error/JSON parse error 七种场景）、`create_message_stream`（流式请求全流程：buildChatRequest(stream:true)→postJsonStream→tool call delta 累积→返回 ApiResponse）。tool call delta 累积：`HashMap<i32, ToolCall>` 按 index 并行累积 id/function_name/arguments
- 测试：51 个单元测试 + 4 个集成测试（`#[ignore]`，需要 OPENROUTER_API_KEY）。类型测试 20 个（MessageRole/Message/Tool/ToolCall/ApiStreamChunk/ApiResponse/ApiClientConfig 构造），非流式 API 测试 16 个（buildChatRequest JSON 结构/messages 转换/tools 字段/parseChatResponse 正常/异常/空场景），流式 API 测试 15 个（parseStreamEvent 各 variant/SSE 注释忽略/单 tool call 跨 chunk 累积/多 tool call 并行累积）

### Changed
- `core/src/api/mod.rs`：新建，导出 `pub mod types; pub mod openai;`
- `core/src/lib.rs`：新增 `pub mod api;`

- Affected files: `core/src/api/types.rs`, `core/src/api/openai.rs`, `core/src/api/mod.rs`, `core/src/lib.rs`

## SSE 流式响应解析模块

### Added
- `core/src/mcp/sse.rs`：W3C Server-Sent Events 协议解析器。公共类型：`SseEvent`（event/data/id）、`SseParser`（增量解析状态机）。公共 API：`SseParser::new`、`feed`（增量解析，返回 `Vec<SseEvent>`）、`flush`（流结束强制输出）、`reset`、`last_event_id`、`reconnection_time`。协议覆盖：`\n`/`\r\n`/`\r` 三种换行符归一化、BOM 剥离、注释(`:`)忽略、`event`/`data`/`id`/`retry` 字段识别（大小写不敏感，字段冒号后可选空格）、多行 data `\n` 拼接、`id` 含 `\0` 忽略、`retry` 非正整数忽略。无项目内依赖
- 测试：30 个测试用例，覆盖完整文本解析（23 个）、流式解析（7 个）

### Changed
- `core/Cargo.toml`：新增 `serde`、`serde_json`、`reqwest`、`libc` 依赖
- `core/src/lib.rs`：新增 `pub mod mcp` 模块导出

- Affected files: `core/src/mcp/sse.rs`, `core/src/mcp/mod.rs`, `core/src/lib.rs`, `core/Cargo.toml`

## MCP HTTP 传输层模块

### Added
- `core/src/mcp/transport_http.rs`：MCP HTTP/Streamable 传输层。使用 `reqwest::blocking::Client` 。公共类型：`HttpResponse`（status_code/headers/body/error/events）、`HttpTransport`（base_url/bearer_token/client/connected/last_error）。公共常量：`DEFAULT_HTTP_TIMEOUT_MS`（30s）、`MAX_RESPONSE_SIZE`（10MB）、`SSE_READ_TIMEOUT_MS`（120s）。公共 API：`HttpTransport::new`（URL 解析 + reqwest client 构建）、`is_connected`、`close`、`post_json`（自动检测 SSE 响应 → `SseParser::feed` / 普通 JSON 响应）、`post_json_stream`（流式 SSE POST，闭包回调中止支持）。依赖 `reqwest`（内置 HTTP/1.1、chunked 解码、TLS、超时管理）+ `mcp::sse`
- 测试：7 个测试用例，覆盖 URL 解析、连接生命周期、close 幂等、post_json/post_json_stream 错误响应、SSE 检测、bearer token 存储

- Affected files: `core/src/mcp/transport_http.rs`, `core/src/mcp/mod.rs`, `core/src/lib.rs`, `core/Cargo.toml`

## stdio 传输层模块

### Added
- `core/src/mcp/transport_stdio.rs`：MCP stdio 传输层。使用 `std::process::Command`。核心 API：`start_stdio_transport(command, args)`（返回 `StdioTransport`，含 child/stdin/stdout/stderr_buf/stderr_thread），`read_json_line(t, timeout_ms)`（`libc::poll` 轮询 + `BufReader::read_line` 实现超时，Timeout 返回 `TransportError::Timeout`），`write_json_line(t, line)`（追加 `\n` 后 write_all + flush），`close(t)`（SIGKILL + wait + join stderr 线程），`get_stderr(t)`（`CircularBuffer::join` 返回 stderr 缓存）。公共常量：`MCP_LINE_BUF_SIZE`（1MB）、`DEFAULT_LINE_TIMEOUT_MS`（30 秒）。stderr 线程：`std::thread::spawn` + `BufReader::lines` → `CircularBuffer::push`
- 测试：6 个测试用例，覆盖空命令错误、进程启动（true）、超时读取、close 两次安全、资源清理、进程强制终止

- Affected files: `core/src/mcp/transport_stdio.rs`, `core/src/mcp/mod.rs`, `core/src/lib.rs`, `core/Cargo.toml`

## JSON-RPC 通信层模块

### Added
- `core/src/mcp/jsonrpc.rs`：JSON-RPC 2.0 通信层基础模块。使用 `serde_json`。公共 API：`build_request(method, params, id)`（构建请求字符串，`params` 为 `None` 或 `Null` 时省略 `params` 字段）、`build_notification(method, params)`（构建通知字符串，无 `id` 字段，`params` 省略规则同上）、`parse_response(json_str)`（反序列化 JSON-RPC 响应，仅做语法解析，不校验 `jsonrpc` 版本或 `id` 匹配）。依赖 `serde_json`（无项目内依赖）
- 测试：16 个测试用例，覆盖 buildRequest（6 个）、buildNotification（4 个）、parseResponse（6 个），覆盖正常参数、null params 省略、空 object/array 保留 params、method 特殊字符、valid result/error 响应、空对象/数组 JSON、空字符串/非法 JSON 异常

- Affected files: `core/src/mcp/jsonrpc.rs`, `core/src/mcp/mod.rs`, `core/src/lib.rs`, `core/Cargo.toml`

## 命令执行模块

### Added
- `core/src/command_exec.rs`：命令执行模块。公共类型：`CommandError`（Ok/ApprovalDenied/ExecutionFailed/Timeout）、`CommandResult`（stdout/stderr/exit_code/execution_time/abnormal_exit/error）、`CircularBuffer`（Mutex 保护的线程安全环形缓冲区）。公共常量：`MAX_FULL_OUTPUT_SIZE`（1MB）、`DEFAULT_TIMEOUT_MS`（300s）、`CIRCULAR_BUFFER_SIZE`（2000）。公共 API：`trim_whitespace`（两端空白/tab 去除）、`split_commands`（按 `&&`/`||`/`&|`/`&`/`|`/`;` 拆分，2-char 分隔符优先）、`requires_approval`（始终返回 true）、`exec_command`（10 步流程：trim → split → 黑名单检查 → detect_shells → 子进程启动 → 双线程 BufReader 流式读取 stdout/stderr → 超时轮询 → kill → join 线程 → 拼接输出 → 返回结果）。`CircularBuffer::join` 拼接所有行。超时实现：`child.try_wait()` 轮询 + `child.kill()` 强制终止
- `core/src/lib.rs`：模块导出声明（pub mod command_exec）

- Affected files: `core/src/command_exec.rs`, `core/src/lib.rs`

## 目录列表模块

### Added
- `core/src/list_files.rs`：目录列表模块。依赖 `pathutils`（路径解析）、`ignore_rules`（crownignore 检测）。公共类型：`ListFilesError`（Success/NullPath/DirNotFound/PermissionDenied/ReadFailed）、`ListFilesResult`（entries/count/did_hit_limit/error/error_message）。公共常量：`MAX_LIST_ENTRIES`（200）。主入口 `list_files`（10 步流程：参数验证 → crownignore 检查 → 路径解析 → `/` 和 `$HOME` 安全限制返回空结果 → `dirExists` 存在性检查 → `std::fs::read_dir` 遍历 → 逐条目 crownignore 过滤 → 达到 200 截断 → `sort_by` 排序（目录优先 + 字母序）→ 返回结果）。目录不可读时返回 `ReadFailed`
- `core/src/lib.rs`：模块导出声明（pub mod list_files）

- Affected files: `core/src/list_files.rs`, `core/src/lib.rs`

## 文件内容搜索模块

### Added
- `core/src/search_files.rs`：文件内容正则搜索模块。依赖 `search`（正则搜索）、`glob`（文件名过滤）、`ignore_rules`（crownignore 检测）。公共类型：`SearchFilesError`（Success/NullParam/DirNotFound/RegexError）、`SearchFilesResult`（results/match_count/error/error_message）。公共常量：`MAX_SEARCH_DEPTH`（10）、`MAX_SEARCH_OUTPUT`（256KB）。核心流程：`search_files`（参数校验 → 正则编译 → `std::path::absolute` 标准化根路径 → 调用 `search_dir` 递归搜索）。内部 `search_dir`（深度 ≥ MAX_SEARCH_DEPTH 返回 → `std::fs::read_dir` 遍历 → glob 过滤条目名 → crownignore 检查 → 目录递归 / 文件调用 `search_file`）。内部 `search_file`（`read_to_string` 读取 → `match_all` 获取所有匹配 → 循环输出：`\n{rel_path}\n│----\n` 头部 → 前一行上下文 → `│{match_line}\n` → 后一行上下文 → `│----\n` 尾部 → 截断检查追加 `[Results truncated...]`）。`│` 为 U+2502 盒绘制字符
- `core/src/lib.rs`：模块导出声明（pub mod search_files）

- Affected files: `core/src/search_files.rs`, `core/src/lib.rs`

## Shell 检测模块

### Added
- `core/src/shell_detect.rs`：Shell 检测模块。公共类型：`ShellInfo`（name/path/found）。核心函数 `detect_shells() -> Vec<ShellInfo>`，POSIX 分支读取 `$SHELL` 环境变量，Windows 分支搜索 PATH 中的已知 shell 可执行文件
- `core/src/lib.rs`：模块导出声明（pub mod shell_detect）
- `core/Cargo.toml`：新增 `regex`、`similar` 依赖

### Changed
- `core/src/main.rs`：保留壳程序入口

- Affected files: `core/src/shell_detect.rs`, `core/src/lib.rs`, `core/src/main.rs`, `core/Cargo.toml`

## 代码格式化模块

### Added
- `core/src/formatter.rs`：代码格式化模块。公共类型：`FormatterError`（Success/NullPath/ReadFailed）、`FormatterResult`（error/error_message）。核心函数 `process_content(content)` 逐字符迭代：行尾空白修剪、行首含制表符→4空格、纯空格行首完全移除、保留空行结构。入口 `format_file(path)` 完整流程：参数验证→路径解析→读文件→格式化→写回
- `core/src/lib.rs`：模块导出声明（pub mod formatter）

- Affected files: `core/src/formatter.rs`, `core/src/lib.rs`

## 文件编辑模块

### Added
- `core/src/file_edit.rs`：文件行级精确替换模块。公共类型：`FileEditError`（Success/FileNotFound/OldStringNotFound/MultipleMatches/ReadFailed/WriteFailed）、`FileEditResult`（error/error_message/match_count）。内部函数 `split_into_lines(content)` 按 `\n` 拆分（末尾 `\n` 产生空串行），`join_lines(lines)` 行间 `\n` 连接。主入口 `edit_file(path, old_str, new_str, multiple)` 流程：路径解析→crownignore 检查→读文件→按行拆分→精确匹配计数→未找到/多次匹配错误→替换匹配行→行合并→写入文件（WriteFailed→WriteFailed，其他错误→ReadFailed）。支持单次替换（multiple=false）和全部替换（multiple=true）
- `core/src/lib.rs`：模块导出声明（pub mod file_edit）

- Affected files: `core/src/file_edit.rs`, `core/src/lib.rs`

## 文件写入模块

### Added
- `core/src/file_writer.rs`：文件写入模块。公共类型：`FileWriterError`（Success/NullPath/FileNotFound/PermissionDenied/WriteFailed）、`FileWriterResult`（error/error_message）。主入口 `write_file_content(path, content)` 流程：参数验证→crownignore 检查→路径解析→文件写入→缓存失效。写入后自动调用 `cache_invalidate` 使文件读取缓存失效
- `core/src/lib.rs`：模块导出声明（pub mod file_writer）

- Affected files: `core/src/file_writer.rs`, `core/src/lib.rs`

## 文件读取模块

### Added
- `core/src/file_reader.rs`：文件读取模块。公共类型：`FileReaderError`（Success/NullPath/FileNotFound/PermissionDenied/ReadFailed）、`LineRange`（start_line/end_line/total_lines/truncated）、`FileReaderResult`（content/range/error/error_message）、`FileCacheEntry`（key/read_count/mtime）。全局缓存使用 `Mutex<Vec<FileCacheEntry>>` 256 槽。主入口 `read_file_range(path, start_line, end_line)` 流程：参数验证→crownignore 检查→路径解析→缓存查找+mtime 检测（变化则驱逐）→重复读取警告（2次 `[File already read]`，≥3次 `[DUPLICATE READ]`）→文件读取→行统计+范围解析+边界裁剪→首次读取写入缓存→格式化输出（`行号 | 内容` + 尾部统计）。`LineRange` 自动交换：`end_line < start_line` 时 swap。格式化尾部：全部显示时 `(File has N lines total.)`，截断时 `(Showing lines X-Y of N total. Use start_line=Z to continue reading.)`
- `core/src/lib.rs`：模块导出声明（pub mod file_reader）

- Affected files: `core/src/file_reader.rs`, `core/src/lib.rs`

### TODO
- 三个模块的 crownignore 访问控制测试因 `.crownignore` 需写入 CWD（与 flaky test 修复冲突）未能覆盖，需人工手动测试：创建 `.crownignore` 写入被忽略路径，验证 file_reader 返回 PermissionDenied、file_writer 返回 PermissionDenied、file_edit 返回 ReadFailed

- Affected files: `core/src/file_reader.rs`, `core/src/lib.rs`

## clineignore 忽略规则模块

### Added
- `core/src/ignore_rules.rs`：clineignore 忽略规则模块。全局状态使用 `Mutex<IgnoreRulesInner>`，懒初始化读取 `$HOME/.cline/data/.clineignore`（全局）和 `.clineignore`（项目）。核心函数：`load_ignore_file(path)`（读取文件，跳过空行和#注释，去除尾部空白）、`reset_ignore_rules()`（重置全局状态，测试用）、`check_ignore_path(path)`（检查路径是否被忽略：参数校验→init→转相对路径→全局规则检查→项目规则检查）。内部匹配：`!`前缀取反 + `fnmatch_pathname`匹配 + 无`/`的pattern加`*/`前缀再试
- `core/src/lib.rs`：模块导出声明（pub mod ignore_rules）
- `core/src/glob.rs`：暴露 `fnmatch_pathname` 为 `pub(crate)` 供 ignore_rules 调用

- Affected files: `core/src/ignore_rules.rs`, `core/src/lib.rs`, `core/src/glob.rs`

## JSON 搜索输出格式化模块

### Added
- `core/src/search_json.rs`：JSON 搜索输出格式化模块。直接字符串拼接（不使用 serde_json）。公共 API：`json_escape(s)`（JSON 字符串转义：`"→\"`, `\→\\`, `\n→\\n`, `\t→\\t`, `\r→\\r`，双引号包裹）、`format_start_json(path)`（输出 `{"type":"start","path":"..."}\n`）、`format_end_json()`（输出 `{"type":"end"}\n`）、`format_match_json(match, ctx)`（输出匹配结果 JSON，ctx 非空且有内容时附加 context_before/context_after 数组）
- `core/src/lib.rs`：模块导出声明（pub mod search_json）

- Affected files: `core/src/search_json.rs`, `core/src/lib.rs`

## 路径解析模块

### Added
- `core/src/pathutils.rs`：四个路径解析函数：`normalize_slashes`（反斜杠→正斜杠）、`resolve_workspace_path`（相对/绝对路径解析）、`to_rel_path`（相对路径计算）、`resolve_path`（返回 `(absolutePath, displayPath)` 元组）
- `core/src/lib.rs`：模块导出声明（pub mod pathutils）

- Affected files: `core/src/pathutils.rs`, `core/src/lib.rs`

## XDiff Unified Diff 引擎

### Added
- `core/src/xdiff.rs`：基于 `similar` crate（Myers O(ND)）的 unified diff 引擎，导出 `diff(a, b, ctx_len)` 公共 API。支持上下文窗口合并（间距 ≤ 2×ctxLen）、0 计数 hunk header（pure addition/deletion）、`\ No newline at end of file` 内联标记
- `core/src/lib.rs`：模块导出声明（pub mod xdiff）

- Affected files: `core/src/xdiff.rs`, `core/src/lib.rs`

## Search 正则搜索模块

### Added
- `core/src/search.rs`：使用 `regex` crate（PCRE2 兼容），导出 5 个公共 API：`new_search`（编译正则，支持 case_insensitive/multi_line/dot_all 选项）、`Search::match_first`（单次匹配，返回 `Option<Match>`）、`Search::match_all`（全部匹配，返回 `Vec<Match>`）、`calc_line_number`（偏移量 → 1-based 行号）、`get_line`（行号 → 行内容）
- `core/src/lib.rs`：模块导出声明（pub mod search）
- `core/Cargo.toml`：新增 `regex` 依赖

- Affected files: `core/src/search.rs`, `core/src/lib.rs`, `core/Cargo.toml`

## Glob 通配符匹配模块

### Added
- `core/src/glob.rs`：手动实现 fnmatch 算法（支持 `*`/`?`/`[...]` 回溯匹配），导出 `match_glob`（单模式，`!` 前缀取反）、`match_glob_pathname`（FNM_PATHNAME 语义：`*`/`?` 不匹配 `/`，回溯不跨越路径分隔符）、`match_any_glob`（多模式，`!` 否定优先短路）
- `core/src/lib.rs`：模块导出声明（pub mod glob）

- Affected files: `core/src/glob.rs`, `core/src/lib.rs`

## Context 上下文缓冲模块

### Added
- `core/src/context.rs`：`Context` struct，提供 `new`、`add_line`、`clear` 方法
- `core/src/lib.rs`：模块导出声明（pub mod context）

- Affected files: `core/src/context.rs`, `core/src/lib.rs`
