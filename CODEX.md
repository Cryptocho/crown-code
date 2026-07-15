# CODEX.md — Codex CLI (OpenAI) ratatui TUI 参考

## Project Overview

Codex 是 OpenAI 的 coding agent CLI 工具。核心实现为 Rust（`codex-rs/`），Node.js 层（`codex-cli/`）仅作为 npm 分发的薄包装。
路径： codex/codex-rs

**重点关注**：codex-rs 的 TUI 层使用 ratatui + crossterm 实现，是 crown-code TUI 的主要参考。

---

## 架构

```
┌─────────────────────────┐
│  codex-tui (ratatui)    │  ← 前端：渲染、输入、流式显示
├─────────────────────────┤
│  codex-app-server-client│  ← 通信层（in-process 或 remote）
├─────────────────────────┤
│  codex-app-server       │  ← 后端：会话管理、协议处理
├─────────────────────────┤
│  codex-core             │  ← 核心：agent loop、工具执行、sandbox
├─────────────────────────┤
│  codex-api              │  ← API 客户端（Responses API）
└─────────────────────────┘
```

- TUI 通过 `AppServerClient` 与后端通信，支持 in-process（typed tokio channel）和 remote（JSON-RPC over Unix socket/WebSocket）
- 后端可嵌入 TUI 同一进程，也可作为独立 daemon 运行

---

## TUI crate 结构

```
codex-rs/tui/src/
├── main.rs                    # 入口：run_main() → run_ratatui_app()
├── lib.rs                     # 模块注册（200+ mod 声明）
├── cli.rs                     # CLI 参数解析（clap）
├── tui.rs                     # Tui 终端抽象（init/restore/draw/event_stream）
├── tui/
│   ├── event_stream.rs        # EventBroker + TuiEventStream（crossterm 事件源）
│   ├── frame_rate_limiter.rs  # 帧率限制
│   ├── frame_requester.rs     # 帧调度
│   ├── job_control.rs         # Unix SIGTSTP/SIGCONT
│   ├── keyboard_modes.rs      # 键盘增强标志
│   └── terminal_stderr.rs     # alt-screen 时抑制 stderr
├── app.rs                     # App 结构体 + 主事件循环（1376 行）
├── app/
│   ├── event_dispatch.rs      # AppEvent 完整分发
│   ├── input.rs               # 键事件路由
│   ├── app_server_events.rs   # 服务端事件处理
│   ├── app_server_requests.rs # TUI→服务端 RPC
│   ├── session_lifecycle.rs   # 会话 生命周期
│   ├── thread_routing.rs      # 多线程事件路由
│   ├── resize_reflow.rs       # resize 时 transcript 回流
│   └── tests/                 # App 级测试
├── chatwidget.rs              # ChatWidget 主聊天面板（~1200 行）
├── chatwidget/
│   ├── rendering.rs           # FlexRenderable 布局渲染
│   ├── streaming.rs           # 流式生命周期管理
│   ├── constructor.rs         # new()
│   ├── interaction.rs         # 键事件处理
│   ├── user_messages.rs       # 用户输入处理
│   ├── turn_lifecycle.rs      # Agent turn 状态机
│   ├── tool_lifecycle.rs      # 工具调用生命周期
│   ├── protocol.rs            # 协议事件处理
│   ├── transcript.rs          # Transcript 状态
│   ├── tokens.rs              # Token 用量显示
│   └── tests/                 # 20+ 测试模块
├── bottom_pane/
│   ├── mod.rs                 # BottomPane + view stack
│   ├── chat_composer.rs       # 可编辑输入框（多行、历史、slash 命令）
│   ├── footer.rs              # 状态 footer
│   ├── textarea.rs            # 文本区域 widget
│   ├── approval_overlay.rs    # 审批请求覆盖层
│   ├── command_popup.rs       # 命令自动补全弹窗
│   └── scroll_state.rs        # 滚动状态
├── history_cell/
│   ├── mod.rs                 # HistoryCell trait
│   ├── base.rs                # PlainHistoryCell、WarningCell、ErrorCell
│   ├── exec.rs                # ExecCell（工具调用渲染）
│   ├── messages.rs            # AgentMessageCell、UserMessageCell
│   ├── plans.rs               # ProposedPlanCell
│   ├── patches.rs             # PatchHistoryCell
│   └── approvals.rs           # 审批历史 cell
├── streaming/
│   ├── mod.rs                 # StreamState（队列式 markdown 采集）
│   ├── controller.rs          # StreamController + PlanStreamController
│   ├── chunking.rs            # 自适应分块策略
│   └── table_holdback.rs      # 表格流式 holdback
├── render/
│   ├── mod.rs                 # Insets、RectExt
│   ├── renderable.rs          # Renderable trait + FlexRenderable
│   └── highlight.rs           # 语法高亮
├── app_event.rs               # AppEvent 枚举（~100+ variants）
├── app_event_sender.rs        # AppEventSender（unbounded channel）
├── keymap.rs                  # RuntimeKeymap、ChatKeymap
├── markdown_stream.rs         # MarkdownStreamCollector
├── markdown_render.rs         # Markdown 渲染
├── diff_model.rs              # Diff 模型
├── diff_render.rs             # Diff 渲染
├── pager_overlay.rs           # 全屏 overlay（Transcript/Static）
└── session_state.rs           # 会话状态
```

