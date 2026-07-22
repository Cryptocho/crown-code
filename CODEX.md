# Codex TUI 架构分析

> 基于 `codex/codex-rs/tui/` 源码的深度分析，专注于 TUI 部分如何使用 ratatui 渲染各种消息。

## 1. 概览

Codex TUI 是一个基于 ratatui 的终端聊天界面，核心功能是将 LLM 对话流实时渲染为终端 UI。整个渲染管线分为四层：

```
┌─────────────────────────────────────────────────────────────┐
│  App (app.rs)              事件循环 + 状态管理                │
│  ┌─────────────────────────────────────────────────────────┐│
│  │  ChatWidget (chatwidget.rs)   主聊天面板编排              ││
│  │  ┌──────────────────────┬──────────────────────────────┐││
│  │  │  HistoryCell[]       │  BottomPane                  │││
│  │  │  历史消息单元         │  底部交互面板                  │││
│  │  └──────────────────────┴──────────────────────────────┘││
│  └─────────────────────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────────────────────┐│
│  │  Tui (tui.rs)            Terminal 封装 + 帧调度          ││
│  │  custom_terminal.rs      ratatui Terminal 的定制版本      ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

## 2. 终端初始化与帧循环

### 2.1 Terminal 初始化 (`tui.rs:391`)

```rust
pub(crate) fn init() -> Result<InitializedTerminal>
```

- 使用 `CrosstermBackend<Stdout>` 作为 ratatui 后端
- 启用 raw mode、bracketed paste、keyboard enhancement、focus change
- 通过 `CustomTerminal::with_options_and_cursor_position()` 创建终端，支持 inline viewport（非 alt-screen 模式下内容显示在正常滚动区域）
- 启动时探测终端能力（cursor position、default colors、keyboard enhancement support）

### 2.2 帧调度机制

`Tui` 结构体通过 `FrameRequester` + `broadcast::channel` 调度帧重绘：

```rust
pub struct Tui {
    frame_requester: FrameRequester,    // 请求下一帧
    draw_tx: broadcast::Sender<()>,     // 帧信号广播
    terminal: Terminal,                 // ratatui Terminal
    pending_history_lines: Vec<PendingHistoryLines>,  // 待写入的历史行
    ...
}
```

`TuiEvent` 枚举驱动事件循环：

```rust
pub enum TuiEvent {
    Key(KeyEvent),   // 键盘事件
    Paste(String),   // 粘贴事件
    Resize,          // 终端大小变化
    Draw,            // 定时重绘
}
```

### 2.3 绘制流程 (`tui.rs:895`)

```rust
pub fn draw(&mut self, height: u16, draw_fn: impl FnOnce(&mut Frame)) -> Result<()>
```

使用 `crossterm::SynchronizedUpdate` 包裹整个绘制操作，避免画面撕裂：

1. 处理 ^Z 恢复（Unix job control）
2. 更新 inline viewport 大小（如果终端尺寸变化）
3. 写入 `pending_history_lines` 到终端滚动区域上方
4. 调用 `terminal.draw(draw_fn)` 渲染当前帧

### 2.4 Inline Viewport vs Alt-Screen

Codex TUI 默认使用 **inline viewport** 模式——内容渲染在终端当前光标位置以下，上方的历史内容留在终端滚动缓冲区。`enter_alt_screen()` / `leave_alt_screen()` 可切换到全屏模式（用于 resume picker、overlay 等）。

```rust
pub fn enter_alt_screen(&mut self) -> Result<()> {
    execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
    execute!(self.terminal.backend_mut(), EnableAlternateScroll)?;
    // 保存当前 viewport，扩展到全屏
}
```

## 3. App 层：事件驱动的主循环

### 3.1 App 结构体 (`app.rs:507`)

`App` 是顶层状态持有者：

```rust
pub(crate) struct App {
    pub(crate) chat_widget: ChatWidget,           // 主聊天面板
    pub(crate) transcript_cells: Vec<Arc<dyn HistoryCell>>,  // 已提交的历史单元
    pub(crate) overlay: Option<Overlay>,          // 分页覆盖层 (Ctrl+T 转录)
    pub(crate) keymap: RuntimeKeymap,             // 运行时键位映射
    pub(crate) config: Config,                    // 配置
    thread_event_channels: HashMap<ThreadId, ThreadEventChannel>,
    active_thread_id: Option<ThreadId>,
    ...
}
```

### 3.2 主事件循环 (`app.rs:1186`)

```rust
loop {
    select! {
        Some(event) = app_event_rx.recv() => { ... }         // 内部事件
        active = active_thread_rx.recv() => { ... }           // 线程事件
        event = tui_events.next() => { ... }                  // 终端事件
        app_server_event = app_server.next_event() => { ... } // 后端事件
    }
}
```

### 3.3 绘制编排 (`app.rs:1366`)

```rust
fn render_chat_widget_frame(&mut self, tui: &mut Tui) -> Result<Rect> {
    let desired_height = self.chat_widget.desired_height(width);
    tui.draw_with_resize_reflow(desired_height, |frame| {
        self.chat_widget.render(area, frame.buffer);
        if let Some((x, y)) = self.chat_widget.cursor_pos(area) {
            frame.set_cursor_position((x, y));
        }
    })?;
}
```

`ChatWidget` 实现了 `Renderable` trait，直接通过 `render(area, buf)` 写入 ratatui `Buffer`。

## 4. Renderable Trait 体系 (`render/renderable.rs`)

Codex 定义了自己的 `Renderable` trait，扩展了 ratatui 的 `Widget`：

```rust
pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, _area: Rect) -> Option<(u16, u16)> { None }
    fn cursor_style(&self, _area: Rect) -> SetCursorStyle { ... }
}
```

核心布局组件：

| 组件 | 用途 |
|------|------|
| `ColumnRenderable` | 垂直堆叠子组件，逐个分配高度 |
| `FlexRenderable` | 弹性布局（类似 Flutter Flex），支持 flex 因子 |
| `RowRenderable` | 水平排列子组件 |
| `InsetRenderable` | 为子组件添加内边距 |

所有 ratatui 基础类型（`Line`、`Span`、`Paragraph`、`String`）都实现了 `Renderable`。

## 5. HistoryCell：消息渲染的核心抽象

### 5.1 trait 定义 (`history_cell/mod.rs:189`)

```rust
pub(crate) trait HistoryCell: Debug + Send + Sync + Any {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;      // 主视图
    fn raw_lines(&self) -> Vec<Line<'static>>;                       // 原始文本（复制用）
    fn display_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine>;  // 含超链接
    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>>;   // 转录覆盖层
    fn desired_height(&self, width: u16) -> u16;                     // 所需高度
    fn transcript_animation_tick(&self) -> Option<u64> { None }      // 动画 tick
}
```

`HistoryCell` 是 TUI 中所有消息类型的统一接口。每种消息类型实现该 trait，负责自己的渲染逻辑。

### 5.2 基础实现 (`history_cell/base.rs`)

| 类型 | 用途 |
|------|------|
| `PlainHistoryCell` | 纯文本行，无特殊处理 |
| `WebHyperlinkHistoryCell` | 自动检测并标注 URL 超链接 |
| `PrefixedWrappedHistoryCell` | 带前缀的自动换行文本（如 `"› "` 用户输入、`"⚠ "` 警告） |
| `CompositeHistoryCell` | 组合多个子 cell（如 session header + help text） |

### 5.3 消息类型渲染

#### 用户消息 (`history_cell/messages.rs`)

```rust
pub(crate) struct UserHistoryCell {
    pub message: String,
    pub text_elements: Vec<TextElement>,  // 带样式的文本元素（如 @mention）
    pub local_image_paths: Vec<PathBuf>,
    pub remote_image_urls: Vec<String>,
}
```

渲染逻辑：
- 使用 `user_message_style()` 设置背景色（根据终端背景色自适应明暗）
- 用户文本前缀 `"› ".bold().dim()`
- 支持 `text_elements` 中的 cyan 高亮（如文件引用）
- 图片以 `"[Image N]"` 标签显示
- 使用 `textwrap::WrapAlgorithm::FirstFit` 进行自动换行

#### 助手消息 (`history_cell/messages.rs`)

两种形式：
1. **`AgentMessageCell`**：流式阶段的增量消息，每行以 `"• ".dim()` 或 `"  "` 缩进
2. **`AgentMarkdownCell`**：流完成后合并的源码级 markdown cell，支持终端 resize 时重新渲染

```rust
pub(crate) struct AgentMarkdownCell {
    markdown_source: String,   // 原始 markdown 源码
    cwd: PathBuf,              // 快照的 cwd（用于本地文件链接显示）
}
```

渲染时调用 `render_markdown_agent_with_links_cwd_and_visualizations()` 从源码重新生成 styled lines。

#### 推理摘要 (`history_cell/messages.rs`)

```rust
pub(crate) struct ReasoningSummaryCell {
    content: String,
    cwd: PathBuf,
    transcript_only: bool,
}
```

以 `Style::default().dim().italic()` 渲染，前缀 `"• "`，仅在转录覆盖层中显示（默认隐藏）。

### 5.4 工具调用渲染

#### 命令执行 (`history_cell/exec.rs`)

`UnifiedExecInteractionCell` 渲染后台终端交互：
```
↳ Interacted with background terminal · <command>
  └ <stdin content>
