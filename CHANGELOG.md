# Changelog

## IPC 断连检测与重连

### Added
- `tui/src/app_event.rs`：新增 4 个事件变体 — `IpcDisconnected`、`ReconnectRequested`、`IpcReconnected { session_id }`、`ReconnectFailed { reason }`
- `tui/src/app.rs`：`SessionStatus` 新增 `Disconnected` 变体；`App` 新增 `disconnect_reason: Option<String>` 字段；新增 `is_disconnected()` 方法；`handle_key()` 开头增加 r 键断连拦截（仅 Disconnected 状态触发 `ReconnectRequested`）；`SubmitMessage`/`Cancel` 在断连状态下被拦截；`handle_app_event()` 新增 4 个事件分支：`IpcDisconnected`（结束流式 + 标记 Running tool 为 Error + 推送 SystemMessageCell）、`IpcReconnected`（恢复 Active 状态）、`ReconnectFailed`（推送 ErrorCell）
- `tui/src/main.rs`：`ipc`/`ipc_reader` 改为 `Option<>`；新增 `maybe_read_message()` 辅助函数（`None` 时返回 `std::future::pending()` 禁用 IPC 分支）；IPC 分支收到 `None` 时设置 `ipc=None` 并发送 `IpcDisconnected`；新增 `ReconnectRequested` 事件处理（连接 + create_session + 发送 `IpcReconnected` 或 `ReconnectFailed`）；`UserMessageSent`/`CancelRequested` 增加 `if let Some(ref ipc)` 连接检查；退出清理适配 Option
- `tui/src/ui/status.rs`：`SessionStatus::Disconnected` → `●` 红色
- `tui/src/ui/input.rs`：`InputBarData` 新增 `is_disconnected: bool`；断连时提示行变为 `r 重连 · Ctrl+C 退出`
- `tui/src/ui/mod.rs`：构造 `InputBarData` 时传入 `is_disconnected: app.is_disconnected()`
- 单元测试：app.rs 11 个新测试（断连状态/流式中断/Running tool 标记/重连清除/重连失败/提交拦截/Cancel 拦截/r 键触发/r 键透传/断连退出）、input.rs 2 个（断连提示/正常提示）、status.rs 1 个（Disconnected 颜色）、ipc.rs 1 个集成测试（断连 + 重启 server + 重连）

### Architecture
- 断连后 `ipc`/`ipc_reader` 设为 `None`，`tokio::select!` 的 IPC 分支通过 `std::future::pending()` 自动禁用，不占用 CPU
- 重连创建新 session，旧聊天历史保留在 UI 中
- r 键拦截在 `App::handle_key()` 开头完成，keymap 无需改动

- Affected files: `tui/src/app_event.rs`, `tui/src/app.rs`, `tui/src/main.rs`, `tui/src/ui/status.rs`, `tui/src/ui/input.rs`, `tui/src/ui/mod.rs`, `tui/src/ipc.rs`

## App 状态机重构：handle_key/handle_paste/handle_ipc_message + tool cursor

### Added
- `tui/src/app.rs`：新增 `handle_key(KeyEvent)` 方法，将 `main.rs` 事件循环中的 key dispatch 逻辑移入 App。支持所有 KeyAction（Quit/Cancel/CycleAgentMode/SubmitMessage/FocusNext/ScrollUp/ScrollDown/ScrollToBottom/ToggleToolExpand/None），`ToggleToolExpand` 使用 `tool_cursor` 替代原来的"查找第一个 ToolCallCell"逻辑
- `tui/src/app.rs`：新增 `handle_paste(&str)` 方法，将 `main.rs` 中的粘贴处理逻辑移入 App。焦点在 Input 时逐字符转换为 KeyEvent 插入；焦点在 ChatPanel 时忽略
- `tui/src/app.rs`：新增 `handle_ipc_message(JsonRpcMessage)` 方法，合并 `parse_ipc_event` + `handle_app_event` 调用
- `tui/src/chatwidget.rs`：新增 `tool_cursor: Option<usize>` 字段，`current_tool_index()` / `next_tool_call()` / `prev_tool_call()` 方法。`start_tool_call()` 自动将 `tool_cursor` 更新为新 cell 的 index
- `tui/src/app.rs`：新增 9 个单元测试（handle_key_quit/cycle_mode/submit_message/scroll/toggle_tool/input_passthrough、handle_paste_in_input/in_chat_panel、handle_ipc_message）
- `tui/src/chatwidget.rs`：新增 3 个单元测试（tool_cursor_auto_update、next_prev_tool_call、current_tool_index_none_when_no_tools）