---

## 关键设计：Tui 结构体

`Tui` 是终端抽象层，封装了 crossterm Terminal：

```rust
// tui/src/tui.rs
pub struct Tui {
    frame_requester: FrameRequester,      // 调度重绘
    draw_tx: broadcast::Sender<()>,       // 广播 channel 触发绘制事件
    event_broker: Arc<EventBroker>,       // crossterm 事件源（可暂停/恢复）
    terminal: Terminal,                   // CustomTerminal<CrosstermBackend<Stdout>>
    pending_history_lines: Vec<...>,      // 待插入 scrollback 的行
    alt_screen_active: Arc<AtomicBool>,
    terminal_focused: Arc<AtomicBool>,
    suspend_context: SuspendContext,       // Unix ^Z 处理
    // ...
}
```

**关键方法**：
- `init()` — 进入 raw mode，启用 bracketed paste / focus change
- `draw(height, draw_fn)` — synchronized viewport 更新
- `enter_alt_screen()` / `leave_alt_screen()` — overlay 用
- `with_restored(mode, f)` — 临时恢复终端给外部程序使用
- `event_stream()` → `TuiEventStream` — 合并 crossterm + draw 事件

---

## 关键设计：事件系统

### 三层事件架构

```
┌─ TuiEvent（终端层）──────────────────────────────┐
│  Key(KeyEvent)     → 键盘输入                    │
│  Paste(String)     → 粘贴内容                    │
│  Resize            → 终端大小变化                │
│  Draw              → 计划重绘                    │
└─────────────────────────────────────────────────┘
         ↓
┌─ AppEvent（应用层）──────────────────────────────┐
│  ~100+ variants（内部消息总线）                    │
│  通过 AppEventSender（unbounded channel）发送     │
│  例：FileSearch、SessionLifecycle、SlashCommand   │
└─────────────────────────────────────────────────┘
         ↓
┌─ AppServerEvent（后端层）────────────────────────┐
│  ItemStarted、ItemDelta、ItemCompleted            │
│  TurnStarted、TurnCompleted                       │
│  CommandExecutionApproval（请求审批）              │
└─────────────────────────────────────────────────┘
```

### 主事件循环

```rust
// app.rs — App::run()
loop {
    tokio::select! {
        Some(event) = app_event_rx.recv() => {
            // 内部 widget 事件（slash 命令、session 生命周期等）
            app.handle_event(tui, &mut app_server, event).await;
        }
        active = active_thread_rx.recv() => {
            // 当前线程的缓冲事件（来自 app-server 的通知/请求）
            app.handle_active_thread_event(tui, &mut app_server, event).await;
        }
        event = tui_events.next() => {
            // 终端事件（键盘、粘贴、resize、draw）
            app.handle_tui_event(tui, &mut app_server, event).await;
        }
        app_server_event = app_server.next_event() => {
            // 服务端全局事件（断连等）
            app.handle_app_server_event(&app_server, event).await;
        }
    }
}
```

**crown-code 对应**：我们的事件源只有两个——终端事件（crossterm）和 IPC 事件（core daemon 推送）。可以用 `tokio::select!` 监听 `crossterm event stream` 和 `IPC read_message()`。

---

## 关键设计：渲染管线

### FlexRenderable 布局系统

codex 自定义了 `Renderable` trait 而非直接使用 ratatui 的 `Layout`：

```rust
// render/renderable.rs
pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)>;
    fn cursor_style(&self, area: Rect) -> SetCursorStyle;
}
```

`FlexRenderable` 按 flex 权重分配子区域高度：

```
┌───────────────────────────────────────┐
│  Transcript Area        (flex: 1)     │  ← 填充可用空间
│  历史消息 + 流式输出                    │
│                                       │
├───────────────────────────────────────┤
│  BottomPane             (flex: 0)     │  ← 固定高度
│  ┌─────────────────────────────────┐  │
│  │ Status Footer                    │  │
│  │ ChatComposer (输入框)             │  │
│  │ Key Hints (快捷键提示)            │  │
│  └─────────────────────────────────┘  │
└───────────────────────────────────────┘
```

