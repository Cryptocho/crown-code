# TODO — crown-code 开发路线图

## 核心架构

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  tui (终端A)  │  │  tui (终端B) │  │  gui (未来)   │
│  ratatui     │  │  ratatui        |              | 
│  thin client │  │  thin client │  │  thin client │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │
       │  Local socket (跨平台)             │
       │                  │                 │
       └─────────┬────────┴────────┬────────┘
                 │                 │
          ┌──────┴─────────────────┴──────┐
          │     crown-core (daemon)       │
          │                               │
          │  ┌─────────────────────────┐  │
          │  │ IPC Server (async)      │  │
          │  │  - Local socket listener│  │
          │  │  - Session multiplexer  │  │
          │  └────────────┬────────────┘  │
          │               │               │
          │  ┌────────────┴────────────┐  │
          │  │ Session Manager         │  │
          │  │  - session_1 (终端A)     │  │
          │  │  - session_2 (终端B)     │  │
          │  └────────────┬────────────┘  │
          │               │               │
          │  ┌────────────┴────────────┐  │
          │  │ Agent Loop (per session)│  │
          │  │  - tools / prompt       │  │
          │  │  - api client           │  │
          │  │  - mcp client           │  │
          │  └─────────────────────────┘  │
          └───────────────────────────────┘
```

**设计原则**：
- Core 是独立 daemon 进程，启动后常驻，监听本地 socket（跨平台：Linux/macOS 为 Unix domain socket，Windows 为 named pipe）
- TUI 是 thin client，只负责 UI 渲染和 IPC 通信，零业务逻辑依赖
- 单 core 进程同时服务多个 TUI session，session 间完全隔离
- IPC 使用 `interprocess` crate（跨平台本地 socket 抽象）+ tokio async，高性能低延迟
- JSON-RPC 2.0 消息协议，每行一个 JSON 对象（`\n` 分隔）

---

## Phase 1: Core Daemon + IPC + TUI MVP（最高优先级）

### 1.1 Core: Async Runtime + 跨平台依赖引入

Core daemon 需要并发处理多个 socket 连接 + 多个 agent loop + API 流式调用，必须引入 async runtime。同时引入 `interprocess` crate 实现跨平台本地 IPC（Unix socket / Windows named pipe）。

- [x] `core/Cargo.toml` 新增依赖：
  - `tokio`（rt-multi-thread, net, io-util, sync, macros）
  - `interprocess`（local-socket, tokio feature）— 跨平台本地 IPC
- [x] `core/src/main.rs` 改为 `#[tokio::main]`
- [x] 现有阻塞 I/O 全部改为 async：
  - `api/openai.rs`：`reqwest::blocking` → `reqwest`（async）
  - `mcp/transport_http.rs`：`reqwest::blocking` → `reqwest`（async）
  - `mcp/transport_stdio.rs`：`std::process::Command` → `tokio::process::Command`
  - `command_exec.rs`：`std::process::Command` → `tokio::process::Command`

### 1.2 Core: IPC 协议定义

使用 JSON-RPC 2.0，消息以 `\n` 分隔，通过本地 socket 双向传输（跨平台）。

#### Socket 路径

- **Linux/macOS**：`/tmp/crown-code-{uid}.sock`
- **Windows**：`\\.\pipe\crown-code-{uid}`
- 可通过 `--socket-path` 参数或环境变量 `CROWN_SOCKET_PATH` 覆盖

#### 连接生命周期

```
TUI 连接 socket → 发送 {"method":"create_session","id":1,"params":{...}}
Core 响应       → {"id":1,"result":{"session_id":"sess_abc123"}}
TUI 发消息      → {"method":"user_message","id":2,"params":{"session_id":"sess_abc123","content":"hello"}}
Core 流式推送   → {"method":"assistant_text","params":{"session_id":"sess_abc123","delta":"Hi"}}
                → {"method":"assistant_text","params":{"session_id":"sess_abc123","delta":" there"}}
                → {"method":"task_done","params":{"session_id":"sess_abc123","summary":"..."}}
```

#### TUI → Core 消息

| method | params | 说明 |
|--------|--------|------|
| `create_session` | `{ "cwd": string, "model"?: string, "base_url"?: string, "api_key"?: string }` | 创建新 session，返回 `session_id`。可选 API 配置覆盖全局默认 |
| `user_message` | `{ "session_id": string, "content": string }` | 用户发送消息 |
| `cancel` | `{ "session_id": string }` | 取消当前任务 |
| `destroy_session` | `{ "session_id": string }` | 销毁 session |
| `list_sessions` | `{}` | 列出所有活跃 session |
| `set_agent_mode` | `{ "session_id": string, "mode": "plan"|"code"|"ask" }` | 切换 Agent 模式（Core 修改 system prompt，不过滤工具） |
| `set_config` | `{ "session_id"?: string, "base_url"?: string, "api_key"?: string, "model"?: string, ... }` | 修改 API 配置（有 session_id 则覆盖该 session，否则全局） |

#### Core → TUI 事件（Server Push，无需 id）

| method | params | 说明 |
|--------|--------|------|
| `assistant_text` | `{ "session_id": string, "delta": string }` | 流式文本片段（增量） |
| `assistant_reasoning` | `{ "session_id": string, "delta": string }` | 模型推理过程（增量） |
| `tool_call_start` | `{ "session_id": string, "call_id": string, "name": string, "arguments": string }` | 工具调用开始 |
| `tool_result` | `{ "session_id": string, "call_id": string, "name": string, "content": string, "is_error": bool }` | 工具执行结果 |
| `usage` | `{ "session_id": string, "input_tokens": int, "output_tokens": int, "cache_read_tokens": int }` | Token 用量 |
| `task_done` | `{ "session_id": string, "summary": string }` | 任务完成 |
| `error` | `{ "session_id"?: string, "code": int, "message": string }` | 错误信息 |
| `session_name_update` | `{ "session_id": string, "name": string }` | Session 名称更新（Core LLM 根据首次用户消息生成） |
| `session_created` | `{ "session_id": string }` | Session 创建成功通知 |
| `session_destroyed` | `{ "session_id": string }` | Session 销毁通知 |

#### 设计决策