```

`UnifiedExecProcessesCell` 渲染 `/ps` 命令输出：
```
Background terminals

  • <command snippet> [...]
    ↳ <recent output chunk>
```

使用 `.bold()`、`.dim()`、`.cyan()` 样式，`take_prefix_by_width()` 处理截断。

#### 补丁应用 (`history_cell/patches.rs`)

`PatchHistoryCell` 调用 `create_diff_summary()` 生成文件级 diff 摘要。失败时使用 `"✘ Failed to apply patch".magenta().bold()` 标题。

#### 计划更新 (`history_cell/plans.rs`)

`PlanUpdateCell` 渲染 checkbox 风格的计划：
```
• Updated Plan
  └ <explanation>
  └ ✔ <completed step>     // crossed_out + dim
  └ □ <in-progress step>   // cyan + bold
  └ □ <pending step>       // dim
```

`ProposedPlanCell` 渲染带背景色的完整计划块，使用 `proposed_plan_style()` 自适应背景。

#### MCP 工具调用 (`history_cell/mcp.rs`)

支持活跃/完成/错误三种状态的 MCP 工具调用渲染。

#### 网页搜索 (`history_cell/search.rs`)

`WebSearchCell` 渲染搜索活动：
- 活跃中：使用 `activity_indicator()` 生成动画 bullet（shimmer 或 blink）
- 完成后：静态 `"•"` bullet

#### 审批状态 (`history_cell/approvals.rs`)

使用 `PrefixedWrappedHistoryCell` + 符号前缀：
- 批准：`"✔ ".green()` + 绿色
- 拒绝：`"✗ ".red()` + 红色
- 支持 User / Guardian 两种 actor

#### 通知/警告 (`history_cell/notices.rs`)

- 警告：`"⚠ ".yellow()` + `PrefixedWrappedHistoryCell`
- 错误：`"■ <message>".red()`
- 信息：`"• ".dim()` + 消息文本
- 更新通知：带边框的卡片（`with_border()`），包含版本号、更新命令、release notes 链接
- 安全拦截：`"ⓘ ".cyan()` + 标题 + 说明 + 链接

#### 分隔符 (`history_cell/separators.rs`)

`FinalMessageSeparator` 在 turn 之间渲染水平线：
```
─ Worked for 2m 15s • Local tools: 3 calls (1.2s) ───────
```

无额外信息时渲染纯 `"─"` 行。

#### 会话头部 (`history_cell/session.rs`)

`SessionHeaderHistoryCell` 渲染带边框的启动卡片：
```
╭──────────────────────────────────────╮
│ >_ OpenAI Codex (v0.1.0)            │
│                                      │
│ model: o3   /model to change         │
│ directory: ~/projects/myapp          │
│ permissions: YOLO mode               │  // 仅在 YOLO 模式
╰──────────────────────────────────────╯
```

边框使用 `"╭╮╰╯─│"` box-drawing 字符，通过 `with_border()` 函数统一处理。

### 5.5 Renderable for HistoryCell

`Box<dyn HistoryCell>` 实现了 `Renderable`：

```rust
impl Renderable for Box<dyn HistoryCell> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let hyperlink_lines = self.display_hyperlink_lines(area.width);
        let lines = visible_lines(hyperlink_lines.clone());
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        // 如果内容超出 area，滚动到最底部
        let y = paragraph.line_count(area.width).saturating_sub(area.height);
        Clear.render(area, buf);  // 清除旧内容
        paragraph.scroll((y, 0)).render(area, buf);
        mark_buffer_hyperlinks(buf, area, &hyperlink_lines, y);  // 标注 OSC 8 超链接
    }
}
```

## 6. Markdown 渲染 (`markdown_render.rs`)

### 6.1 渲染管线

使用 `pulldown-cmark` 解析 markdown 事件，转换为 ratatui `Line`/`Span`：

```
Markdown Source → pulldown-cmark Parser → Event stream → styled ratatui Lines
```

### 6.2 样式映射 (`MarkdownStyles`)

| 元素 | 样式 |
|------|------|
| h1 | `bold().underlined()` |
| h2 | `bold()` |
| h3 | `bold().italic()` |
| h4-h6 | `italic()` |
| code (inline) | `cyan()` |
| emphasis | `italic()` |
| strong | `bold()` |
| strikethrough | `crossed_out()` |
| ordered list marker | `light_blue()` |
| link | `cyan().underlined()` |
| blockquote | `green()` |

### 6.3 表格渲染

表格渲染有独立管线（`render_table_lines`）：
1. 过滤 spillover 行
2. 规范化列数
3. 计算列宽（按内容类型分类：Narrative / TokenHeavy / Compact）
4. 选择展示方式（列式 or 转置为 key/value）
5. 使用 `"━"` (header) / `"─"` (body) 分隔线

### 6.4 流式 Markdown 渲染 (`streaming/render.rs`)

`StreamingRender` 实现增量渲染：
- 保留已完成的顶层 block（stable prefix）
- 仅重新渲染最后一个 block（mutable tail）
- reference-style link definitions 触发全量重渲染

### 6.5 代码高亮 (`render/highlight.rs`)

使用 `syntect` + `two-face` 进行语法高亮，将 syntect 主题 scope 映射到 ratatui `Style`。

## 7. Diff 渲染 (`diff_render.rs`)

### 7.1 渲染特性

- 使用 `diffy` 生成 unified diff
- 每行前缀：右对齐行号 + gutter sign (`+`/`-`/` `) + 内容
- 支持 syntax highlighting（按 hunk 整体高亮，保持跨行状态）
- 长行硬换行，syntax span 在字符边界分割

### 7.2 主题自适应

`DiffTheme` 根据终端背景色自动选择 Dark/Light 主题：

| | Dark | Light |
|---|---|---|
| Add 行背景 | `#213A2B` | `#dafbe1` |
| Del 行背景 | `#4A221D` | `#ffebe9` |
| 行号背景 | 同上 | 更饱和 (`#aceebb` / `#ffcecb`) |