### Refactored
- `tui/src/main.rs`：简化事件循环，`TuiEvent::Key` → `app.handle_key(key)`，`TuiEvent::Paste` → `app.handle_paste(&text)`，IPC 消息 → `app.handle_ipc_message(msg)`。移除 `FocusTarget` 未使用导入

- Affected files: `tui/src/app.rs`, `tui/src/chatwidget.rs`, `tui/src/main.rs`

## 状态栏简化：单行文本 + 管道符分隔 + 优先级裁剪

### Refactored
- `tui/src/ui/status.rs`：状态栏从右对齐多段布局简化为单行文本流式布局。所有段（name / tokens / latency / icon）用 `│` 管道符连接，从左到右整体排列。宽度不足时按优先级从低到高裁剪（P4 icon → P3 latency → P2 tokens → P1 name）。指示灯 `●` 保留颜色：green=active / blue=completed / red=error，其余段无 fg/bg 颜色。移除 `segments_width`/`truncate_str` 等旧辅助函数，保留 `truncate_str` 处理名称截断

- Affected files: `tui/src/ui/status.rs`, `TODO.md`

## UI 渲染层：各面板 ratatui Widget 实现

### Added
- `tui/Cargo.toml`：新增 `unicode-width = "0.1"` 依赖（CJK 字符宽度计算）
- `tui/src/app.rs`：App 状态结构体模块。包含 `SessionStatus`（Active/Completed/Error）、`AgentMode`（Plan/Code/Ask，含 `label()`/`cycle()` 方法）、`FocusTarget`（ChatPanel/Input）、`App` 结构体（chat_widget/status/session_id/session_name/agent_mode/should_quit/needs_redraw/app_event_tx/focus/input_tokens/output_tokens/cache_read_tokens/model/api_latencies）。公共 API：`App::new()`（构造器）、`avg_latency()`（VecDeque< u64 > 容量5，计算平均延迟）。5 个单元测试
- `tui/src/ui/mod.rs`：UI 渲染入口模块。`pub fn render(frame, app)` 使用 ratatui `Layout` 组合三区域（Status(Length 1) / Chat(Min 1) / Input(Length 2)）。`status_bar_data_from_app` 桥接 App 数据到 StatusBarData。2 个测试（full render + minimum terminal size）
- `tui/src/ui/status.rs`：状态栏渲染模块。`StatusBarData` 结构体 + `render_status_bar()` 优先级排序。从右到左组装：P4 icon(●) / P3 latency(avg:Nms) / P2 token(In:X Out:Y Cache R:Z)，宽度不足时从低优先级开始隐藏。name 使用 `UnicodeWidthStr` 按字符边界截断。6 个测试（full width / narrow drop / very narrow truncate / status colors / zero height / default name）
- `tui/src/ui/chat.rs`：聊天面板渲染模块。`render_chat_panel()` 使用 `buf.set_line` 逐行渲染 `display_lines()` 输出，支持 scroll_offset 滚动和 clear_line 行清除。5 个测试（empty chat / single message / scroll offset / streaming cell / tool call）
- `tui/src/ui/input.rs`：输入栏渲染模块。`InputBarData` + `render_input_bar()` 处理 height=0/1/2+ 三种情况。行0：前缀(model + mode + ">") + textarea 内容 + REVERSED 光标（CJK 宽度正确偏移）。行1：快捷键提示。4 个测试（normal rendering / empty input / empty model / minimum height）
- `tui/src/ui/tools.rs`：工具调用渲染辅助模块。`tool_call_lines()`/`tool_call_height()` 委托 `ToolCallCell::display_lines()`/`desired_height()`。3 个测试（collapsed single line / expanded multiple lines / running height）
- `tui/src/ui/streaming.rs`：流式文本渲染器模块。`StreamingRenderer` 持有 raw_source + last_width + rendered_lines，`append_delta` 标记 dirty，`rendered_lines(width)` 宽度变化时 re-render，`render()` 使用 `Paragraph::wrap` 渲染到 Buffer。`reset()` 清空状态。为 P2 两区模型（markdown 重渲染 + commit 动画）预留接口。5 个测试（empty renderer / delta accumulates / rerender on width change / render to buffer / reset clears）
- `tui/src/main.rs`：注册 `mod app; mod ui;`。完整事件循环：快捷键先于 textarea 检查（防止 Tab 被 textarea 吞掉）、mock 数据注入（session_name + token stats + latency + 5 条模拟消息）。ChatPanel 焦点支持 scroll/toggle tool expand