- 请求消息含 `id` 字段（需要响应），通知/事件消息无 `id` 字段（fire-and-forget）
- Core 主动推送事件时，`session_id` 必填，TUI 据此路由到正确的 UI 面板
- 错误响应遵循 JSON-RPC 2.0 标准：`{"jsonrpc":"2.0","id":...,"error":{"code":int,"message":"..."}}`

### 1.3 Core: Agent Loop 事件驱动化

**当前问题**：`agent/loop.rs` 直接 `print!` / `stdin.read_line`，与终端 I/O 紧耦合。

**重构方案**：Agent loop 变为纯逻辑层，通过 trait 回调与外部通信。

```rust
// core/src/agent/loop.rs

pub trait AgentEventHandler: Send {
    fn on_assistant_text(&mut self, delta: &str);
    fn on_reasoning(&mut self, delta: &str);
    fn on_tool_call_start(&mut self, call_id: &str, name: &str, arguments: &str);
    fn on_tool_result(&mut self, call_id: &str, name: &str, content: &str, is_error: bool);
    fn on_usage(&mut self, input_tokens: i32, output_tokens: i32, cache_read_tokens: i32);
    fn on_task_done(&mut self, summary: &str);
    fn on_error(&mut self, code: i32, message: &str);
}

pub struct AgentSession {
    config: ApiClientConfig,
    history: Vec<Message>,
    cwd: String,
    cancelled: AtomicBool,
}

impl AgentSession {
    pub fn new(config: ApiClientConfig, cwd: String) -> Self;
    pub async fn handle_user_message(&mut self, content: &str, handler: &mut dyn AgentEventHandler);
    pub fn cancel(&self);
    pub fn history_len(&self) -> usize;
    pub fn reset(&mut self);
}
```

- [x] 定义 `AgentEventHandler` trait
- [x] 将 `run_agent_loop` 重构为 `AgentSession::handle_user_message`
- [x] 所有 `print!` / `eprintln!` 替换为 `handler.on_*` 调用
- [x] 所有 `stdin.read_line` 移除，用户输入由外层 IPC 传入
- [x] 支持 `cancel()` 通过 `AtomicBool` 中断正在进行的 API 调用

### 1.4 Core: IPC Server + Session Manager

新增 `core/src/ipc/` 模块：

```
core/src/ipc/
├── mod.rs              # 模块导出
├── message.rs          # JSON-RPC 消息序列化/反序列化
├── transport.rs        # 跨平台本地 socket transport (interprocess + tokio)
├── server.rs           # IPC server：accept 连接、消息路由、广播
└── session_manager.rs  # Session 生命周期管理
```

#### `message.rs`

```rust
#[derive(Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,           // "2.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,           // 请求有 id，通知无 id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,    // 请求/通知有 method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,     // 请求/通知的参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,     // 成功响应
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>, // 错误响应
}

#[derive(Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}
```

#### `transport.rs`

```rust
use interprocess::local_socket::tokio::{Listener, Stream};
use interprocess::local_socket::ListenerOptions;

pub struct LocalSocketTransport {
    listener: Listener,
    socket_name: String,
}

impl LocalSocketTransport {
    pub async fn bind(socket_name: &str) -> Result<Self, IpcError>;
    pub async fn accept(&self) -> Result<Stream, IpcError>;
    pub fn socket_name(&self) -> &str;
}
```

- [x] 使用 `interprocess::local_socket::tokio::Listener/Stream`（跨平台）
- [x] 启动时自动清理残留的 socket 文件（Linux/macOS 需要，Windows named pipe 自动管理）
- [x] 进程退出时自动清理（signal handler + Drop）
- [x] 支持 graceful shutdown（SIGTERM/SIGINT / Windows Ctrl+C）

#### `server.rs`

```rust
pub struct IpcServer {
    transport: LocalSocketTransport,
    session_manager: Arc<SessionManager>,
}

impl IpcServer {
    pub async fn new(socket_path: &str, config: ApiClientConfig) -> Result<Self, IpcError>;
    pub async fn run(&self) -> Result<(), IpcError>;  // 主循环：accept → spawn handler task
    pub async fn shutdown(&self) -> Result<(), IpcError>;
}
```

- [x] 每个连接 spawn 一个 tokio task 处理
- [x] 连接 handler：循环读取消息 → 解析 JSON-RPC → 分发到对应 session → 写回响应/事件
- [x] 广播能力：session 产生的事件通过该 session 对应的连接推送

#### `session_manager.rs`

```rust
pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Mutex<AgentSession>>>>,
    config: Mutex<ApiClientConfig>,
}

impl SessionManager {
    pub fn new(config: ApiClientConfig) -> Self;
    pub async fn create_session(&self, cwd: String) -> String;  // 返回 session_id
    pub async fn destroy_session(&self, session_id: &str) -> Result<(), IpcError>;
    pub fn list_sessions(&self) -> Vec<SessionInfo>;
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<Mutex<AgentSession>>>;
    pub fn update_config(&self, config: ApiClientConfig);
}
```

- [x] session_id 生成：`nanoid` 或自增 ID + 随机后缀
- [x] Session 持有独立的 `AgentSession` 实例（独立 history、独立 API client）
- [x] 所有 session 共享同一份 API config（可通过 `set_config` 全局修改）

### 1.5 Core: `main.rs` 入口改造

```rust
// core/src/main.rs

#[tokio::main]
async fn main() {
    // 1. 加载配置（环境变量 / CLI 参数 / 配置文件）
    let config = load_config();
    let socket_path = resolve_socket_path();

    // 2. 启动 IPC server
    let server = IpcServer::new(&socket_path, config).await.unwrap();
    eprintln!("crown-core daemon started, listening on {}", socket_path);

    // 3. 注册信号处理（SIGTERM/SIGINT / Windows Ctrl+C → graceful shutdown）
    // 4. 运行主循环
    server.run().await.unwrap();
}
```

- [x] CLI 参数解析：`--socket-path`（覆盖默认 socket 路径）、`--config`（配置文件路径）
- [x] 默认进入 daemon 模式（监听本地 socket）

### 1.6 TUI: 基础框架（参考 codex-rs ratatui 架构）

> 参考文档：`CODEX.md` — codex-rs 的 ratatui TUI 实现详解

#### 文件结构

