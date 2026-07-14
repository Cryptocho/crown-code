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
| `create_session` | `{ "cwd": string }` | 创建新 session，返回 `session_id` |
| `user_message` | `{ "session_id": string, "content": string }` | 用户发送消息 |
| `cancel` | `{ "session_id": string }` | 取消当前任务 |
| `destroy_session` | `{ "session_id": string }` | 销毁 session |
| `list_sessions` | `{}` | 列出所有活跃 session |
| `set_config` | `{ "base_url"?: string, "api_key"?: string, "model"?: string, ... }` | 修改 API 配置（全局） |

#### Core → TUI 事件（Server Push，无需 id）

| method | params | 说明 |
|--------|--------|------|
| `assistant_text` | `{ "session_id": string, "delta": string }` | 流式文本片段（增量） |
| `assistant_reasoning` | `{ "session_id": string, "delta": string }` | 模型推理过程（增量） |
| `tool_call_start` | `{ "session_id": string, "call_id": string, "name": string, "arguments": string }` | 工具调用开始 |
| `tool_result` | `{ "session_id": string, "call_id": string, "name": string, "content": string, "is_error": bool }` | 工具执行结果 |
| `usage` | `{ "session_id": string, "input_tokens": int, "output_tokens": int }` | Token 用量 |
| `task_done` | `{ "session_id": string, "summary": string }` | 任务完成 |
| `error` | `{ "session_id"?: string, "code": int, "message": string }` | 错误信息 |
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
    fn on_usage(&mut self, input_tokens: i32, output_tokens: i32);
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

- [ ] 定义 `AgentEventHandler` trait
- [ ] 将 `run_agent_loop` 重构为 `AgentSession::handle_user_message`
- [ ] 所有 `print!` / `eprintln!` 替换为 `handler.on_*` 调用
- [ ] 所有 `stdin.read_line` 移除，用户输入由外层 IPC 传入
- [ ] 支持 `cancel()` 通过 `AtomicBool` 中断正在进行的 API 调用

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

- [ ] 使用 `interprocess::local_socket::tokio::Listener/Stream`（跨平台）
- [ ] 启动时自动清理残留的 socket 文件（Linux/macOS 需要，Windows named pipe 自动管理）
- [ ] 进程退出时自动清理（signal handler + Drop）
- [ ] 支持 graceful shutdown（SIGTERM/SIGINT / Windows Ctrl+C）

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

- [ ] 每个连接 spawn 一个 tokio task 处理
- [ ] 连接 handler：循环读取消息 → 解析 JSON-RPC → 分发到对应 session → 写回响应/事件
- [ ] 广播能力：session 产生的事件通过该 session 对应的连接推送

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

- [ ] session_id 生成：`nanoid` 或自增 ID + 随机后缀
- [ ] Session 持有独立的 `AgentSession` 实例（独立 history、独立 API client）
- [ ] 所有 session 共享同一份 API config（可通过 `set_config` 全局修改）

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

- [ ] CLI 参数解析：`--socket-path`（覆盖默认 socket 路径）、`--config`（配置文件路径）
- [ ] 默认进入 daemon 模式（监听本地 socket）

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
│   ├── tools.rs        # 工具调用面板渲染
│   ├── status.rs       # 状态栏渲染（session_id、模型名、token 用量、连接状态）
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

FlexRenderable 按 flex 权重分配子区域高度，实现自适应布局：

```
┌──────────────────────────────────────┬────────────────────┐
│  ChatPanel (flex: 1)                 │  ToolPanel (flex: 0)│
│  聊天消息 + 流式输出                   │  工具调用列表        │
│  填充可用空间                          │  固定宽度 30%       │
│                                      │                     │
├──────────────────────────────────────┴────────────────────┤
│  InputBar (flex: 0)                  固定高度              │
│  用户输入框 + 状态提示                                       │
└──────────────────────────────────────────────────────────┘
```

##### 3. HistoryCell trait（类型化消息渲染）