### Refactored
- `tui/src/main.rs`：事件循环重构为 keymap-first 模式（先查快捷键，未匹配才传 textarea），支持 SubmitMessage/CycleAgentMode/Scroll/ToggleToolExpand 等完整 keymap actions

### Bug Fixes
- `tui/src/ui/input.rs`：光标位置修复 — `textarea.cursor().1` 返回字符索引，CJK 字符占 2 列显示宽度，修复为累加 `UnicodeWidthStr::width(text_before)` 计算实际偏移
- `tui/src/ui/input.rs`：快捷键拦截修复 — 原逻辑先传 textarea 再查 keymap，Tab/Enter 被 textarea 吃掉无法切换焦点或提交消息，修复为 keymap-first 模式

### Architecture
- UI 渲染架构：`ui::render()` → Layout 三分区 → status/chat/input 各自独立渲染，不依赖 `Renderable` trait（直接使用 ratatui Buffer API）
- 状态栏算法：从右到左优先级排序，使用 `unicode_width` crate 计算 CJK 字符宽度，与 ratatui 内部一致
- 输入框光标：使用 `Style::REVERSED`（反色）渲染，在任意终端主题下可见
- StreamingRenderer 独立性：不集成到 chat.rs，为两区模型（markdown 重渲染 + commit 动画）预留接口

- Affected files: `tui/Cargo.toml`, `tui/src/app.rs`, `tui/src/ui/mod.rs`, `tui/src/ui/status.rs`, `tui/src/ui/chat.rs`, `tui/src/ui/input.rs`, `tui/src/ui/tools.rs`, `tui/src/ui/streaming.rs`, `tui/src/main.rs`

## ChatWidget + Keymap: 聊天状态管理 + 键绑定 + 工具块折叠渲染

### Added
- `tui/src/chatwidget.rs`：ChatWidget 状态管理模块。包含 `cells: Vec<Box<dyn HistoryCell>>`（已提交消息列表）、`active_cell`（流式活动 cell）、`textarea: TextArea<'static>`（用户输入）、`tool_trackers: HashMap<String, ToolCallTracker>`（工具调用追踪）。公共 API：`push_cell`、`start_streaming`/`append_streaming`/`finish_streaming`（流式生命周期）、`start_tool_call`/`finish_tool_call`（工具调用生命周期，80 字符参数摘要截断 + Instant 耗时追踪）、`scroll_up`/`scroll_down`/`scroll_to_bottom`、`input_key`/`take_input`/`input_is_empty`、`toggle_tool_expanded`/`is_tool_call`、`total_rendered_lines`。14 个单元测试
- `tui/src/keymap.rs`：键绑定定义模块。两个纯函数 `map_input_key`/`map_chat_key` 返回 `KeyAction` 枚举（Quit/Cancel/CycleAgentMode/SubmitMessage/ScrollUp/ScrollDown/ScrollToBottom/ToggleToolExpand/FocusNext/None）。支持 `KeyEventKind::Press` 过滤（忽略 Release/Repeat）。全局快捷键（Ctrl+C/D/X/Esc/P）在两个焦点下一致。18 个单元测试，含全局快捷键一致性检查
- `tui/src/history_cell.rs`：HistoryCell trait 扩展（新增 `finish_streaming`、`as_tool_call`/`as_tool_call_mut` 类型查询方法）。`AssistantMessageCell` 覆写 `finish_streaming`。`ToolCallCell` 重写为折叠感知渲染：折叠态单行标题（`▶/▼` + 名称 + 参数摘要 + 状态图标 + 耗时），展开态标题行 + 带行号输出。`desired_height` 在 width=0 时使用 `.max(1)` 防止 panic。10 个新增测试
- `core/src/ipc/message.rs`：新增 `METHOD_SET_AGENT_MODE` 常量
- `core/src/ipc/server.rs`：新增 `set_agent_mode` 暂存处理（返回 OK，后续 Phase 实现 system prompt 修改）。1 个测试