```
tui/src/
├── main.rs             # 入口：run_main() → 初始化 terminal + 启动事件循环
├── app.rs              # App 状态机 + tokio::select! 主事件循环
├── app_event.rs        # AppEvent 枚举（内部消息总线）
├── event.rs            # TuiEvent 枚举（终端事件：Key/Paste/Resize/Draw）+ 事件流合并
├── ipc.rs              # IPC 客户端（连接 core daemon、JSON-RPC 读写分离）
├── chatwidget.rs       # ChatWidget 主聊天面板状态
├── history_cell.rs     # HistoryCell trait + 消息 cell 类型定义
├── renderable.rs       # Renderable trait + FlexRenderable 布局引擎
├── tui.rs              # Tui 终端抽象封装（init/restore/draw/event_stream）
├── keymap.rs           # 键绑定定义
├── ui/
│   ├── mod.rs          # UI 渲染入口（compose 各子面板）
│   ├── chat.rs         # 聊天消息渲染（HistoryCell → ratatui Widget）
│   ├── input.rs        # 输入框渲染
│   ├── tools.rs        # 工具调用可折叠块渲染（内联在聊天流中）
│   ├── status.rs       # 状态栏渲染（session 名称、token 用量、API 延迟、session 活动状态）
│   └── streaming.rs    # 流式文本渲染（两区模型：stable scrollback + tail 活动区）
```

#### 核心设计模式（源自 codex-rs）

##### 1. 三层事件架构

```
┌─ TuiEvent（终端层）──────────────────────────────┐
│  Key(KeyEvent)     → 键盘输入                    │
│  Paste(String)     → 粘贴内容                    │
│  Resize            → 终端大小变化                │
│  Draw              → 计划重绘                    │
└─────────────────────────────────────────────────┘
         ↓
┌─ AppEvent（应用层）──────────────────────────────┐
│  主要内部事件：                                   │
│  UserMessageSent, AssistantDelta, ToolCallStart   │
│  ToolResult, TaskDone, Error, CancelRequested     │
│  RedrawRequested, Quit                            │
└─────────────────────────────────────────────────┘
         ↓
┌─ IpcMessage（后端层）────────────────────────────┐
│  core daemon 推送的 JSON-RPC 消息                 │
│  assistant_text, assistant_reasoning              │
│  tool_call_start, tool_result, usage, task_done   │
│  error, session_created, session_destroyed        │
└─────────────────────────────────────────────────┘
```

- `TuiEvent`：crossterm 事件通过独立 tokio task 读取，合并为统一的事件流
- `AppEvent`：App 内部通过 `tokio::sync::mpsc::UnboundedSender<AppEvent>` 发送
- `IpcMessage`：IPC 客户端在独立 task 中读取，通过 channel 传递到主循环

##### 2. Renderable trait + FlexRenderable 布局引擎（替代直接使用 ratatui Layout）

```rust
// renderable.rs
pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)>;
}
```

FlexRenderable 按 flex 权重分配子区域高度，实现自适应布局（单面板线性流）：

```
┌──────────────────────────────────────────────────────────┐
│ StatusBar (flex: 0, 固定 1 行)                            │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ChatPanel (flex: 1)                                     │
│  所有消息按时间顺序线性排列：                                │
│  用户消息、assistant 回复、工具调用（可折叠块）、系统消息      │
│  填充可用空间，支持滚动                                     │
│                                                          │
├──────────────────────────────────────────────────────────┤
│  InputBar (flex: 0, 固定 2-3 行)                         │
│  模型名 + Agent 模式 + 用户输入框                           │
└──────────────────────────────────────────────────────────┘
```

##### 3. HistoryCell trait（类型化消息渲染）

```rust
// history_cell.rs
pub trait HistoryCell: Debug + Send + Sync {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn is_stream_continuation(&self) -> bool;  // true = 该 cell 是前一个 cell 的流式续接，渲染时合并为同一消息块
}

// 实现类型
pub struct UserMessageCell { ... }        // 用户消息
pub struct AssistantMessageCell { ... }   // assistant 回复（支持流式追加）
pub struct ToolCallCell { ... }           // 工具调用（命令 + 输出 + 状态）
pub struct SystemMessageCell { ... }      // 系统消息
pub struct ErrorCell { ... }             // 错误消息
```

每种消息类型独立控制渲染样式，便于后续扩展（如 markdown 渲染、语法高亮、patch diff 等）。

##### 4. 流式输出两区模型

```
raw_source (markdown 原文)
    ↓ 按宽度重新渲染
rendered_lines
    ↓ 分区
┌──────────────────────┐
│ stable region        │ → 提交到 scrollback（已确认的行）
├──────────────────────┤
│ tail region          │ → 活动 cell（当前流式文本，持续更新）
└──────────────────────┘
```

- 收到 IPC `assistant_text` delta → 累积到 raw_source → 重新渲染 → 分区
- stable region 的行逐帧 drain 到 scrollback，产生打字效果
- 流结束时 tail region 最终提交，整个消息固化为 `AssistantMessageCell`

##### 5. Tui 终端抽象

```rust
// tui.rs
pub struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    event_tx: mpsc::UnboundedSender<TuiEvent>,
    // ...
}

impl Tui {
    pub fn init() -> Result<Self>;        // raw mode, bracketed paste, alternate screen
    pub fn restore() -> Result<()>;       // 恢复终端状态
    pub fn draw<F>(&mut self, f: F)       // 渲染 + 同步更新 viewport
    where F: FnOnce(&mut Frame);
    pub fn event_stream(&self) -> impl Stream<Item = TuiEvent>;  // 合并的事件流
}
```

#### TUI 布局

单面板线性流，所有内容（用户消息、assistant 回复、工具调用/结果）按时间顺序排列。工具调用以可折叠块形式内联显示。