**crown-code 对应**：TODO 中的设计是单面板线性流（StatusBar + ChatPanel + InputBar），工具调用内联为可折叠块。用 `FlexRenderable` 垂直分割。

### 渲染流程

```
App::render_chat_widget_frame()
  → tui.draw_with_resize_reflow(height, |frame| {
        chat_widget.render(area, frame.buffer);
        // 设置光标位置
    })
  → ChatWidget::render()
    → as_renderable() 构建 FlexRenderable
      → TranscriptAreaRenderable（flex: 1）
      → BottomPaneComposerReserveRenderable（flex: 0）
```

---

## 关键设计：流式输出

### 两区模型

```
┌─────────────────────────┐
│ raw_source (markdown)   │ ← 累积的 markdown 原文
└──────────┬──────────────┘
           │ 按宽度重新渲染
┌──────────▼──────────────┐
│ rendered_lines          │ ← 完整重新渲染
└──────────┬──────────────┘
           │ 分区
┌──────────▼──────────────┐
│ stable region           │ → 提交队列 → scrollback
├─────────────────────────┤
│ tail region             │ → 活动 cell（可变）
└─────────────────────────┘
```

**StreamController** 管理流式生命周期：
1. `emit(delta)` 接收文本增量
2. `push_delta()` 采集器提交完整行 → 重新渲染 → 同步 stable 队列
3. `CommitTick` 动画以固定间隔（~33ms）从队列逐行 drain 到 scrollback，产生打字效果
4. 流结束时 → `ConsolidateAgentMessage` → 用带 source 的 `AgentMarkdownCell` 替换流式 cell

**表格 holdback**：检测到 pipe table 时，从表头开始 hold 在 tail region，直到流结束才提交。

**crown-code 对应**：我们收到 IPC 的 `assistant_text` delta 后，需要类似的两区模型——已提交行固定在 scrollback，当前流式行在 tail 区动态更新。

---

## 关键设计：HistoryCell trait

每个聊天内容块是一个 `HistoryCell`：

```rust
// history_cell/mod.rs
pub(crate) trait HistoryCell: Debug + Send + Sync + Any {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn raw_lines(&self) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn is_stream_continuation(&self) -> bool;
    // ...
}
```

| Cell 类型 | 用途 |
|-----------|------|
| `AgentMessageCell` | 流式 assistant 文本 |
| `AgentMarkdownCell` | 合并后的 markdown（带源码支持 resize 重渲染） |
| `UserMessageCell` | 用户输入 |
| `ExecCell` | 工具调用（命令 + 输出） |
| `McpToolCallCell` | MCP 工具调用 |
| `ProposedPlanCell` | 计划模式输出 |
| `PatchHistoryCell` | 文件变更 patch |
| `PlainHistoryCell` | 系统信息 |
| `ErrorCell` | 错误信息 |

**crown-code 对应**：TODO 中的 `ChatMessage` 枚举可以扩展为 trait-based 的 HistoryCell 系统，支持不同渲染方式。

---

## 关键设计：输入处理

### 键事件路由

```
crossterm::EventStream → TuiEvent → App::handle_tui_event()
  → ChatWidget::handle_key_event()
    → BottomPane 路由
      → 有 overlay/popup? → 路由到活动 BottomPaneView
      → view stack 非空? → 路由到活动 view
      → 否则 → ChatComposer（文本输入）
    → 全局快捷键（Ctrl+C、Ctrl+D 等）在路由前处理
```

### ChatComposer 功能

- 多行文本输入（基于 `tui-textarea`）
- 粘贴突发处理（多行粘贴）
- 历史导航（Up/Down）
- Slash 命令自动补全
- `@` 文件搜索提及
- Vim 模式支持

**crown-code 对应**：初期只需基本的多行输入 + Enter 发送 + Ctrl+C 退出。slash 命令和 @mention 可以后续迭代。

---

## 关键设计：IPC 通信

### AppServerClient 枚举

```rust
pub enum AppServerClient {
    InProcess(InProcessAppServerClient),  // 同进程，typed tokio channel
    Remote(RemoteAppServerClient),        // JSON-RPC over socket
}
```

### 协议方法

| 方法 | 方向 | 用途 |
|------|------|------|
| `thread/start` | Client→Server | 创建对话 |
| `turn/start` | Client→Server | 发送用户输入 |
| `turn/interrupt` | Client→Server | 取消任务 |
| `item/started` | Server→Client | 新 item 开始 |
| `item/delta` | Server→Client | 流式文本增量 |
| `item/completed` | Server→Client | item 完成 |
| `turn/completed` | Server→Client | turn 完成 |