### Refactored
- `tui/src/history_cell.rs`：`ToolCallCell::display_lines` 从旧的 status icon + 参数输出模式改为折叠感知渲染（`▶/▼` + `[✓/✗/⟳ elapsed]`）。`ToolCallCell::desired_height` 在折叠态返回 1 行。`title_display_len` 使用 `.chars().count()` 而非 `.len()` 修复 Unicode 显示宽度问题
- `tui/src/main.rs`：注册 `mod chatwidget; mod keymap;`

### Bug Fixes
- `tui/src/history_cell.rs`：`desired_height` 在 `width=0` 时 `div_ceil(0)` 导致 panic，修复为 `.div_ceil(width.max(1))`
- `tui/src/history_cell.rs`：`ToolCallCell::display_lines` 的 `title_display_len` 使用字节计数（`.len()`），对 Unicode 字符（▶/▼/✓/✗）计算不正确，修复为 `.chars().count()`

- Affected files: `tui/src/chatwidget.rs`, `tui/src/keymap.rs`, `tui/src/history_cell.rs`, `tui/src/main.rs`, `core/src/ipc/message.rs`, `core/src/ipc/server.rs`

## TUI IPC Client: 连接 core daemon + 读写分离 + 请求-响应关联

### Added
- `core/src/ipc/transport.rs`：新增 `IpcReadHalf`/`IpcWriteHalf` 结构体，读写逻辑从 `IpcConnection` 迁移到各自结构体；`IpcConnection` 内部持有 `read_half`/`write_half`，原有 API 委托调用；新增 `split(self) -> (IpcReadHalf, IpcWriteHalf)` 零成本 move；新增 `connect(socket_path)` 公共构造器；新增 `IpcTransportError::ConnectFailed(String)` 变体
- `tui/src/ipc.rs`：TUI IPC 客户端模块，包含 `IpcError` 枚举（Transport/Disconnected/RequestTimeout/RpcError）、`IpcClient`（发送请求/通知）、`IpcEventReader`（接收通知），`connect()` 返回元组，内部 spawn read/write task，请求-响应关联通过 `pending: Arc<Mutex<HashMap<u64, oneshot::Sender>>>`，`send_request` 30s 超时，`is_connected()` 断连检测
- `tui/src/ipc.rs` 测试：7 个集成测试全覆盖（connect+create_session、notification、error response、disconnect detection、nonexistent socket、concurrent requests）
- `core/src/ipc/server.rs`：新增 `IpcServer::socket_path()` 公共方法

### Refactored
- `core/src/ipc/transport.rs`：`IpcConnection` 从直接持有 `reader`/`writer` 改为持有 `read_half`/`write_half`，读写方法委托调用

### Architecture
- 读写分离设计：`IpcClient` 通过 mpsc channel 与 write task 通信，`IpcEventReader` 通过 mpsc channel 从 read task 接收消息
- 请求-响应关联：`send_request` 生成自增 ID，插入 pending map，通过 oneshot channel 等待匹配响应
- 断连检测：`connected: AtomicBool` 由 read/write task 在 EOF/error 时设置为 false

- Affected files: `core/src/ipc/transport.rs`, `core/src/ipc/server.rs`, `tui/src/ipc.rs`, `tui/src/main.rs`, `tui/Cargo.toml`