```
┌────────────────────────────────────────────────────────────────┐
│ 我的第一个项目 │ In:1234 Out:567 Cache R:890 │ avg:230ms │ ●   │ ← 状态栏
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ [You] 读取 main.rs 并添加配置加载逻辑                             │
│                                                                │
│ [Assistant] 我来帮你实现。首先读取当前文件结构...                    │
│                                                                │
│   ▶ read_file "core/src/main.rs" → 14 lines           [✓ 0.1s]│ ← 可折叠块（默认折叠）
│                                                                │
│   ▼ read_file "core/src/lib.rs"                      [✓ 0.08s]│ ← 可折叠块（展开状态）
│     1 | pub mod agent;                                         │
│     2 | pub mod api;                                           │
│     3 | pub mod command_exec;                                  │
│     ...                                                        │
│                                                                │
│ [Assistant] 文件结构清楚了，现在添加配置加载逻辑...                  │
│                                                                │
│   ⟳ write_to_file "core/src/main.rs"                 [running]│ ← 正在执行
│                                                                │
│ [Assistant] 完成！已在 main.rs 中添加配置加载逻辑。                 │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ gemma4:e4b │ [Code] │ > 请输入你的任务...                        │ ← 输入框
│ Enter 发送 · Ctrl+C 退出 · Tab 切换模式                         │
└────────────────────────────────────────────────────────────────┘
```

**状态栏设计**：
- 顶部固定 1 行，信息按优先级从左到右排列，根据终端宽度自适应裁剪
- 优先级 1：session 名称（由模型根据用户首次输入自动生成，如"我的第一个项目"）
- 优先级 2：token 用量（`In:{input} Out:{output} Cache R:{cache_read}`）
- 优先级 3：API 平均延迟（`avg:{ms}ms`，取最近 5 次请求平均值）
- 优先级 4：session 活动状态指示灯（`●`(green)=active/running，`●`(blue)=completed/finished，`●`(red)=error）
- 终端宽度不足时，按优先级从低到高隐藏信息

**输入框设计**：
- 固定 2-3 行（输入框 + 快捷键提示）
- 左侧显示：模型名 + 当前 Agent 模式标签（`[Plan]`/`[Code]`/`[Ask]`，可切换）
- 模型名和 Agent 模式显示在输入框上方或左侧，不占用输入空间

#### 1.6.1 项目脚手架：依赖 + Tui 终端抽象 + TuiEvent

> 目标：TUI crate 可编译，终端能进入/退出 raw mode，键盘事件可异步读取

- [x] `tui/Cargo.toml` 添加依赖：

```toml
[dependencies]
crown-core = { path = "../core" }   # 仅用于共享类型定义（Message、ToolCall 等），不依赖业务逻辑
ratatui = "0.29"
crossterm = { version = "0.28", features = ["bracketed-paste", "event-stream"] }
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "sync", "macros"] }
interprocess = { version = "2", features = ["tokio"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tui-textarea = "0.7"   # 多行文本输入组件（参考 codex 的 ChatComposer）
```

- [x] `tui/src/tui.rs` — Tui 终端抽象：
  - `Tui::init()`：进入 raw mode、启用 bracketed paste、切换 alternate screen
  - `Tui::restore()`：恢复终端状态（Drop 自动调用）
  - `Tui::enter_alt_screen()` / `leave_alt_screen()`
  - `Tui::draw<F>(&mut self, f: F)`：渲染帧 + 同步 viewport
  - `Tui::event_stream()`：返回 `impl Stream<Item = TuiEvent>`，内部 spawn tokio task 读取 crossterm `EventStream`，将 `crossterm::event::Event` 转换为 `TuiEvent` 发送到 mpsc channel
  - 终端大小查询：`Tui::size()` → `Rect`

- [x] `tui/src/event.rs` — TuiEvent 枚举：

```rust
pub enum TuiEvent {
    Key(KeyEvent),     // 键盘输入
    Paste(String),     // 粘贴内容（bracketed paste）
    Resize,            // 终端大小改变
    Draw,              // 计划重绘（由帧率限制器调度）
}
```

- [x] 验证：`cargo build -p crown-tui` 编译通过

#### 1.6.2 事件与类型基础：AppEvent + HistoryCell + Renderable

> 目标：定义三层事件类型、消息 cell 类型体系、布局 trait

- [x] `tui/src/app_event.rs` — AppEvent 枚举 + AppEventSender：

```rust
pub enum AppEvent {
    // 用户操作
    UserMessageSent(String),
    CancelRequested,
    Quit,

    // IPC 事件（从 core daemon 接收后转换）
    AssistantDelta { delta: String },
    ReasoningDelta { delta: String },
    ToolCallStart { call_id: String, name: String, arguments: String },
    ToolResult { call_id: String, name: String, content: String, is_error: bool },
    Usage { input_tokens: i32, output_tokens: i32, cache_read_tokens: i32 },
    TaskDone { summary: String },
    Error { code: i32, message: String },

    // UI 控制
    RedrawRequested,
}

pub struct AppEventSender {
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl AppEventSender {
    pub fn new(tx: mpsc::UnboundedSender<AppEvent>) -> Self;
    pub fn send(&self, event: AppEvent);
}
```

- [x] `tui/src/history_cell.rs` — HistoryCell trait + 5 种 cell 类型：

```rust
pub trait HistoryCell: Debug + Send + Sync {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn is_stream_continuation(&self) -> bool;  // true = 该 cell 是前一个 cell 的流式续接，渲染时合并为同一消息块
}
```

  - `UserMessageCell`：用户消息，`[You]` 前缀 + 内容
  - `AssistantMessageCell`：assistant 回复，支持流式追加 delta（`append_delta(&mut self, delta: &str)`）
  - `ToolCallCell`：工具调用（工具名 + 参数摘要 + 输出 + 状态 + 是否展开 + 耗时），内联在聊天流中，支持折叠/展开
  - `SystemMessageCell`：系统消息（灰色斜体）
  - `ErrorCell`：错误消息（红色）

- [x] `tui/src/renderable.rs` — Renderable trait + FlexRenderable 布局引擎：

```rust
pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)>;
}
```

  - `FlexRenderable`：按 flex 权重分配子区域高度/宽度，替代直接使用 ratatui `Layout`
  - 支持垂直分割（StatusBar:ChatPanel:InputBar = fixed:flex:fixed）

- [x] 验证：模块可编译，HistoryCell 各类型单元测试（display_lines 宽度换行、desired_height 计算）

#### 1.6.3 IPC 客户端：连接 core daemon + 读写分离

> 目标：TUI 能连接 core daemon socket，双向收发 JSON-RPC 消息