支持 TrueColor / Ansi256 / Ansi16 三级色深。

## 8. 流式消息系统 (`streaming/`)

### 8.1 架构

```
StreamState (mod.rs)
├── MarkdownStreamCollector   # 基于换行的 markdown 收集器
├── QueuedLine[]              # FIFO 提交队列（带入队时间戳）
│
├── StreamController (controller.rs)     # 助手消息流
│   ├── StreamCore                       # 共享状态
│   │   ├── raw_source: String           # 累积的原始 markdown
│   │   ├── render: StreamingRender      # 增量渲染状态
│   │   ├── stable/tail 分区             # 已提交/可变尾部
│   │   └── TableHoldbackScanner         # 表格检测器
│   └── emit() → Box<dyn HistoryCell>    # 生成 cell
│
├── PlanStreamController (controller.rs) # 计划流
│
├── AdaptiveChunkingPolicy (chunking.rs) # 自适应分块策略
│   ├── Smooth 模式: 每 tick 排出 1 行
│   └── CatchUp 模式: 一次性排出所有积压
│
└── commit_tick.rs                       # 协调分块策略与控制器
```

### 8.2 Two-Region 流模型

流将渲染内容分为两个区域：
- **Stable region**：已提交到 `StreamState` 队列的行，通过动画队列逐步写入滚动缓冲区
- **Tail region**：可变的活跃 cell（`StreamingAgentTailCell` / `StreamingPlanTailCell`），在 `ChatWidget.active_cell` 插槽中显示