## TUI 脚手架：终端抽象 + 事件类型 + 消息 Cell + 布局 Trait

### Added
- `tui/Cargo.toml`：添加 9 个依赖（crown-core、ratatui 0.29、crossterm 0.28、tokio、interprocess、serde、serde_json、anyhow、tui-textarea 0.7、futures-util 0.3）
- `tui/src/event.rs`：`TuiEvent` 枚举（`Key(KeyEvent)` / `Paste(String)` / `Resize`），终端层事件类型
- `tui/src/tui.rs`：`Tui` 终端抽象结构体，封装 crossterm `Terminal` + `EventStream` 事件读取。方法：`init()`（raw mode + alternate screen + bracketed paste + spawn 事件 task）、`restore()`（静态方法，恢复终端状态）、`enter_alt_screen()`/`leave_alt_screen()`、`draw()`、`size()`、`event_receiver()`。实现 `Drop` 自动恢复终端
- `tui/src/app_event.rs`：`AppEvent` 枚举（11 variants：UserMessageSent/CancelRequested/Quit/AssistantDelta/ReasoningDelta/ToolCallStart/ToolResult/Usage/TaskDone/Error/RedrawRequested）+ `AppEventSender`（封装 `UnboundedSender`，实现 `Clone`）
- `tui/src/history_cell.rs`：`HistoryCell` trait（`display_lines`/`desired_height`/`is_stream_continuation`/`append_delta`）+ 5 种 cell 类型（`UserMessageCell`/`AssistantMessageCell`/`ToolCallCell`/`SystemMessageCell`/`ErrorCell`）+ `ToolCallStatus` 枚举 + 辅助函数 `wrap_text_to_lines`（UTF-8 安全，使用 `char_indices` 断行）/ `make_line`。覆盖 `wrap_text_to_lines` 的多字节字符安全测试
- `tui/src/renderable.rs`：`Renderable` trait（`render`/`desired_height`/`cursor_pos`）+ `FlexDirection`（Horizontal/Vertical）+ `FlexItem`（Flex/Fixed）+ `flex_layout` 布局引擎（基于 ratatui `Layout::split`）。3 个测试（垂直分割/水平分割/溢出固定优先）
- `tui/src/main.rs`：声明所有模块 + 最小化 main（初始化 TUI + 显示 block + 等待 q/Esc 退出）

### Architecture
- `EnableBracketedPaste`/`DisableBracketedPaste` 从 `crossterm::event` 导入（非 `crossterm::terminal`），与 codex-rs 实现一致
- `wrap_text_to_lines` 使用 `char_indices().nth(width)` 定位字符边界，避免 `remaining[..width]` 在 UTF-8 多字节字符处 panic
- 测试覆盖：12 个单元测试（9 个 history_cell + 3 个 renderable），全部 pass

- Affected files: `tui/Cargo.toml`, `tui/src/main.rs`, `tui/src/event.rs`, `tui/src/tui.rs`, `tui/src/app_event.rs`, `tui/src/history_cell.rs`, `tui/src/renderable.rs`

## Agent Loop 事件驱动化

### Added
- `core/src/agent/loop.rs`：`AgentEventHandler` trait（7 个异步安全回调：on_assistant_text/on_reasoning/on_tool_call_start/on_tool_result/on_usage/on_task_done/on_error），`AgentSession` struct（`new`/`handle_user_message`/`cancel`/`history_len`/`reset`），单元测试覆盖构造/取消/重置/共享 Arc 标志
- `core/src/ipc/session_manager.rs`：`IpcEventHandler` struct 实现 `AgentEventHandler`，通过 `try_send` 非阻塞推送 JSON-RPC 通知（assistant_text/assistant_reasoning/tool_call_start/tool_result/usage/task_done/error）；`SessionManager::cancel_session()` 使用独立 `cancel_flags: RwLock<HashMap<String, Arc<AtomicBool>>>` 实现无锁取消
- 新增测试：`test_cancel_session`、`test_cancel_nonexistent`（session_manager.rs）；`test_agent_session_new_has_system_prompt`、`test_agent_session_cancel`、`test_agent_session_reset`、`test_agent_session_cancelled_flag_shared`（loop.rs）