- [x] `core/src/ipc/transport.rs` — `IpcConnection` 重构：
  - 新增 `IpcReadHalf` / `IpcWriteHalf` 结构体，读写逻辑从 `IpcConnection` 迁移到各自结构体
  - `IpcConnection` 内部持有 `read_half` + `write_half`，原有 API 委托调用
  - 新增 `split(self) -> (IpcReadHalf, IpcWriteHalf)` — 零成本 move
  - 新增 `connect(socket_path) -> Result<Self>` — 公共构造器
  - 新增 `IpcTransportError::ConnectFailed(String)` 变体
  - 3 个新测试：`test_connect_to_listener`、`test_connect_nonexistent`、`test_split_read_write`

- [x] `tui/src/ipc.rs` — IPC 客户端实现：
  - `IpcError` 枚举：`Transport`、`Disconnected`、`RequestTimeout`、`RpcError { code, message }`
  - `IpcClient` + `IpcEventReader` 读写分离设计
  - `connect(socket_path)` 返回 `(IpcClient, IpcEventReader)` 元组
  - 内部 spawn read task + write task，通过 mpsc channel 通信
  - 请求-响应关联：`pending: Arc<Mutex<HashMap<u64, oneshot::Sender>>>`
  - `send_request`：30s 超时，返回 `Result<Value, IpcError>`
  - `send_notification`：fire-and-forget
  - `is_connected()`：AtomicBool 断连检测

- [x] `tui/src/main.rs` — 新增 `mod ipc;`

- [x] `tui/Cargo.toml` — `[dev-dependencies]` 新增 `nanoid = "0.4"`

- [x] 7 个测试全覆盖：
  - `test_connect_and_create_session`：connect → send_request("create_session") → session_id 以 "sess_" 开头
  - `test_send_notification`：connect → send_notification → send_request → 正常响应
  - `test_request_error_response`：connect → send_request("nonexistent_method") → RpcError 含 "unknown method"
  - `test_send_request_after_disconnect`：connect → server.shutdown() → is_connected() == false
  - `test_read_message_returns_none_on_disconnect`：connect → drop client → read_message() 返回 None
  - `test_connect_to_nonexistent_socket`：connect("/tmp/nonexistent.sock") → Err(ConnectFailed)
  - `test_multiple_concurrent_requests`：connect → tokio::join!(send_request×2) → 各自正确响应

#### 1.6.4 状态管理：ChatWidget + Keymap

> 目标：聊天面板状态模型（含内联可折叠工具块）、键绑定定义

- [x] `tui/src/chatwidget.rs` — ChatWidget 状态：

```rust
pub struct ChatWidget {
    pub cells: Vec<Box<dyn HistoryCell>>,          // 已提交的消息 cell 列表
    pub active_cell: Option<Box<dyn HistoryCell>>,  // 当前流式活动 cell
    pub input: String,                              // 用户输入缓冲区
    pub input_cursor: usize,                        // 输入光标位置
    pub scroll_offset: usize,                       // 滚动偏移（行数）
    pub auto_scroll: bool,                          // 是否自动滚动到底部
}
```

  - `push_cell(cell)`：提交 cell 到列表，auto_scroll 时重置 scroll_offset
  - `start_streaming(cell)`：设置 active_cell（流式文本开始）
  - `append_streaming(delta)`：向 active_cell 追加 delta
  - `finish_streaming()`：将 active_cell 提交到 cells 列表，清空 active_cell
  - `scroll_up(lines)` / `scroll_down(lines)` / `scroll_to_bottom()`
  - 输入编辑：`input_insert_char` / `input_backspace` / `input_move_cursor` / `input_clear` / `input_submit`
  - `visible_height()`：计算当前可见区域的总行数（cells + active_cell）

- [x] 工具调用可折叠块状态（内联在 ChatWidget 中）：

```rust
pub struct ToolCallCell {
    pub call_id: String,
    pub name: String,
    pub arguments_summary: String,  // TUI 端截断到 80 字符
    pub status: ToolCallStatus,
    pub output: Option<String>,
    pub expanded: bool,             // 是否展开显示完整输出
    pub elapsed_ms: Option<u64>,    // 执行耗时
}

pub enum ToolCallStatus {
    Running,
    Success,
    Error,
}
```

- [x] `tui/src/keymap.rs` — 键绑定定义：

```rust
pub fn handle_key(key: KeyEvent, app: &mut App) -> bool;  // 返回 true 表示已处理
```

  - 全局：Ctrl+C / Ctrl+D → `Quit`，Ctrl+X / Esc → `CancelRequested`
  - 输入框焦点：Enter → 提交消息，Backspace/Ctrl+H → 删除字符，Ctrl+A → 行首，Ctrl+E → 行尾，Ctrl+K → 删除到行尾，Ctrl+U → 删除整行
  - 聊天面板焦点：PageUp/PageDown → 滚动，Ctrl+End → 滚动到底部，Enter/Space → 展开/折叠当前工具块
  - Tab → 切换焦点（聊天面板 ↔ 输入框）
  - Ctrl+P → 切换 Agent 模式（Plan → Code → Ask → Plan...）

- [x] 验证：ChatWidget 单元测试（push/scroll/edit 操作）、Keymap 单元测试（按键映射）

#### 1.6.5 UI 渲染层：各面板 ratatui Widget 实现

> 目标：所有 UI 面板可通过 `render(area, frame)` 渲染到 ratatui Buffer

- [x] `tui/Cargo.toml` 新增 `unicode-width = "0.1"` 依赖

- [x] `tui/src/app.rs` — App 状态结构体 + 枚举定义（`SessionStatus`/`AgentMode`/`FocusTarget`/`App`）+ `new()` 构造器 + `avg_latency()` 方法

- [x] `tui/src/ui/mod.rs` — UI 渲染入口，使用 ratatui `Layout` 组合三区域（状态栏1行 + 聊天面板Min(1) + 输入栏2行）

- [x] `tui/src/ui/status.rs` — 状态栏渲染（单行文本，管道符分隔，P4 icon → P3 latency → P2 tokens → P1 name 优先级裁剪，指示灯带颜色）

- [x] `tui/src/ui/chat.rs` — 聊天面板渲染（`buf.set_line` 逐行渲染，scroll_offset 支持滚动）

- [x] `tui/src/ui/input.rs` — 输入栏渲染（前缀 + textarea 内容 + REVERSED 光标 + 快捷键提示，CJK 字符宽度正确处理）

- [x] `tui/src/ui/tools.rs` — 工具调用渲染辅助（委托 `ToolCallCell::display_lines`/`desired_height`）