### 8.3 表格回退 (Table Holdback)

表格渲染是非增量的——新增一行可能改变所有列宽。`TableHoldbackScanner` 检测 pipe-table 模式，将从表头开始的内容保留在 tail 中，直到流结束。

### 8.4 自适应分块 (`chunking.rs`)

两档策略（hysteresis 防抖）：

| 模式 | 触发条件 | 行为 |
|------|---------|------|
| Smooth | 默认 | 每 commit tick 排出 1 行 |
| CatchUp | 队列深度 ≥ 8 或 最老行年龄 ≥ 120ms | 一次性排出所有队列 |

退出 CatchUp 需要：深度 ≤ 2 且年龄 ≤ 40ms，保持 200ms。

### 8.5 Commit Tick 编排 (`commit_tick.rs`)

```
run_commit_tick()
  → stream_queue_snapshot()      # 采集队列压力
  → resolve_chunking_plan()      # 决策：Smooth/CatchUp
  → apply_commit_tick_plan()     # 应用排水计划
  → CommitTickOutput { cells }   # 返回生成的 cells
```

## 9. 样式系统

### 9.1 样式规范 (`styles.md`)

| 用途 | 样式 |
|------|------|
| 标题 | `bold` |
| 主要文本 | 默认前景色 |
| 次要文本 | `dim` |
| 用户输入提示/选择/状态 | `cyan` |
| 成功/添加 | `green` |
| 错误/失败/删除 | `red` |
| Codex 品牌 | `magenta` |