### Refactored
- `core/src/agent/loop.rs`：将 `run_agent_loop` 的内层 agent loop 提取为 `AgentSession::handle_user_message`，所有终端 I/O（print!/eprintln!/stdin.read_line）替换为 `AgentEventHandler` 回调；`run_agent_loop` 重构为 `AgentSession` 的 CLI 薄包装
- `core/src/ipc/session_manager.rs`：`SessionState` 移除 `history`/`cancelled` 字段，新增 `agent: AgentSession`；`SessionManager` 新增 `cancel_flags` 独立 RwLock map，锁顺序统一为 `sessions → cancel_flags` 避免死锁
- `core/src/ipc/server.rs`：`user_message` handler 改为 `tokio::spawn` 异步执行 `state.agent.handle_user_message()`，`cancel` handler 使用 `sm.cancel_session()`（无需锁 SessionState，无死锁风险）；移除 3 个未使用 import

- Affected files: `core/src/agent/loop.rs`, `core/src/ipc/session_manager.rs`, `core/src/ipc/server.rs`

## IPC 协议定义

### Added
- `core/src/ipc/message.rs`：JSON-RPC 2.0 消息类型（`JsonRpcMessage`、`JsonRpcError`），4 个构造函数（`make_request`/`make_notification`/`make_response`/`make_error_response`），3 个分类判断（`is_request`/`is_notification`/`is_response`），12 个标准/自定义错误码常量，12 个方法/事件名常量
- `core/src/ipc/transport.rs`：跨平台 Unix socket transport（`IpcTransport` 含 `ListenerOptions` reclaim/try_overwrite），`IpcConnection` 支持 `split()` 并发读写，socket 路径解析（CLI 参数 / `CROWN_SOCKET_PATH` 环境变量 / 默认 `/tmp/crown-code-{uid}.sock` 三级回退），`IpcTransportError` 枚举
- `core/src/ipc/session_manager.rs`：`SessionManager`（`RwLock<HashMap>` + `Mutex<ApiClientConfig>`），`SessionState`（含 `event_tx` mpsc channel、`cancelled` AtomicBool flag），`SessionInfo`（Serialize/Deserialize），nanoid 12 字符 session ID 生成
- `core/src/ipc/server.rs`：`IpcServer`（`new`/`run`/`shutdown`），`handle_connection`（biased select! 优先处理请求），`dispatch_request` 路由 6 个方法（create_session/list_sessions/destroy_session/user_message/cancel/set_config），user_message stub 通过 event channel 发送模拟事件
- `core/src/main.rs` daemon 模式：`--socket-path` 参数，`CROWN_SOCKET_PATH` 环境变量，`ctrl_c()` 优雅关闭
- 依赖：`nanoid 0.4`（session ID）、`libc`（getuid Unix）、tokio `signal` + `time` features
- 42 个 IPC 单元测试 + 集成测试全覆盖

### Refactored
- `core/src/lib.rs`：新增 `pub mod ipc;`
- `core/src/ipc/mod.rs`：4 模块声明

- Affected files: `core/Cargo.toml`, `core/src/lib.rs`, `core/src/main.rs`, `core/src/ipc/mod.rs`, `core/src/ipc/message.rs`, `core/src/ipc/transport.rs`, `core/src/ipc/session_manager.rs`, `core/src/ipc/server.rs`

## 核心异步运行时迁移

