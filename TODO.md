# TODO.md — Crown-Code 开发路线图

> 基于 AGENTS.md 目标 + Codex/Cline/MiMo-Code 三个参考实现分析

## 当前状态总览

| 子系统 | 状态 | 说明 |
|--------|------|------|
| Core daemon + IPC | ✅ 完成 | JSON-RPC 2.0 over Unix socket，7 方法 + 7 事件通知 |
| Agent Loop | ✅ 完成 | 7 工具，流式 API 调用 → 工具执行 → 结果反馈 |
| OpenAI API | ✅ 完成 | 流式 + 非流式，tool call delta 累积，SSE 解析 |
| MCP 子系统 | ✅ 完成 | stdio + HTTP/SSE 传输，多 server 注册表 |
| 文件操作 | ✅ 完成 | read/write/edit/search/list/glob + crownignore |
| TUI 基础框架 | ✅ 完成 | 3 区布局 + 5 种 cell + IPC 断连重连 |
| 测试覆盖 | ✅ 541+ | 全模块覆盖 |

---

## Phase 1: TUI 渲染质量提升

> 目标：达到 Codex TUI 的渲染质量，支持 Markdown、Diff、语法高亮

### 1.1 Markdown 渲染引擎
- [ ] 引入 `pulldown-cmark` 依赖，实现 `markdown_render.rs`
- [ ] 标题/粗体/斜体/代码块/列表/链接/引用 样式映射
- [ ] 表格渲染（列宽计算 + 分隔线）
- [ ] 流式增量渲染（stable prefix + mutable tail，参考 Codex `StreamingRender`）
- [ ] 代码块语法高亮（引入 `syntect` + `two-face`）

### 1.2 Diff 渲染引擎
- [ ] 扩展 `xdiff.rs`，增加带行号 + gutter sign 的渲染输出
- [ ] Diff 行背景色（add: green tint, del: red tint），终端背景自适应
- [ ] Syntax highlighting within diff hunks
- [ ] 长行硬换行 + span 分割

### 1.3 HistoryCell 类型扩展
- [ ] `AgentMarkdownCell`：流完成后合并的 Markdown cell（参考 Codex）
- [ ] `DiffCell`：文件变更 diff 渲染
- [ ] `SystemNoticeCell`：警告/错误/信息通知（⚠/■/• 前缀）
- [ ] `SeparatorCell`：Turn 间分隔符（`── Worked for Xm Xs ──`）
- [ ] `UserMessageCell` 样式增强：背景色自适应 + `› ` 前缀

### 1.4 Streaming Two-Region 模型
- [ ] 实现 `StreamController`（参考 Codex `streaming/controller.rs`）
- [ ] Stable region → 已提交行写入滚动缓冲区
- [ ] Tail region → 活跃 cell 在 `ChatWidget.active_cell` 插槽
- [ ] 自适应分块策略（Smooth: 逐行 / CatchUp: 批量追赶）
- [ ] 表格回退（`TableHoldbackScanner`：pipe-table 检测 → tail 保留至流结束）

### 1.5 样式系统
- [ ] `style.rs`：主题自适应样式函数（Dark/Light 终端自动检测）
- [ ] Shimmer 动画（加载状态 sweep 效果，参考 Codex `shimmer.rs`）
- [ ] 活动指示器（blink `•/◦`）
- [ ] 样式规范：cyan=交互, green=成功, red=错误, magenta=品牌, dim=次要

### 1.6 终端超链接
- [ ] OSC 8 超链接支持（`HyperlinkLine` 分离存储，参考 Codex `terminal_hyperlinks.rs`）
- [ ] URL 自动检测 + 可点击

---

## Phase 2: Agent Loop 增强

> 目标：达到 MiMo-Code 的恢复能力 + Cline 的工具体系

### 2.1 步骤分类器
- [ ] `classify.rs`：对 assistant 输出分类（continue/final/failed/filtered/text-tool-call/think-only/invalid）
- [ ] 自动续接：finish="length" → 注入 "continue" 合成消息（上限 3 次）
- [ ] 空输出恢复：invalid → 注入 "空输出，重试"（上限 3 次）
- [ ] 文本重复检测：n-gram 滑动窗口 + 恢复提示

### 2.2 Goal Gate（目标门控）
- [ ] 当 assistant 输出为 "final" 时，用独立 judge 模型评估目标是否达成
- [ ] 未达成 → 注入合成 user 消息 + 未完成任务列表，`continue`
- [ ] 子 agent 重入限制（`MAX_TASK_GATE` 可配置）

### 2.3 错误恢复增强
- [ ] 详细错误信息发送到 TUI（错误码 + 原始响应 + 建议操作）
- [ ] 可重试错误自动重试（429/5xx/网络错误，指数退避）
- [ ] Prefill 拒绝检测 + 自动修剪重试
- [ ] Doom Loop 检测（连续 3 次完全相同的 tool call → 权限询问）

### 2.4 工具体系扩展
- [ ] `glob` 工具（文件名模式匹配）
- [ ] `grep` 工具（内容正则搜索，已有 `search_files` 底层）
- [ ] `apply_patch` 工具（unified diff patch 应用，复用 `xdiff.rs`）
- [ ] `web_fetch` 工具（HTTP 内容获取）
- [ ] `question` 工具（向用户提问，需 IPC 扩展 ask/answer 协议）