**避免使用**：自定义 RGB 颜色（shimmer 除外）、`black`/`white` 前景色、`blue`/`yellow`。

### 9.2 样式辅助 (`style.rs`)

```rust
pub fn user_message_style() -> Style        // 用户消息背景（自适应明暗）
pub fn proposed_plan_style() -> Style       // 计划块背景
pub(crate) fn table_separator_style() -> Style  // 表格分隔线（低对比度）
pub(crate) fn accent_style() -> Style       // 强调色（dark: cyan bold, light: dark cyan bold）
```

背景色通过 `color::blend()` 混合计算：
- Dark 终端：白色 12% alpha 混合
- Light 终端：黑色 4% alpha 混合

### 9.3 Shimmer 动画 (`shimmer.rs`)

基于时间的 sweep 动画，用于加载状态：

```rust
pub(crate) fn shimmer_spans(text: &str) -> Vec<Span<'static>>
```

- 使用 `process_start` 作为时间基准
- 2 秒一个 sweep 周期
- 高亮带半宽 5 字符，cosine 插值
- TrueColor 模式：混合前景/背景色
- 非 TrueColor：DIM / 默认 / BOLD 三级

### 9.4 Motion 系统 (`motion.rs`)

集中管理动画原语，支持 reduced-motion 降级：