### Architecture
- `core/Cargo.toml`：移除 `libc` 依赖，移除 `reqwest` 的 `blocking` feature，新增 `tokio`（6 features：rt-multi-thread/net/io-util/sync/macros/process）和 `interprocess` 依赖
- `core/src/main.rs`：入口改为 `#[tokio::main] async fn main()`
- `core/src/mcp/transport_http.rs`：`reqwest::blocking::Client` → `reqwest::Client`（async），`post_json`/`post_json_stream` 变为 `async fn`
- `core/src/mcp/transport_stdio.rs`：`std::process` → `tokio::process`，`libc::poll` → `tokio::time::timeout`，`std::thread` → `tokio::spawn`，所有 I/O 方法变为 `async fn`
- `core/src/command_exec.rs`：`std::process::Command` → `tokio::process::Command`，`std::thread::spawn` → `tokio::spawn`，`try_wait` 轮询 → `tokio::time::timeout` + `child.wait()`，`exec_command` 变为 `async fn`
- `core/src/api/openai.rs`：`create_message`/`create_message_stream` 变为 `async fn`
- `core/src/mcp/client.rs`：3 个 Mutex（stdio/http/transport_lock）改为 `tokio::sync::Mutex`（跨 await 持有），其余 4 个保持 `std::sync::Mutex`（短暂访问）；`send_json_rpc`/`send_notification`/`initialize`/`reconnect`/`call_tool`/`list_tools`/`destroy`/`new` 变为 `async fn`；心跳改 `tokio::spawn`，`JoinHandle<()>` 替代 `thread::JoinHandle<()>`
- `core/src/mcp/registry.rs`：`destroy`/`get_client` 变为 `async fn`
- `core/src/agent/tools.rs`：`execute_tool`/`execute_execute_command` 变为 `async fn`
- `core/src/agent/loop.rs`：`run_agent_loop` 变为 `async fn`

### Bug Fixes
- `core/src/mcp/transport_stdio.rs`：`close()` 缺少 `child.wait().await` 回收子进程，修复僵尸进程泄漏

### Refactored
- 全部 451 个测试通过，约 90 个测试从 `#[test]` 改为 `#[tokio::test]`
- 纯 CPU 模块（sse/jsonrpc/glob/search/xdiff/formatter/pathutils 等）保持同步不变
- `mock_mcp_server.rs` 独立进程保持不变

- Affected files: `core/Cargo.toml`, `core/src/main.rs`, `core/src/mcp/transport_http.rs`, `core/src/mcp/transport_stdio.rs`, `core/src/command_exec.rs`, `core/src/api/openai.rs`, `core/src/mcp/client.rs`, `core/src/mcp/registry.rs`, `core/src/agent/tools.rs`, `core/src/agent/loop.rs`

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
- 测试：51 个单元测试 + 4 个集成测试。类型测试 20 个（MessageRole/Message/Tool/ToolCall/ApiStreamChunk/ApiResponse/ApiClientConfig 构造），非流式 API 测试 16 个（buildChatRequest JSON 结构/messages 转换/tools 字段/parseChatResponse 正常/异常/空场景），流式 API 测试 15 个（parseStreamEvent 各 variant/SSE 注释忽略/单 tool call 跨 chunk 累积/多 tool call 并行累积）

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

- 三个模块的 crownignore 访问控制测试已通过 `serial_test` crate + `set_current_dir` 临时目录实现自动化覆盖（`core/src/file_reader.rs:637`、`core/src/file_writer.rs:152`、`core/src/file_edit.rs:363`）
- 依赖：`core/Cargo.toml` 新增 `serial_test = "3"`

- Affected files: `core/Cargo.toml`, `core/src/file_reader.rs`, `core/src/file_writer.rs`, `core/src/file_edit.rs`, `CHANGELOG.md`

## crownignore 忽略规则模块

### Added
- `core/src/ignore_rules.rs`: crownignore 忽略规则模块。全局状态使用 `Mutex<IgnoreRulesInner>`，懒初始化读取 `$HOME/.crown/data/.crownignore`（全局）和 `.crownignore`（项目）。核心函数：`load_ignore_file(path)`（读取文件，跳过空行和#注释，去除尾部空白）、`reset_ignore_rules()`（重置全局状态，测试用）、`check_ignore_path(path)`（检查路径是否被忽略：参数校验→init→转相对路径→全局规则检查→项目规则检查）。内部匹配：`!`前缀取反 + `fn match_pathname`匹配 + 无`/`的pattern加`*/`前缀再试
- `core/src/lib.rs`：模块导出声明（pub mod ignore_rules）
- `core/src/glob.rs`：暴露 `fn match_pathname` 为 `pub(crate)` 供 ignore_rules 调用

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