---

## Phase 3: 上下文管理

> 目标：达到 MiMo-Code 的上下文管理能力

### 3.1 Token 压力检测
- [ ] `overflow.ts` 等价实现：`pressureLevel(0-3)` 基于 token 使用率
- [ ] 输入 token 统计（从 API response usage 中提取）
- [ ] 压力阈值配置（0.50/0.70/0.85）

### 3.2 上下文压缩（Compaction）
- [ ] `compaction.rs`：触发条件 = token 总量 ≥ usable 预算
- [ ] Tail 选择算法：保留最近 N 轮（默认 2 轮），受 token 预算约束
- [ ] 摘要生成：调用 LLM 对 head 部分生成结构化摘要（Goal/Instructions/Discoveries/Files）
- [ ] Prune：压缩前释放大工具输出（> 40K token 的部分标记为 compacted）

### 3.3 Checkpoint 系统（Shadow Git）
- [ ] 独立 Git 仓库快照（`~/.local/share/crown/snapshot/<project>/<hash>`）
- [ ] `track()`：git add + write-tree → tree hash
- [ ] `patch(hash)`：diff --cached → 变更文件列表
- [ ] `restore(hash)`：read-tree + checkout-index → 恢复文件
- [ ] 每次文件写入后自动 checkpoint
- [ ] `checkpoint-writer` 子 agent 生成 11 章节结构化快照
- [ ] Checkpoint Rebuild：溢出时注入 checkpoint.md + MEMORY.md 重建上下文

### 3.4 会话记忆
- [ ] `MEMORY.md`：项目级跨 session 持久记忆
- [ ] `checkpoint.md`：Session 级结构化快照
- [ ] `notes.md`：Session 级草稿本
- [ ] 记忆指令注入 system prompt

---

## Phase 4: 成本与可观测性

### 4.1 成本统计
- [ ] 从 API usage 计算 input/output/reasoning/cache tokens
- [ ] 模型价格配置（per-provider cost table）
- [ ] 单次请求成本 = (input × price_in) + (output × price_out) + (reasoning × price_reasoning)
- [ ] Session 累计成本追踪（含子 agent 调用）
- [ ] TUI 状态栏显示累计成本

### 4.2 TUI 状态栏增强
- [ ] Token 使用量（In:X Out:Y CacheR:Z）
- [ ] 平均延迟（滑动窗口 5 次）
- [ ] 当前模型名称
- [ ] Session 时长
- [ ] 成本显示（$X.XX）

---

## Phase 5: 交互增强

### 5.1 重新生成（Regenerate）
- [ ] IPC 协议扩展：`regenerate` 方法
- [ ] Core：回退到最后一个 assistant 消息，重新调用 LLM
- [ ] TUI：`Ctrl+R` 或 `Regenerate` 按钮触发
- [ ] 流式中断后可重新生成

### 5.2 多 Session TUI
- [ ] Session 切换快捷键（`Ctrl+N` 新建 / `Ctrl+Tab` 切换）
- [ ] Session 列表面板（`Ctrl+S`）
- [ ] 每个 session 独立的 ChatWidget 状态
- [ ] Core 侧已支持（SessionManager），需 TUI 侧管理多个 IpcClient

### 5.3 审批系统
- [ ] 工具执行前权限检查（危险命令确认）
- [ ] TUI 审批弹窗（`ApprovalOverlay`）
- [ ] 权限模式：YOLO / Ask / Deny
- [ ] IPC 扩展：`approve` / `deny` 方法

### 5.4 键位系统增强
- [ ] Ctrl+C 复制（当前为 cancel）
- [ ] Ctrl+T 转录覆盖层（全量对话历史）
- [ ] Ctrl+R Raw 模式切换（纯文本 vs 样式）
- [ ] Vim 模式支持（Normal/Insert/Operator）
- [ ] 可配置键位映射（`keymap.json`）

---

## Phase 6: 高级特性

### 6.1 SWE-Pruner（上下文修剪）
- [ ] 自适应上下文修剪框架
- [ ] 基于相关性的工具输出裁剪
- [ ] 代码变更相关性评分

### 6.2 Workspace 向量索引
- [ ] 引入 embedding 模型（本地或 API）
- [ ] 文件级向量索引构建
- [ ] 语义搜索工具（替代纯正则搜索）
- [ ] 增量更新（文件变更时重新索引）

### 6.3 子 Agent 系统
- [ ] `explore` agent（只读代码搜索专家）
- [ ] `orchestrator` agent（多 session 协调）
- [ ] 子 agent 任务委托（`task` 工具）
- [ ] 子 agent 成本归入父 session

---

## 优先级排序（下一步建议）

**立即**（Phase 1.1-1.3）：
Markdown 渲染 + HistoryCell 扩展 → 这是 TUI 体验的核心差距。当前 5 种 cell 类型无法渲染 LLM 输出的 markdown 内容（代码块、表格、列表等），导致信息丢失。

**短期**（Phase 2.1 + 3.1）：
步骤分类器 + Token 压力检测 → Agent Loop 缺乏恢复能力和上下文管理，长对话会爆 context window。

**中期**（Phase 3.3 + 4.1 + 5.1）：
Checkpoint 系统 + 成本统计 + 重新生成 → 这三个是 AGENTS.md 中的核心差异化特性。