- [x] `tui/src/ui/streaming.rs` — 流式文本渲染器（plain-text 换行，宽度变化时重新渲染，为 P2 两区模型预留接口）

- [x] `tui/src/main.rs` — 注册 `mod app; mod ui;`，完整事件循环（快捷键先于 textarea 检查，draw 错误优雅处理，mock 数据验证）

- [x] 验证：25 个新增单元测试通过（status 6 / chat 5 / input 4 / tools 3 / streaming 5 / mod 2），总计89 个测试通过

#### 1.6.6 App 状态机：整合所有组件 + 事件分发

> 目标：App 持有所有子组件状态，实现事件处理方法，为 main.rs 事件循环提供接口

- [ ] `tui/src/app.rs` — App 状态机：

```rust
pub struct App {
    pub chat_widget: ChatWidget,
    pub status: SessionStatus,
    pub session_id: Option<String>,
    pub session_name: Option<String>,       // 模型根据首次输入自动生成
    pub agent_mode: AgentMode,              // Plan / Code / Ask
    pub should_quit: bool,
    pub needs_redraw: bool,
    pub app_event_tx: AppEventSender,
    pub focus: FocusTarget,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cache_read_tokens: i32,
    pub model: String,
}

pub enum FocusTarget {
    ChatPanel,
    Input,
}

pub enum AgentMode {
    Plan,   // 只读分析：Core 修改 system prompt 强制只读行为，不过滤工具（LLM 自觉遵守）
    Code,   // 正常执行所有工具
    Ask,    // 只回答问题：Core 修改 system prompt 禁止工具调用，不过滤工具（LLM 自觉遵守）
}

pub enum SessionStatus {
    Active,    // ●(green) - 正在执行任务
    Completed, // ●(blue) - 任务已完成
    Error,     // ●(red) - 出现错误
}
```

  - `App::new(session_id, app_event_tx)` → 初始化所有子组件
  - `handle_key(key: KeyEvent)`：委托给 keymap 处理，更新状态
  - `handle_paste(text: &str)`：插入到输入缓冲区
  - `handle_ipc_message(msg: JsonRpcMessage)`：解析 JSON-RPC 通知/响应 → 转换为 AppEvent → 通过 `app_event_tx` 发送
    - `assistant_text` → `AppEvent::AssistantDelta`
    - `assistant_reasoning` → `AppEvent::ReasoningDelta`
    - `tool_call_start` → `AppEvent::ToolCallStart`
    - `tool_result` → `AppEvent::ToolResult`
    - `usage` → `AppEvent::Usage`
    - `task_done` → `AppEvent::TaskDone`
    - `error` → `AppEvent::Error`
  - `handle_app_event(event: AppEvent)`：处理内部事件，更新子组件状态
    - `UserMessageSent(text)` → `chat_widget.push_cell(UserMessageCell)` + 通过 IPC 发送 `user_message`
    - `AssistantDelta { delta }` → `chat_widget.append_streaming(delta)` + `needs_redraw = true`
    - `ToolCallStart { .. }` → `chat_widget.finish_streaming()`（结束当前 assistant 文本段）+ `chat_widget.push_cell(ToolCallCell { status: Running })`（中断插入 ToolCallCell）
    - `ToolResult { .. }` → 更新对应 ToolCallCell 的 status/output
    - `TaskDone { .. }` → `chat_widget.finish_streaming()` + 重置流式状态
    - `Usage { .. }` → 累加 `input_tokens` / `output_tokens` / `cache_read_tokens`
    - `Error { .. }` → `chat_widget.push_cell(ErrorCell)`
    - `Quit` → `should_quit = true`
  - `request_redraw()`：设置 `needs_redraw = true`

- [ ] 验证：App 单元测试（构造、事件处理、状态转换），cargo build 通过

### 1.7 TUI: `main.rs` 入口 + 主事件循环

#### 1.7.1 main.rs 入口骨架：初始化流程 + 事件循环框架

> 目标：TUI 可启动、连接 core daemon、创建 session、运行事件循环、优雅退出

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 解析 CLI 参数：--socket-path（覆盖默认 socket 路径）
    let socket_name = resolve_socket_name();

    // 2. 连接 core daemon（在进入 raw mode 之前，连接失败可直接打印错误退出）
    let ipc = IpcClient::connect(&socket_name).await?;

    // 3. 创建 session（传入 cwd 和可选的 API 配置）
    let session = ipc.send_request("create_session", json!({
        "cwd": std::env::current_dir()?.to_string_lossy(),
        "model": null,           // 可选，null 使用全局默认
        "base_url": null,        // 可选
        "api_key": null,         // 可选
    })).await?;
    let session_id = session["session_id"].as_str().unwrap().to_string();

    // 4. 初始化终端（raw mode, bracketed paste, alternate screen）
    let mut tui = Tui::init()?;
    tui.enter_alt_screen();

    // 5. 构建 App 状态
    let (app_event_tx, mut app_event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new(session_id, AppEventSender::new(app_event_tx));

    // 6. 主事件循环（tokio::select! 监听三路事件源）
    // ... 见 1.7.2

    // 7. 清理
    tui.leave_alt_screen();
    Tui::restore();
    ipc.send_notification("destroy_session", json!({"session_id": app.session_id})).await?;
    Ok(())
}
```

- [ ] CLI 参数解析：`--socket-path` 覆盖默认 socket 路径（复用 core 的 `resolve_socket_path`）
- [ ] 初始化顺序：IPC connect → create_session → Tui::init → App::new → 进入事件循环（IPC 连接失败时可直接打印错误退出，不进入 raw mode）
- [ ] 初始化失败处理：IPC 连接超时/拒绝时打印错误信息并退出；TUI 初始化失败时清理 IPC 连接
- [ ] 验证：`cargo build -p crown-tui` 编译通过

#### 1.7.2 事件路由与分发：三路事件源接入

> 目标：`tokio::select!` 同时监听终端事件、IPC 事件、内部事件，正确分发到 App

```rust
let mut draw_interval = tokio::time::interval(Duration::from_millis(50));
let mut tui_events = tui.event_stream();