```rust
// history_cell.rs
pub trait HistoryCell: Debug + Send + Sync {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn is_stream_continuation(&self) -> bool;
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

```
┌────────────────────────────────────────────────────────────────┐
│ crown-code │ sess:abc │ gemma4:e4b │ In:1234 Out:567 │  ● ● ● │ ← 状态栏
├──────────────────────────────────────┬─────────────────────────┤
│                                      │  Tool Calls             │
│ [You] 读取 main.rs 并添加配置加载逻辑   │                         │
│                                      │  ✓ read_file            │
│ [Assistant] 我来帮你实现。首先读取文件... │    "core/src/main.rs"  │
│                                      │    → 14 lines           │
│   ── read_file ──                    │                         │
│   1 | use crown_core::...;           │  ✗ write_to_file        │
│   2 | use crown_core::...;           │    (running...)         │
│   ...                                │                         │
│                                      │                         │
│ [Assistant] 文件结构清楚了，现在添加配置  │                         │
│ 加载逻辑...                           │                         │
│                                      │                         │
│   ── write_to_file ── (running...)   │                         │
│                                      │                         │
├──────────────────────────────────────┴─────────────────────────┤
│ > 请输入你的任务... (Enter 发送, Ctrl+C 退出, Tab 切换焦点)       │ ← 输入框
└────────────────────────────────────────────────────────────────┘
```

#### 1.6.1 项目脚手架：依赖 + Tui 终端抽象 + TuiEvent

> 目标：TUI crate 可编译，终端能进入/退出 raw mode，键盘事件可异步读取

- [ ] `tui/Cargo.toml` 添加依赖：

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

- [ ] `tui/src/tui.rs` — Tui 终端抽象：
  - `Tui::init()`：进入 raw mode、启用 bracketed paste、切换 alternate screen
  - `Tui::restore()`：恢复终端状态（Drop 自动调用）
  - `Tui::enter_alt_screen()` / `leave_alt_screen()`
  - `Tui::draw<F>(&mut self, f: F)`：渲染帧 + 同步 viewport
  - `Tui::event_stream()`：返回 `impl Stream<Item = TuiEvent>`，内部 spawn tokio task 读取 crossterm `EventStream`，将 `crossterm::event::Event` 转换为 `TuiEvent` 发送到 mpsc channel
  - 终端大小查询：`Tui::size()` → `Rect`

- [ ] `tui/src/event.rs` — TuiEvent 枚举：

```rust
pub enum TuiEvent {
    Key(KeyEvent),     // 键盘输入
    Paste(String),     // 粘贴内容（bracketed paste）
    Resize,            // 终端大小改变
    Draw,              // 计划重绘（由帧率限制器调度）
}
```

- [ ] 验证：`cargo build -p crown-tui` 编译通过

#### 1.6.2 事件与类型基础：AppEvent + HistoryCell + Renderable

> 目标：定义三层事件类型、消息 cell 类型体系、布局 trait

- [ ] `tui/src/app_event.rs` — AppEvent 枚举 + AppEventSender：

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
    Usage { input_tokens: i32, output_tokens: i32 },
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

- [ ] `tui/src/history_cell.rs` — HistoryCell trait + 5 种 cell 类型：

```rust
pub trait HistoryCell: Debug + Send + Sync {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn is_stream_continuation(&self) -> bool;
}
```

  - `UserMessageCell`：用户消息，`[You]` 前缀 + 内容
  - `AssistantMessageCell`：assistant 回复，支持流式追加 delta（`append_delta(&mut self, delta: &str)`）
  - `ToolCallCell`：工具调用（工具名 + 参数摘要 + 输出 + 状态：Running/Success/Error）
  - `SystemMessageCell`：系统消息（灰色斜体）
  - `ErrorCell`：错误消息（红色）

- [ ] `tui/src/renderable.rs` — Renderable trait + FlexRenderable 布局引擎：

```rust
pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)>;
}
```

  - `FlexRenderable`：按 flex 权重分配子区域高度/宽度，替代直接使用 ratatui `Layout`
  - 支持水平分割（chat:tool panel = flex:fixed）和垂直分割（content:input = flex:fixed）

- [ ] 验证：模块可编译，HistoryCell 各类型单元测试（display_lines 宽度换行、desired_height 计算）

#### 1.6.3 IPC 客户端：连接 core daemon + 读写分离

> 目标：TUI 能连接 core daemon socket，双向收发 JSON-RPC 消息

- [ ] `tui/src/ipc.rs` — IpcClient：

```rust
pub struct IpcClient {
    write_tx: mpsc::UnboundedSender<JsonRpcMessage>,  // 写端：独立 task 负责发送
    read_rx: mpsc::UnboundedReceiver<JsonRpcMessage>,  // 读端：独立 task 负责接收
    next_id: AtomicU64,                                // 请求 ID 自增
}