```rust
pub(crate) enum MotionMode { Animated, Reduced }

pub(crate) fn activity_indicator(...) -> Option<Span<'static>>  // 活动指示器 (•/◦ blink)
pub(crate) fn shimmer_text(...) -> Vec<Span<'static>>          // shimmer 文本
```

## 10. 底部面板 (`bottom_pane/`)

### 10.1 架构

`BottomPane` 是底部交互区域的编排器：

```
BottomPane
├── ChatComposer          # 可编辑输入框（基于 tui-textarea）
│   ├── textarea          # 文本编辑
│   ├── command_popup     # 命令弹窗 (/slash commands)
│   ├── file_search_popup # 文件搜索 (@file mentions)
│   └── skill_popup       # 技能弹窗 ($skill mentions)
│
├── BottomPaneView[]      # 视图栈（模态弹窗）
│   ├── ApprovalOverlay   # 审批弹窗（exec/network/patch/permissions）
│   ├── ListSelectionView # 列表选择器
│   ├── FeedbackView      # 反馈表单
│   ├── RequestUserInput  # MCP 用户输入请求
│   └── ...
│
├── StatusIndicatorWidget # 状态指示器（spinner + interrupt hint）
└── Footer                # 底部状态栏
```

### 10.2 StatusIndicatorWidget (`status_indicator_widget.rs`)

在 agent 工作时显示的单行状态栏：

```
• Working · 15s                    Esc to interrupt
```

- 使用 `shimmer_text()` 渲染 "Working" 文本
- 支持 inline message（如后台进程摘要）
- 可选的 interrupt hint（键位绑定提示）

### 10.3 Footer (`bottom_pane/footer.rs`)

渲染底部状态栏，包含：
- Collaboration mode 指示器
- Goal status 指示器
- 服务层级指示器

## 11. 终端超链接 (`terminal_hyperlinks.rs`)

### 11.1 HyperlinkLine

```rust
pub(crate) struct HyperlinkLine {
    pub(crate) line: Line<'static>,
    pub(crate) hyperlinks: Vec<TerminalHyperlink>,
}
```

超链接与 ratatui `Line` 分离存储。布局代码只操作 `line`，OSC 8 超链接字节仅在写入终端 buffer 时注入，不影响宽度计算。

### 11.2 超链接标注

- `annotate_web_urls()` 自动检测文本中的 URL 并添加 hyperlink annotation
- `mark_buffer_hyperlinks()` 在 ratatui Buffer 中注入 OSC 8 序列
- 支持 Web 链接和 TrustedFile 链接两种类型

## 12. 键位映射 (`keymap.rs`)

### 12.1 解析优先级

```
Context-specific binding → Global fallback → Built-in defaults
```

### 12.2 键位域

```rust
pub(crate) struct RuntimeKeymap {
    pub(crate) app: AppKeymap,           // 全局：Ctrl+T transcript, Ctrl+C copy, ...
    pub(crate) chat: ChatKeymap,         // 聊天级：backtrack (Esc), interrupt
    pub(crate) composer: ComposerKeymap, // 输入框：submit, newline, cancel
    pub(crate) editor: EditorKeymap,     # 编辑器模式
    pub(crate) vim_normal: VimNormalKeymap,
    pub(crate) vim_operator: VimOperatorKeymap,
    pub(crate) vim_text_object: VimTextObjectKeymap,
    pub(crate) pager: PagerKeymap,       # 分页覆盖层
    pub(crate) list: ListKeymap,         # 列表选择
    pub(crate) approval: ApprovalKeymap, # 审批弹窗
}
```