loop {
    tokio::select! {
        // 终端事件（键盘、粘贴、resize）
        Some(tui_event) = tui_events.next() => {
            match tui_event {
                TuiEvent::Key(key) => app.handle_key(key),
                TuiEvent::Paste(text) => app.handle_paste(&text),
                TuiEvent::Resize => app.request_redraw(),
                TuiEvent::Draw => {}
            }
        }
        // IPC 事件（core daemon 推送）
        Some(msg) = ipc.read_message() => {
            app.handle_ipc_message(msg);
        }
        // 内部事件（子组件通信）
        Some(event) = app_event_rx.recv() => {
            app.handle_app_event(event);
        }
        // 帧刷新
        _ = draw_interval.tick() => {
            // 见 1.7.3
        }
    }
    if app.should_quit { break; }
}
```

- [ ] 终端事件 → `app.handle_key()` / `app.handle_paste()` / `app.request_redraw()`
- [ ] IPC 消息 → `app.handle_ipc_message()`：解析 JSON-RPC → 转换为 AppEvent → 发送到 app_event_tx
- [ ] 内部事件 → `app.handle_app_event()`：更新 ChatWidget / SessionStatus / token 统计
- [ ] IPC 断连处理：`ipc.read_message()` 返回 `None` 时设置 `SessionStatus::Error`，提示用户重连或退出
- [ ] 验证：启动 core daemon + TUI → 输入消息 → 看到事件在三路之间正确流转

#### 1.7.3 帧率控制与渲染：needs_redraw + interval 调度

> 目标：20fps 渲染，needs_redraw 避免空闲时 CPU 空转

```rust
_ = draw_interval.tick() => {
    if app.needs_redraw {
        tui.draw(|frame| {
            ui::render(frame, &app);
        })?;
        app.needs_redraw = false;
    }
}
```

- [ ] `tokio::time::interval(50ms)` 调度重绘（20fps 上限）
- [ ] `needs_redraw` 标志：仅在状态变化时（收到 IPC 消息、用户输入、resize）设置为 true
- [ ] `tui.draw()` 调用 `ui::render(frame, &app)` 完成实际渲染
- [ ] 空闲时 `needs_redraw = false` → 跳过渲染 → CPU 不空转
- [ ] 验证：空闲时 CPU 占用接近 0%，流式输出时 UI 刷新流畅无卡顿

#### 1.7.4 优雅退出与错误恢复

> 目标：各种退出/异常场景下终端状态正确恢复，不留下脏终端

- [ ] **正常退出**：Ctrl+C / Ctrl+D → `AppEvent::Quit` → `should_quit = true` → 退出循环 → `leave_alt_screen` + `restore` + 发送 `destroy_session`
- [ ] **panic 恢复**：设置 `std::panic::set_hook`，panic 时执行 `Tui::restore()` 恢复终端（避免终端卡在 raw mode）
- [ ] **IPC 断连恢复**：
  - 检测到断连 → 设置 `SessionStatus::Error` → UI 显示错误提示
  - 用户按 `R` 键 → 尝试重连（3 次，间隔 1s/2s/4s）→ 成功则恢复 `SessionStatus::Active`
  - 重连失败 → 提示用户重启 core daemon，可选择退出
- [ ] **信号处理**：SIGTERM / SIGINT（Windows: Ctrl+C）触发优雅退出流程
- [ ] **Session 恢复**：TUI 重新连接后可选择新建 session 或恢复已有 session（通过 `list_sessions` 查询）
- [ ] 验证：
  - 正常退出后终端状态正常（无残留 raw mode / alternate screen）
  - kill core daemon → TUI 显示断连 → 重启 core → 按 R 重连成功
  - TUI panic 后终端状态正常恢复

### 1.8 端到端集成验证

- [ ] **启动流程**：终端 A 运行 `crown-core`（daemon 启动，监听 socket）→ 终端 B 运行 `crown-tui`（连接成功，创建 session）
- [ ] **基本对话**：TUI 输入 "hello" → 看到 assistant 流式回复
- [ ] **工具调用**：TUI 输入 "读取 core/src/main.rs" → 看到 tool_call 可折叠块 + 文件内容 + assistant 总结
- [ ] **Multi-session**：终端 B 和终端 C 同时运行 `crown-tui`，连接同一个 core daemon，session 完全隔离
- [ ] **取消任务**：TUI 按 Ctrl+X 取消正在执行的任务
- [ ] **断线恢复**：TUI 异常退出 → 重新连接 → session 可恢复或新建
- [ ] **Daemon 退出**：所有 TUI 断开后 core 自动退出（或保持运行等待新连接）
- [ ] **性能验证**：流式文本延迟 < 50ms，UI 刷新 20fps 无卡顿

---

## Phase 2: Core 功能补全

### 2.1 MCP 工具接入 Agent

- [ ] 新增 `use_mcp_tool` 工具（`server_name`, `tool_name`, `arguments`）
- [ ] 新增 `access_mcp_resource` 工具（`server_name`, `uri`）
- [ ] `agent/tools.rs` 实现执行逻辑，调用 `mcp::registry` + `mcp::client`
- [ ] System prompt 中添加 MCP 工具描述
- [ ] MCP server 懒加载：首次调用时自动初始化连接

### 2.2 ask_followup_question 工具

- [ ] 新增 `ask_followup_question` 工具（`question`, `options[]`）
- [ ] 通过 IPC `ask_followup_question` 事件发送到 TUI
- [ ] TUI 渲染问题和选项列表，用户选择后发送 `answer` 回 core
- [ ] Agent loop 暂停等待用户回答

### 2.3 补充工具

- [ ] `list_code_definition_names` — 列出代码定义（函数、类、变量）
- [ ] `web_fetch` — 获取网页内容
- [ ] `apply_patch` — 应用 unified diff patch

---

## Phase 3: 核心差异化特性

### 3.1 Checkpoint 系统（精细回滚）

- [ ] `core/src/checkpoint.rs` 模块
- [ ] 每次文件写入后自动创建 checkpoint（git commit 或 snapshot）
- [ ] Checkpoint 元数据：时间戳、触发工具、修改文件列表、tool_call_id
- [ ] `rollback_to(checkpoint_id)` API
- [ ] `list_checkpoints()` API
- [ ] 通过 IPC 暴露给 TUI，TUI 显示 checkpoint 列表和回滚操作

### 3.2 Cost Tracking

- [ ] `core/src/cost.rs` 模块
- [ ] 追踪每次 API 调用的 input/output/cache_read tokens
- [ ] 根据模型定价表计算费用
- [ ] 包含 subagent 调用的 token 统计
- [ ] 通过 IPC `usage` 事件实时推送到 TUI
- [ ] TUI 状态栏显示累计费用

---

## Phase 4: 高级特性

### 4.1 Workspace 向量索引

- [ ] 选择嵌入模型（本地或 API）
- [ ] 扫描 workspace 文件生成向量嵌入
- [ ] 索引存储（SQLite / 本地文件）
- [ ] 语义搜索 API + agent 工具

### 4.2 配置系统

- [ ] 配置文件（`~/.crown-code/config.toml`）
- [ ] 多 API provider 切换
- [ ] Model 参数预设
- [ ] TUI 主题配置

### 4.3 Subagent 支持

- [ ] Agent spawn 子 agent 处理子任务
- [ ] 子 agent 独立 history 和 token 统计
- [ ] 父 agent 等待子 agent 完成

### 4.4 Plan Mode / Act Mode

- [ ] Plan Mode：只分析规划，限制为只读工具
- [ ] Act Mode：正常执行
- [ ] 用户可在两种模式间切换

### 4.5 GUI / WebUI 前端

- [ ] Core IPC 扩展支持 TCP/WebSocket
- [ ] GUI 前端（egui / Tauri / Slint）
- [ ] WebUI 前端（WebSocket + React）

---

## 里程碑

| 里程碑 | 内容 | 验收标准 |
|--------|------|----------|
| **M1** | Phase 1 全部完成 | 两个终端同时运行 TUI 连接同一个 core daemon，各自独立完成编码任务 |
| **M2** | Phase 2 全部完成 | MCP 工具可用，agent 能 ask followup question，工具集完整 |
| **M3** | Phase 3 全部完成 | 每次编辑可回滚，费用实时追踪（含 subagent） |
| **M4** | Phase 4.1 + 4.2 | 向量语义搜索可用，配置系统完善 |
| **M5** | Phase 4.3 ~ 4.5 | 多前端支持，高级 agent 模式 |

---

## 文件结构规划（新增部分）

```
core/src/
├── ipc/                        # Phase 1 新增
│   ├── mod.rs                  # 模块导出
│   ├── message.rs              # JSON-RPC 消息类型
│   ├── transport.rs            # 跨平台本地 socket transport (interprocess + tokio)
│   ├── server.rs               # IPC server（accept + 路由 + 广播）
│   └── session_manager.rs      # Session 生命周期管理
├── agent/
│   ├── loop.rs                 # Phase 1 重构：AgentSession + AgentEventHandler
│   └── tools.rs                # Phase 2 新增 MCP/followup 工具
├── checkpoint.rs               # Phase 3 新增
├── cost.rs                     # Phase 3 新增
└── main.rs                     # Phase 1 重构：daemon 入口