impl IpcClient {
    pub async fn connect(socket_name: &str) -> Result<Self, IpcError>;
    pub async fn send_request(&self, method: &str, params: Value) -> Result<Value, IpcError>;
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), IpcError>;
    pub async fn read_message(&mut self) -> Option<JsonRpcMessage>;
    pub fn is_connected(&self) -> bool;
}
```

  - 读写分离：`connect()` 内部 spawn 两个 tokio task（读 task + 写 task），通过 mpsc channel 与主循环通信
  - 读 task：循环 `BufReader::read_line` → `serde_json::from_str` → 发送到 `read_rx`
  - 写 task：从 `write_rx` 接收 → `serde_json::to_string` + `\n` → `BufWriter::write_all` + `flush`
  - socket 检测：连接前检查 socket 文件是否存在，不存在返回明确错误（提示用户启动 core daemon）
  - 断线重连：读 task 检测到 EOF/错误时，通过 channel 通知主循环连接断开；主循环负责重连决策
  - `send_request`：自增 `next_id`，发送后等待匹配 `id` 的响应（通过 oneshot channel 或直接在 read_rx 中匹配）

- [ ] 复用 core 的 `ipc::message` 类型（通过 `crown-core` crate 依赖）

- [ ] 验证：手动启动 core daemon → 运行 TUI IPC 客户端 → 发送 `create_session` → 收到 `session_id` 响应

#### 1.6.4 状态管理：ChatWidget + ToolPanel + Keymap

> 目标：聊天面板和工具面板的状态模型，键绑定定义

- [ ] `tui/src/chatwidget.rs` — ChatWidget 状态：

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

- [ ] 工具面板状态（内联在 `app.rs` 中或独立文件）：

```rust
pub struct ToolPanel {
    pub calls: Vec<ToolCallDisplay>,
    pub visible: bool,
    pub scroll_offset: usize,
}

pub struct ToolCallDisplay {
    pub call_id: String,
    pub name: String,
    pub arguments_summary: String,  // 截断的参数摘要
    pub status: ToolCallStatus,
    pub output: Option<String>,
}

pub enum ToolCallStatus {
    Running,
    Success,
    Error,
}
```

- [ ] `tui/src/keymap.rs` — 键绑定定义：

```rust
pub fn handle_key(key: KeyEvent, app: &mut App) -> bool;  // 返回 true 表示已处理
```

  - 全局：Ctrl+C / Ctrl+D → `Quit`，Ctrl+X / Esc → `CancelRequested`
  - 输入框焦点：Enter → 提交消息，Backspace/Ctrl+H → 删除字符，Ctrl+A → 行首，Ctrl+E → 行尾，Ctrl+K → 删除到行尾，Ctrl+U → 删除整行
  - 聊天面板焦点：PageUp/PageDown → 滚动，Ctrl+End → 滚动到底部
  - Tab → 切换焦点（聊天面板 ↔ 输入框）
  - `T` → toggle 工具面板折叠/展开（仅在输入框非焦点时）

- [ ] 验证：ChatWidget 单元测试（push/scroll/edit 操作）、Keymap 单元测试（按键映射）

#### 1.6.5 UI 渲染层：各面板 ratatui Widget 实现

> 目标：所有 UI 面板可通过 `render(area, frame)` 渲染到 ratatui Buffer

- [ ] `tui/src/ui/mod.rs` — UI 渲染入口：

```rust
pub fn render(frame: &mut Frame, app: &App);
```

  - 使用 FlexRenderable（或 ratatui Layout）组合三区域：
    - 顶部：状态栏（固定 1 行）
    - 中部：聊天面板（flex:1）+ 工具面板（固定 30% 宽度，可折叠）
    - 底部：输入栏（固定 3 行）

- [ ] `tui/src/ui/status.rs` — 状态栏渲染：

```
│ crown-code │ sess:abc │ gemma4:e4b │ In:1234 Out:567 │  ● ● ● │
```

  - 显示：项目名、session_id（截断）、模型名、累计 token 用量、连接状态指示灯
  - 连接状态：`●`(green) = Connected，`●`(yellow) = Reconnecting，`●`(red) = Disconnected

- [ ] `tui/src/ui/chat.rs` — 聊天消息渲染：

  - 遍历 `ChatWidget.cells` + `active_cell`，对每个 cell 调用 `display_lines(width)`
  - 根据 `scroll_offset` 裁剪可见区域
  - 各 cell 类型样式：
    - `[You]` 前缀（cyan bold）+ 用户消息
    - `[Assistant]` 前缀（green bold）+ assistant 回复
    - 工具调用：`── tool_name ──` 标题 + 输出内容 + 状态标记
    - 系统消息：灰色斜体
    - 错误消息：红色

- [ ] `tui/src/ui/input.rs` — 输入框渲染：

```
│ > 请输入你的任务... (Enter 发送, Ctrl+C 退出, Tab 切换焦点)       │
```

  - 显示用户输入文本 + 光标位置
  - 输入为空时显示占位提示文本（灰色）
  - 多行支持：输入超过一行时自动换行
  - 底部快捷键提示行

- [ ] `tui/src/ui/tools.rs` — 工具面板渲染：

```
│  Tool Calls             │
│  ✓ read_file            │
│    "core/src/main.rs"   │
│    → 14 lines           │
│  ✗ write_to_file        │
│    (running...)         │
```

  - 每个工具调用一行：状态图标（✓/✗/⟳）+ 工具名
  - 展开时显示：参数摘要 + 输出预览
  - 正在执行的工具高亮显示
  - 可折叠（`ToolPanel.visible` 控制）

- [ ] `tui/src/ui/streaming.rs` — 流式文本渲染（两区模型）：

  - `StreamingRenderer`：管理 raw_source → rendered_lines → stable/tail 分区
  - `append_delta(delta: &str)`：累积 raw_source，触发重新渲染
  - `render(area, frame, scroll_offset)`：渲染当前帧
  - stable region：已确认的行，逐帧 drain 产生打字效果
  - tail region：活动流式文本，持续更新
  - 宽度变化时自动重新渲染（Resize 事件触发）

- [ ] 验证：各 UI 模块编译通过，可在 test 中构造 mock 数据调用 `render()` 检查输出 buffer

#### 1.6.6 App 状态机：整合所有组件 + 事件分发

> 目标：App 持有所有子组件状态，实现事件处理方法，为 main.rs 事件循环提供接口

- [ ] `tui/src/app.rs` — App 状态机：

```rust
pub struct App {
    pub chat_widget: ChatWidget,
    pub tool_panel: ToolPanel,
    pub status: ConnectionStatus,
    pub session_id: Option<String>,
    pub should_quit: bool,
    pub needs_redraw: bool,
    pub app_event_tx: AppEventSender,
    pub focus: FocusTarget,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub model: String,
}