## 13. 关键设计模式

### 13.1 源码级 Markdown Cell

最终渲染的助手消息以**原始 markdown 源码**存储（`AgentMarkdownCell`），在每次 resize 时从源码重新渲染。这避免了 wrap 状态与终端宽度的耦合。

### 13.2 双模式渲染 (Rich / Raw)

```rust
pub(crate) enum HistoryRenderMode {
    Rich,  // 带样式、自动换行、语法高亮
    Raw,   // 纯文本，适合复制
}
```

`Ctrl+R` 切换到 Raw 模式，所有 cell 通过 `raw_lines()` 输出纯文本。

### 13.3 流式 Two-Region 模型

流式内容分为 stable（已提交到滚动缓冲区）和 tail（活跃 cell 中的可变预览），通过 `StreamCore` 管理分区边界。

### 13.4 自适应分块

流式 commit 不是逐字节的，而是基于队列压力的自适应分块——平稳时逐行输出（打字机效果），积压时批量追赶。

### 13.5 Terminal Hyperlink 分离

OSC 8 超链接数据与 ratatui `Line` 的样式/内容分离存储（`HyperlinkLine`），仅在 buffer 写入阶段注入终端，避免影响宽度计算和文本换行。

## 14. 关键文件索引

| 文件 | 职责 |
|------|------|
| `tui.rs` | Terminal 封装、帧调度、inline viewport 管理 |
| `custom_terminal.rs` | ratatui Terminal 的定制版本（cursor position、viewport、OSC 8 support） |
| `app.rs` | 顶层 App 状态、事件循环、绘制编排 |
| `chatwidget.rs` | 主聊天面板编排（2000+ 行） |
| `history_cell/mod.rs` | HistoryCell trait 定义 + cell 类型注册 |
| `history_cell/messages.rs` | User/Agent/Reasoning/Streaming 消息 cell |
| `history_cell/exec.rs` | 命令执行 cell |
| `history_cell/plans.rs` | 计划更新/提案 cell |
| `history_cell/approvals.rs` | 审批决策 cell |
| `history_cell/patches.rs` | 补丁应用 cell |
| `history_cell/notices.rs` | 警告/错误/更新通知 cell |
| `history_cell/search.rs` | 网页搜索活动 cell |
| `history_cell/separators.rs` | Turn 分隔符 cell |
| `history_cell/session.rs` | 会话头部卡片 cell |
| `render/renderable.rs` | Renderable trait + 布局组件 |
| `render/line_utils.rs` | 行级辅助函数（prefix、clone to static） |
| `render/highlight.rs` | syntect 语法高亮集成 |
| `markdown_render.rs` | pulldown-cmark → ratatui 渲染器（2800+ 行） |
| `diff_render.rs` | unified diff 渲染（2500+ 行） |
| `streaming/controller.rs` | StreamController / PlanStreamController |
| `streaming/chunking.rs` | 自适应分块策略 |
| `streaming/commit_tick.rs` | commit tick 编排 |
| `streaming/render.rs` | 增量 markdown 渲染 |
| `style.rs` | 主题自适应样式函数 |
| `color.rs` | 颜色混合、亮度检测、感知距离 |
| `shimmer.rs` | Shimmer sweep 动画 |
| `motion.rs` | 动画原语（reduced-motion 降级） |
| `terminal_hyperlinks.rs` | OSC 8 超链接管理 |
| `bottom_pane/mod.rs` | 底部面板编排 |
| `bottom_pane/chat_composer.rs` | 输入框组件 |
| `status_indicator_widget.rs` | 状态行组件 |
| `keymap.rs` | 键位映射解析 |
| `wrapping.rs` | 文本换行辅助 |
| `live_wrap.rs` | 增量文本换行（RowBuilder） |