**crown-code 对应**：我们的 IPC 协议已定义在 `core/src/ipc/message.rs`，与 codex 的 JSON-RPC 方案一致，方法名和事件类型已映射好。

---

## 关键依赖

```toml
# codex-rs/tui/Cargo.toml
ratatui = "0.29"      # 终端 UI 框架
crossterm = "0.28"    # 终端后端
tokio = { features = ["rt-multi-thread", "signal", "process", "time"] }
pulldown-cmark = "..."  # markdown 解析
syntect = "..."         # 语法高亮
tui-textarea = "..."    # 多行文本输入组件
vt100 = "..."           # VT100 模拟（测试用）
image = "..."           # 图片渲染
```

---

## crown-code TUI 实现建议

基于 codex 的设计，建议 crown-code TUI 按以下顺序实现：

### 1. 基础框架

```
tui/src/
├── main.rs             # 入口：连接 core daemon + 初始化 terminal
├── app.rs              # App 状态机 + tokio::select! 主事件循环
├── event.rs            # TuiEvent 枚举 + crossterm/IPC 事件合并
├── ipc.rs              # IPC 客户端（连接 core daemon、JSON-RPC 读写）
├── chatwidget.rs       # 聊天面板状态 + 渲染
├── input.rs            # 输入框 + 键事件处理
├── tool_panel.rs       # 工具调用面板
├── ui/
│   ├── mod.rs          # UI 渲染入口
│   ├── chat.rs         # 聊天消息渲染
│   ├── input.rs        # 输入框渲染
│   ├── tools.rs        # 工具面板渲染
│   └── status.rs       # 状态栏渲染
└── history_cell.rs     # 消息 cell 类型定义
```

### 2. 简化的事件循环（参考 codex 但精简）

```rust
loop {
    tokio::select! {
        // 终端事件
        event = tui_events.next() => {
            match event {
                TuiEvent::Key(key) => app.handle_key(key),
                TuiEvent::Paste(text) => app.handle_paste(text),
                TuiEvent::Resize => app.request_redraw(),
                TuiEvent::Draw => app.render(&mut terminal),
            }
        }
        // IPC 事件
        msg = ipc.read_message() => {
            app.handle_ipc_message(msg);
        }
    }
    if app.should_quit { break; }
}
```

### 3. 渲染布局（单面板线性流 + 内联可折叠工具块）

```
┌────────────────────────────────────────────────────────────────┐
│ 我的第一个项目 │ In:1234 Out:567 Cache R:890 │ avg:230ms │ ●   │ ← 状态栏
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ [You] 读取 main.rs                                              │
│                                                                │
│ [Assistant] 我来帮你读取...                                       │
│                                                                │
│   ▶ read_file "core/src/main.rs" → 14 lines           [✓ 0.1s]│ ← 可折叠块（默认折叠）
│                                                                │
│   ▼ read_file "core/src/lib.rs"                      [✓ 0.08s]│ ← 可折叠块（展开状态）
│     1 | pub mod agent;                                         │
│     2 | pub mod api;                                           │
│     ...                                                        │
│                                                                │
│   ⟳ write_to_file "core/src/main.rs"                 [running]│ ← 正在执行
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ gemma4:e4b │ [Code] │ > 请输入你的任务...                        │ ← 输入框
│ Enter 发送 · Ctrl+C 退出 · Tab 切换模式                         │
└────────────────────────────────────────────────────────────────┘
```

**状态栏**：顶部固定 1 行，按优先级自适应裁剪：session 名称 > token 用量(In/Out/Cache R) > API 平均延迟 > session 活动状态(●green=active/●blue=completed/●red=error)

**输入框**：模型名 + Agent 模式标签([Plan]/[Code]/[Ask]) + 用户输入框 + 快捷键提示

### 4. 核心模块优先级

| 优先级 | 模块 | 说明 |
|--------|------|------|
| P0 | `app.rs` + `event.rs` | 主事件循环骨架 |
| P0 | `ipc.rs` | 连接 core daemon |
| P0 | `chatwidget.rs` + `ui/chat.rs` | 聊天消息显示 |
| P0 | `input.rs` + `ui/input.rs` | 用户输入 |
| P0 | `ui/status.rs` | 状态栏 |
| P1 | `ui/tools.rs` + `tool_panel.rs` | 工具面板 |
| P1 | `history_cell.rs` | 类型化消息 cell |
| P2 | 流式渲染优化 | 两区模型、commit animation |
| P2 | markdown 渲染 | pulldown-cmark 集成 |
| P3 | 语法高亮 | syntect 集成 |