tui/src/
├── main.rs                     # Phase 1 新增：入口 + 主事件循环
├── app.rs                      # Phase 1 新增：App 状态机
├── app_event.rs                # Phase 1 新增：AppEvent 枚举（内部消息总线）
├── event.rs                    # Phase 1 新增：TuiEvent 枚举 + 终端事件流
├── ipc.rs                      # Phase 1 新增：IPC 客户端（读写分离）
├── chatwidget.rs               # Phase 1 新增：ChatWidget 聊天面板状态
├── history_cell.rs             # Phase 1 新增：HistoryCell trait + 消息 cell 类型
├── renderable.rs               # Phase 1 新增：Renderable trait + FlexRenderable 布局
├── tui.rs                      # Phase 1 新增：Tui 终端抽象（init/restore/draw）
├── keymap.rs                   # Phase 1 新增：键绑定定义
├── ui/                         # Phase 1 新增
│   ├── mod.rs                  #           UI 渲染入口
│   ├── chat.rs                 #           聊天消息渲染
│   ├── input.rs                #           输入框渲染
│   ├── tools.rs                #           工具调用可折叠块渲染（内联在聊天流中）
│   ├── status.rs               #           状态栏渲染
│   └── streaming.rs            #           流式文本渲染（两区模型）
```

---

## 功能建议（待排期）

### P1 — TUI 体验核心

- [ ] **`/` 斜杠命令**：`/clear` 清空历史、`/compact` 压缩上下文（调用 LLM 总结历史后截断）、`/model` 切换模型。输入框输入 `/` 时弹出自动补全列表。codex 有完整实现可参考。
- [ ] **输入历史导航**：Up/Down 箭头浏览历史输入（类似 shell history），存储在内存中，session 结束丢弃。
- [ ] **执行审批流**：`execute_command` 执行前显示命令内容，用户可 Approve/Reject。参考 codex 的 approval overlay（全屏覆盖层，显示命令 + 预估风险，Enter 执行 / Esc 拒绝）。默认自动执行，后续可通过配置启用审批流。
- [ ] **多行输入**：Shift+Enter 换行（tui-textarea 已支持），粘贴多行内容不自动发送（bracketed paste 检测）。

### P2 — 功能增强

- [ ] **上下文窗口用量**：状态栏或输入框附近显示 context window 占用百分比（`ctx: 45%`），接近 80% 时黄色警告，超过 90% 时红色警告。
- [ ] **Session 持久化**：Session history 保存到磁盘（`~/.crown-code/sessions/`），daemon 重启后可通过 `list_sessions` 恢复。
- [ ] **Diff 预览**：`write_to_file` / `replace_in_file` 执行前在聊天流中显示 unified diff 预览（复用 `xdiff` 模块），用户可 Approve/Reject。
- [ ] **Agent 模式感知提示**：Plan/Ask 模式下，输入框附近显示当前模式的约束说明（如 Plan: "只读分析模式，不会修改文件"）。

### P3 — 未来探索

- [ ] **复制模式**：类 vim visual mode（按 `v` 进入），方向键选择聊天内容，`y` 复制到系统剪贴板。
- [ ] **文件路径自动补全**：输入框中输入 `@` 或 `/` 时弹出文件路径补全（扫描 workspace 文件树）。
- [ ] **主题配置**：用户可自定义颜色方案（`~/.crown-code/theme.toml`），支持 dark/light 切换。
- [ ] **快捷键自定义**：用户可覆盖默认键绑定（`~/.crown-code/keymap.toml`）。