pub enum FocusTarget {
    ChatPanel,
    Input,
}

pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting(u32),
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
    - `ToolCallStart { .. }` → `tool_panel.calls.push(ToolCallDisplay { status: Running })`
    - `ToolResult { .. }` → 更新对应 ToolCallDisplay 的 status/output
    - `TaskDone { .. }` → `chat_widget.finish_streaming()` + 重置流式状态
    - `Usage { .. }` → 累加 `input_tokens` / `output_tokens`
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

    // 2. 初始化终端（raw mode, bracketed paste, alternate screen）
    let mut tui = Tui::init()?;
    tui.enter_alt_screen();

    // 3. 连接 core daemon
    let ipc = IpcClient::connect(&socket_name).await?;

    // 4. 创建 session
    let session = ipc.send_request("create_session", json!({
        "cwd": std::env::current_dir()?.to_string_lossy()
    })).await?;
    let session_id = session["session_id"].as_str().unwrap().to_string();

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
- [ ] 初始化顺序：Tui::init → IPC connect → create_session → App::new → 进入事件循环
- [ ] 初始化失败处理：连接超时/拒绝时显示错误信息并退出（不进入 raw mode）
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
- [ ] 内部事件 → `app.handle_app_event()`：更新 ChatWidget / ToolPanel / 状态
- [ ] IPC 断连处理：`ipc.read_message()` 返回 `None` 时设置 `ConnectionStatus::Disconnected`，提示用户重连或退出
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
  - 检测到断连 → 设置 `ConnectionStatus::Disconnected` → UI 显示断连提示
  - 用户按 `R` 键 → 尝试重连（3 次，间隔 1s/2s/4s）→ 成功则恢复 `Connected`
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
- [ ] **工具调用**：TUI 输入 "读取 core/src/main.rs" → 看到 tool_call 面板 + 文件内容 + assistant 总结
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
│   ├── tools.rs                #           工具面板渲染
│   ├── status.rs               #           状态栏渲染
│   └── streaming.rs            #           流式文本渲染（两区模型）
```
