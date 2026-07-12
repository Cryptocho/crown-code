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

- [ ] `core/Cargo.toml` 新增依赖：
  - `tokio`（rt-multi-thread, net, io-util, sync, macros）
  - `interprocess`（local-socket, tokio feature）— 跨平台本地 IPC
- [ ] `core/src/main.rs` 改为 `#[tokio::main]`
- [ ] 现有阻塞 I/O 全部改为 async：
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
- [ ] 保留 `main.rs` 中一个 CLI fallback 模式（`--cli` 参数），使用 stdin/stdout 直接交互，方便调试

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

- [ ] CLI 参数解析：`--socket-path`、`--cli`（stdin/stdout 模式，调试用）、`--config`
- [ ] 默认进入 daemon 模式（监听本地 socket）
- [ ] `--cli` 模式保留旧的 stdin/stdout 交互（不经过 IPC）

### 1.6 TUI: 基础框架

```
tui/src/
├── main.rs             # 入口：连接 core daemon + 初始化 terminal
├── app.rs              # App 状态机（消息列表、输入框、工具面板、焦点管理）
├── event.rs            # 终端事件源（键盘、resize） + IPC 事件源（core 推送）
├── ipc.rs              # Core daemon IPC 客户端（跨平台本地 socket 连接、JSON-RPC 读写）
├── ui/
│   ├── mod.rs          # UI 渲染入口（compose 各子面板）
│   ├── chat.rs         # 聊天消息面板（可滚动、markdown 渲染）
│   ├── input.rs        # 用户输入框（多行编辑、Enter 发送）
│   ├── tools.rs        # 工具调用面板（折叠/展开、状态标记）
│   └── status.rs       # 状态栏（session_id、模型名、token 用量、连接状态）
```

#### `tui/Cargo.toml` 依赖

```toml
[dependencies]
crown-core = { path = "../core" }   # 仅用于共享类型定义（Message、ToolCall 等），不依赖业务逻辑
ratatui = "0.29"
crossterm = "0.28"
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "sync", "macros"] }
interprocess = { version = "2", features = ["tokio"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
```

**关键**：TUI 仅依赖 core 的**类型定义**（通过 `crown-core` crate 的 `ipc::message` 等模块），不调用 core 的 agent/api/mcp 等业务逻辑。所有业务逻辑在 core daemon 进程中执行。

#### `ipc.rs` — IPC 客户端

```rust
use interprocess::local_socket::tokio::Stream;

pub struct IpcClient {
    stream: Stream,
    // ...
}

impl IpcClient {
    pub async fn connect(socket_name: &str) -> Result<Self, IpcError>;
    pub async fn send_request(&self, method: &str, params: Value) -> Result<Value, IpcError>;
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), IpcError>;
    pub async fn read_message(&self) -> Result<JsonRpcMessage, IpcError>;
}
```

- [ ] 连接时自动检测 socket 是否存在，不存在则提示用户启动 core daemon
- [ ] 断线重连逻辑（socket 断开时尝试重连 3 次，间隔 1s/2s/4s）
- [ ] 读写分离：读消息和写消息分别在独立 tokio task 中运行，避免死锁

#### `app.rs` — App 状态机

```rust
pub struct App {
    pub session_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub input_cursor: usize,
    pub tool_calls: Vec<ToolCallDisplay>,
    pub scroll_offset: usize,
    pub status: ConnectionStatus,
    pub should_quit: bool,
    // ...
}

pub enum ChatMessage {
    User { content: String },
    Assistant { content: String },
    ToolCall { name: String, arguments: String, result: Option<String>, is_error: bool },
    System { content: String },
    Error { code: i32, message: String },
}
```

- [ ] 消息列表：可滚动，自动滚动到底部（新消息到达时）
- [ ] 输入框：支持基本编辑（光标移动、退格、Ctrl+A/E/K/U）
- [ ] 工具调用：显示工具名、参数摘要、执行状态（进行中/完成/失败）
- [ ] 焦点管理：Tab 切换焦点（聊天面板 ↔ 输入框）

#### `event.rs` — 事件处理

```rust
pub enum AppEvent {
    Terminal(crossterm::event::Event),  // 键盘/鼠标/resize
    Ipc(JsonRpcMessage),                // Core 推送的消息
    Tick,                                // 定时刷新
}
```

- [ ] 使用 `tokio::select!` 同时监听终端事件和 IPC 事件
- [ ] 终端事件：`crossterm::event::read()` 在独立 tokio task 中阻塞读取
- [ ] IPC 事件：`ipc_client.read_message()` 在独立 tokio task 中异步读取
- [ ] Tick 事件：`tokio::time::interval(50ms)` 用于 UI 刷新（20fps）

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

- [ ] 左侧主面板占 70% 宽度，右侧工具面板占 30%
- [ ] 工具面板可折叠（按 `T` 键 toggle）
- [ ] 聊天面板支持 PageUp/PageDown 滚动
- [ ] 流式文本实时渲染，不阻塞 UI

### 1.7 TUI: `main.rs` 入口

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 解析 CLI 参数：--socket-path（覆盖默认 socket 路径）
    let socket_name = resolve_socket_name();

    // 2. 连接 core daemon
    let ipc = IpcClient::connect(&socket_name).await?;

    // 3. 创建 session
    let session = ipc.send_request("create_session", json!({
        "cwd": std::env::current_dir()?.to_string_lossy()
    })).await?;
    let session_id = session["session_id"].as_str().unwrap().to_string();

    // 4. 初始化 TUI terminal
    let mut terminal = ratatui::init();

    // 5. 事件循环
    let mut app = App::new(session_id);
    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        match event::next().await? {
            AppEvent::Terminal(event) => app.handle_terminal_event(event),
            AppEvent::Ipc(msg) => app.handle_ipc_message(msg),
            AppEvent::Tick => {}
        }

        if app.should_quit { break; }
    }

    // 6. 清理
    ratatui::restore();
    ipc.send_notification("destroy_session", json!({"session_id": app.session_id})).await?;
    Ok(())
}
```

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
├── main.rs                     # Phase 1 新增
├── app.rs                      # Phase 1 新增
├── event.rs                    # Phase 1 新增
├── ipc.rs                      # Phase 1 新增
└── ui/                         # Phase 1 新增
    ├── mod.rs
    ├── chat.rs
    ├── input.rs
    ├── tools.rs
    └── status.rs
```
