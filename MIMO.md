# MiMo-Code 底层逻辑深度解析

> 本文档覆盖 MiMo-Code 核心运行时的全部底层逻辑，包括提示词系统、消息组装、上下文管理、Agent Loop、Session 管理、历史会话等。不含 TUI 显示层内容。

---

## 1. 项目总体架构

```
┌──────────────────────────────────────────────────────────────────┐
│                     MiMo-Code Runtime                           │
│                                                                  │
│  ┌──────────┐   HTTP/WS    ┌──────────────────────────────────┐ │
│  │ CLI/TUI  │ ◄──────────► │ Server (Hono)                    │ │
│  └──────────┘              │  ├─ Routes (session/config/...)   │ │
│                            │  ├─ Middleware (auth/cors/...)    │ │
│                            │  └─ SSE Event Stream             │ │
│                            └──────────┬───────────────────────┘ │
│                                       │                         │
│  ┌────────────────────────────────────▼───────────────────────┐ │
│  │                  Session Layer (核心)                       │ │
│  │  ┌─────────────┐ ┌──────────────┐ ┌─────────────────────┐ │ │
│  │  │ prompt.ts   │ │ processor.ts │ │ llm.ts              │ │ │
│  │  │ (Agent Loop)│ │ (Stream处理) │ │ (LLM调用封装)       │ │ │
│  │  └──────┬──────┘ └──────┬───────┘ └──────────┬──────────┘ │ │
│  │         │               │                    │            │ │
│  │  ┌──────▼───────────────▼────────────────────▼──────────┐ │ │
│  │  │              message-v2.ts (消息模型)                 │ │ │
│  │  │              session.ts   (持久化)                   │ │ │
│  │  │              classify.ts  (步骤分类)                 │ │ │
│  │  └──────────────────────────────────────────────────────┘ │ │
│  │  ┌─────────────┐ ┌──────────────┐ ┌─────────────────────┐ │ │
│  │  │ compaction  │ │ overflow.ts  │ │ checkpoint.ts       │ │ │
│  │  │ (上下文压缩)│ │ (溢出检测)   │ │ (持久化快照)        │ │ │
│  │  └─────────────┘ └──────────────┘ └─────────────────────┘ │ │
│  └───────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌────────────┐ ┌─────────────┐ ┌────────────┐ ┌─────────────┐ │
│  │ Agent      │ │ Provider    │ │ Tool       │ │ Permission  │ │
│  │ (角色定义) │ │ (LLM抽象层)│ │ (工具注册) │ │ (权限控制)  │ │
│  └────────────┘ └─────────────┘ └────────────┘ └─────────────┘ │
│                                                                  │
│  ┌────────────┐ ┌─────────────┐ ┌────────────┐ ┌─────────────┐ │
│  │ Bus        │ │ Snapshot    │ │ Memory     │ │ History     │ │
│  │ (事件总线) │ │ (Git快照)  │ │ (持久记忆) │ │ (FTS搜索)   │ │
│  └────────────┘ └─────────────┘ └────────────┘ └─────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

技术栈：TypeScript + Bun + Effect-TS + Drizzle ORM (SQLite) + Vercel AI SDK

---

## 2. 提示词系统 (Prompt System)

### 2.1 系统提示词组装流程

系统提示词由 `llm.ts:buildSystemArray()` 组装，按以下顺序拼接：

```
┌─────────────────────────────────────────────────────┐
│  1. 基础提示词 (provider-specific 或 agent.prompt)   │
│  2. 自定义 system 提示词 (调用时传入)                │
│  3. 用户消息中的 system 字段                         │
│  4. 环境信息块 (environment)                         │
│  5. 模态能力说明 (非视觉模型的 vision-capability)     │
│  6. 记忆系统指令 (memory instructions)               │
│  7. 插件钩子注入 (plugin: experimental.chat.system)  │
│  8. 技能列表 (skills)                                │
│  9. 指令文件 (AGENTS.md / CLAUDE.md)                 │
└─────────────────────────────────────────────────────┘
         ↓ join("\n\n") 合并为单条 system 消息
```

**关键代码路径：**
- `session/llm.ts:240-307` — `buildSystemArray()`
- `session/system.ts:23-40` — `provider()` 基础提示词选择
- `session/instruction.ts` — 指令文件发现与加载

### 2.2 基础提示词选择 (Provider-Specific)

`system.ts:provider()` 根据模型 ID 选择不同的基础提示词：

| 模型特征 | 提示词文件 | 说明 |
|----------|-----------|------|
| `gpt-4*`, `o1*`, `o3*` | `beast.txt` | Beast 模式，自主性强 |
| `gpt*codex*` | `codex.txt` | Codex 专用 |
| `gpt*` | `gpt.txt` | GPT 系列通用 |
| `gemini-*` | `gemini.txt` | Gemini 专用 |
| `claude*` | `anthropic.txt` | Anthropic 专用 |
| `trinity*` | `trinity.txt` | Trinity 专用 |
| `kimi*` | `kimi.txt` | Kimi 专用 |
| `deepseek*` | `deepseek.txt` | DeepSeek 专用 |
| `glm*` | `glm.txt` | GLM 专用 |
| `minimax*` | `minimax.txt` | MiniMax 专用 |
| 其他 | `default.txt` | 默认提示词 |

当 `agent.prompt` 存在时（如 explore、orchestrator 等子 agent），使用 agent 自己的提示词替代 provider 提示词。

### 2.3 默认系统提示词结构 (`default.txt`)

```
## System          — 工具执行、权限模式、系统标签说明
## Doing tasks     — 任务执行原则、安全编码、代码风格
## Executing actions — 可逆性评估、危险操作确认
## Agent system    — Agent 架构说明（mode/permission/tools/tasks/skills/session lifecycle/memory/plan mode/MCP/trust boundaries）
## Tone and style  — 输出风格（简洁、无emoji、file:line 引用）
## Text output     — 文本输出规则（可见性假设、更新频率）
## Session-specific — 子agent使用、技能搜索
```

### 2.4 环境信息注入

`system.ts:environment()` 注入固定格式的环境块：

```xml
<env>
  Working directory: /path/to/project
  Workspace root folder: /path/to/worktree
  Is directory a git repo: yes/no
  Platform: linux/darwin/win32
  Today's date: Mon Jul 21 2026
</env>
```

环境信息锚定到 session 创建时间（非请求时间），确保同一 session 内每轮字节一致，利于 Anthropic 前缀缓存。

### 2.5 记忆系统指令

`llm.ts:buildMemoryInstructions()` 仅为主 agent 和 peer agent 注入（不注入给子 agent）：

- **Project memory** — `MEMORY.md`，跨 session 持久
- **Session checkpoint** — `checkpoint.md`，11 个结构化章节
- **Per-task progress** — `tasks/<id>/progress.md`，writer 派生
- **Global memory** — `global/MEMORY.md`，用户级偏好
- **Notes scratchpad** — `notes.md`，session 级草稿本
- **Active recall protocol** — 避免重复读取已在上下文中的文件
- **Subagent return format** — 子 agent 返回格式约定

### 2.6 指令文件加载 (`instruction.ts`)

发现顺序：
1. **项目级** — 向上搜索 `AGENTS.md`；若内容 < 500 字符，也加载 `CLAUDE.md`
2. **全局级** — `~/.config/mimocode/AGENTS.md` 或 `~/.claude/CLAUDE.md`
3. **配置级** — `mimocode.json` 中的 `instructions` 字段（支持文件路径和 URL）

每次 Read 工具执行时，`resolve()` 会从被读文件向上搜索附近的指令文件，每个 message 只注入一次（通过 `claims` Map 追踪）。

### 2.7 Agent 特有提示词

| Agent | 提示词 | 用途 |
|-------|--------|------|
| `explore` | `prompt/explore.txt` | 代码搜索专家 |
| `orchestrator` | `prompt/orchestrator.txt` | 多 session 协调 |
| `compaction` | `prompt/compaction.txt` | 上下文压缩 |
| `checkpoint-writer` | `prompt/checkpoint-writer.txt` | 检查点写入（11 章节） |
| `dream` | `prompt/dream.txt` | 记忆整合（5 阶段） |
| `distill` | `prompt/distill.txt` | 工作流提取（6 阶段） |
| `title` | `prompt/title.txt` | 标题生成（≤50 字符） |
| `summary` | `prompt/summary.txt` | Session 总结 |
| `compose` | `prompt/compose.txt` | 技能编排 |
| `build` / `plan` / `general` | 无自定义提示词 | 使用 provider 提示词 |

---

## 3. 消息组装与发送

### 3.1 消息数据模型 (`message-v2.ts`)

**消息类型**（按 `role` 区分）：

```typescript
// User 消息
{
  role: "user",
  id: MessageID,         // ascending("message") 递增ID
  sessionID: SessionID,  // descending("session") 递减ID
  agentID?: string,
  time: { created },
  format?: OutputFormat, // text | json_schema
  agent: string,
  model: { providerID, modelID, variant? },
  system?: string,       // 用户级自定义 system 提示词
  tools?: Record<string, boolean>,  // 用户级工具开关
}

// Assistant 消息
{
  role: "assistant",
  id: MessageID,
  sessionID: SessionID,
  agentID?: string,
  time: { created, completed? },
  error?: ErrorObject,
  parentID: MessageID,   // 指向触发它的 user 消息
  modelID, providerID,
  mode: string,          // "build" | "compaction" | "checkpoint" 等
  agent: string,
  path: { cwd, root },
  cost: number,
  tokens: { input, output, reasoning, cache: { read, write } },
  finish?: string,       // "stop" | "tool-calls" | "length" | "content-filter" | "error"
  summary?: boolean,     // 是否为压缩摘要
}
```

**Part 类型**（13 种，附着在消息上）：

| Part Type | 关键字段 | 说明 |
|-----------|---------|------|
| `text` | `text, synthetic?, ignored?` | 文本输出 |
| `reasoning` | `text, time: {start, end}` | 推理/思考链 |
| `file` | `mime, filename?, url, source?` | 文件附件 |
| `tool` | `callID, tool, state(pending/running/completed/error)` | 工具调用 |
| `step-start` | `snapshot?` | 步骤开始标记 |
| `step-finish` | `reason, snapshot?, cost, tokens` | 步骤结束标记 |
| `snapshot` | `snapshot(hash)` | Git 快照标记 |
| `patch` | `hash, files: string[]` | 文件变更补丁 |
| `compaction` | `auto, overflow?, tail_start_id?` | 压缩边界 |
| `checkpoint` | `checkpointDir, checkpointNumber, coveredUpTo` | 检查点标记 |
| `retry` | `attempt, error` | 重试记录 |
| `agent` | `name, source?` | Agent 标记 |
| `subtask` | `prompt, description, agent, model?` | 子任务委托 |

**工具状态机**：
```
pending → running → completed
                  → error
```

### 3.2 消息到模型格式转换 (`toModelMessagesEffect`)

消息从 DB 格式转换为 Vercel AI SDK 的 `ModelMessage[]`：

```
User 消息 → { role: "user", content: [text parts + file parts] }
Assistant 消息 → { role: "assistant", content: [text + reasoning + tool-call parts] }
Tool 结果 → { role: "tool", content: [{ type: "tool-result", toolCallId, output }] }
Compaction 边界 → 跳过（不发送给模型）
```

**压缩后过滤** — `filterCompactedEffect()`：
- 从最新消息向前遍历，遇到 compaction 边界则截断
- 子 session 通过 `contextFrom` / `contextWatermark` 拼接父 session 消息
- 每个 agent slice 独立过滤（按 `agentID` 切片）

### 3.3 LLM 请求组装 (`llm.ts:run()`)

```
┌─ system messages ─────────────────────────────┐
│  [0] 基础提示词 + 自定义 + 用户 system         │
│  [1] 环境信息 + 视觉能力                       │
│  [2] 记忆系统指令 (仅主 agent)                 │
│  [3] 插件注入                                  │
│  → join("\n\n") → 单条 system 消息             │
└───────────────────────────────────────────────┘
         +
┌─ conversation messages ───────────────────────┐
│  filterCompactedEffect() 过滤后的消息          │
│  → toModelMessagesEffect() 转换               │
│  → ProviderTransform.message() 各 provider 适配│
└───────────────────────────────────────────────┘
         +
┌─ tools ──────────────────────────────────────┐
│  ToolRegistry.tools() 按 agent/model 过滤     │
│  → ProviderTransform.tools() 转换为 SDK 格式  │
│  → resolveTools() 附加 execute 闭包           │
└──────────────────────────────────────────────┘
         ↓
    streamText() 调用
```

**特殊处理**：
- **OpenAI OAuth** — system 通过 `instructions` 参数传递，不作为消息
- **LiteLLM 代理** — 当有 tool-call 历史但无活跃工具时，注入 `_noop` 占位工具
- **GitLab Workflow** — WebSocket 双向通信，tool 执行通过 `toolExecutor` 回调
- **Prefill 拒绝防护** — 两层防御：主动 `ensureTrailingUserMessage` + 被动 `dropAssistantPrefill` 重试

### 3.4 Provider Transform 管线 (`provider/transform.ts`)

消息在发送前经过一系列变换：

1. `normalizeContentArray` — 确保 content 为 string 或 array
2. `unsupportedParts` — 移除/替换不支持的模态
3. `limitImages` — 图片数量/大小限制
4. `normalizeMessages` — Provider 特定修复（Anthropic 空内容、Claude tool ID、Mistral 消息序列）
5. `ensureTrailingUserMessage` — 防止 Bedrock prefill 拒绝
6. `applyCaching` — 添加缓存控制标记（Anthropic/Bedrock/OpenRouter）
7. `mapProviderOptions` — 重映射 providerOptions 到 SDK 命名空间

---

## 4. Agent Loop 核心循环

### 4.1 入口流程

```
用户输入
  → prompt()                    # prompt.ts:2104
    → createUserMessage()       # 创建 User 消息 + Parts 写入 DB
    → loop()                    # prompt.ts:2133
      → runLoop()               # prompt.ts:2157 — 核心 while(true) 循环
```

### 4.2 `runLoop()` 主循环 (`prompt.ts:2157-3923`)

```
while (true) {
  ① 获取消息: filterCompactedEffect(sessionID, { agentID })
  ② 查找最后的 user/assistant/finished 消息
  
  ③ 对已有 assistant 消息做分类 (classifyAssistantStep)
     ├─ "continue"  → 有待执行的 tool call，进入生成步骤
     ├─ "final"     → goalGate() 检查 → break
     ├─ "failed"    → error → break
     ├─ "filtered"  → 内容过滤 → break
     ├─ "text-tool-call" → 重试 → continue/break
     ├─ "think-only" → 仅有推理无文本 → continue/break
     └─ "invalid"   → 空输出 → continue/break
  
  ④ 自动续接: autoContinueOutputLength / autoContinueInvalidOutput
  
  ⑤ Step 递增, 标题生成
  
  ⑥ 压缩/溢出处理:
     ├─ pressureLevel() 检测 token 压力 (0-3)
     ├─ pressure >= 2 → checkpoint 触发
     ├─ overflow → compaction.process() 或 checkpoint rebuild
     └─ skipOverflowCheck 标志控制
  
  ⑦ 重复步骤提示 / 记忆刷新提示
  
  ⑧ 解析工具: resolveTools() + buildLLMRequestPrefix()
  
  ⑨ 创建 assistant 消息 + Processor Handle
  
  ⑩ 插件钩子: session.userQuery.pre
  
  ⑪ handle.process(processArgs) ← 调用 LLM
     └─ processor.ts: stream → handleEvent → tool 执行
        返回: "overflow" | "stop" | "continue" | "text-repeat"
  
  ⑫ 后处理:
     ├─ autoContinue (输出长度/无效输出)
     ├─ text-repeat 检测
     ├─ structured output 验证
     └─ handleEmptyStep() (空工具调用保护)
  
  ⑬ classifyAssistantStep() 对新 assistant 分类
  
  ⑭ 文本循环检测 (n-gram buffer)
  
  ⑮ goalGate() — 目标门控，未达目标则注入合成 user 消息继续
  
  ⑯ break 或 continue
}
```

### 4.3 步骤分类器 (`classify.ts`)

纯函数，按优先级判定：

```
1. 有待处理的 client tool part → "continue"    // 最高优先级
2. 无 finish → "continue"
3. finish="tool-calls" + 无结构化 tool part + 文本含 tool-call 标记 → "text-tool-call"
4. finish="tool-calls" → "continue"            // provider-executed tools
5. 过时的 assistant (早于 lastUser) → "continue"
6. 有 error → "failed"
7. 有 structured/summary → "final"
8. finish="content-filter" → "filtered"
9. finish="stop"/"length" + 有文本 → "final"
10. 仅有 reasoning (非 GPT) → "think-only"
11. 无内容 → "invalid"
```

### 4.4 流处理器 (`processor.ts`)

`SessionProcessor.create()` 创建处理器，核心方法 `process()`：

```
process(streamInput):
  1. 创建 LLM Stream: llm.stream(streamInput)
  2. Stream.tap(handleEvent) 逐事件处理:
     ├─ "start"          → 设置 session 状态为 busy
     ├─ "start-step"     → 捕获 Git snapshot
     ├─ "reasoning-*"    → 创建/更新 reasoning part
     ├─ "text-*"         → 创建/更新 text part (含 n-gram 检测)
     ├─ "tool-input-*"   → 创建 pending tool part
     ├─ "tool-call"      → 更新为 running + doom loop 检测
     ├─ "tool-result"    → 完成 tool part
     ├─ "tool-error"     → 失败 tool part
     ├─ "finish-step"    → 记录 tokens/cost + overflow 检测 + snapshot patch
     └─ "error"          → 抛出异常
  3. Stream.takeUntil(needsOverflow || textNgramRepeat || blocked)
  4. 错误重试: SessionRetry.policy (指数退避)
  5. 清理: 完成未完成的 tool calls, 更新时间戳
  6. 返回: "overflow" | "stop" | "continue" | "text-repeat"
```

**Doom Loop 检测** — 连续 3 次完全相同的 tool call (同名+同参数) 触发 `doom_loop` 权限询问。

**Try-Best 检测** — 监控重复的 edit/bash 模式，检测"无进展循环"。

**Max Mode Replay** — `replay()` 方法合成候选者的事件流，用于 max mode 的胜选回放。

### 4.5 恢复策略阶梯

Agent Loop 包含多层恢复策略，按严重程度递进：

| 检测条件 | 恢复策略 | 上限 |
|----------|---------|------|
| `finish="length"` + 有文本 | 注入 "continue" 合成消息 | 3 次 |
| `finish="length"` + 无文本 | 注入 "你被截断了，继续" | 3 次 |
| `invalid` (空输出) | 注入 "空输出，重试" | 3 次 |
| `text-tool-call` | 重试，提示使用结构化 tool call | 2 次 |
| `think-only` (仅推理) | 注入 "需要文本输出" | - |
| `structured output` 校验失败 | 注入 schema 错误，重试 | `retryCount` (默认 2) |
| 文本重复 (3 次相同输出) | `RECOVERY_PROMPT_MILD` → `STRONG` | 2 次 |
| n-gram 重复 (滑动窗口) | `text-ngram-detection.ts` | - |
| 空工具调用 (无参数) | `EMPTY_STEP_RECOVERY_REMIND` → `REPLAN` | 可配置 |
| Goal Gate (目标未达成) | 注入合成 user 消息继续 ReAct | `MAX_TASK_GATE` |

### 4.6 Goal Gate (目标门控)

当 assistant 输出为 "final" 时，`goalGate()` 用独立的 judge 模型评估目标是否达成：
- 如果未达成 → 注入合成 user 消息（包含未完成任务列表），`continue`
- 如果达成 → `break`
- 子 agent 有 `MAX_TASK_GATE_SUBAGENT_REACT` 限制（默认 2 次重入）

---

## 5. 上下文管理

### 5.1 Token 压力检测 (`overflow.ts`)

```
usable = model.limit.input - reserved
         或 (context - outputReserve - reserved)

其中:
  reserved = config.compaction.reserved ?? min(20000, maxOutputTokens)
  outputReserve = min(maxOutputTokens, 20000)

pressureLevel:
  ratio = totalTokens / usable
  < 0.50 → 0 (安全)
  < 0.70 → 1 (关注)
  < 0.85 → 2 (警告)
  ≥ 0.85 → 3 (危险)
```

### 5.2 压缩系统 (`compaction.ts`)

**触发条件**：`isOverflow()` — token 总量 ≥ usable 预算

**压缩流程**：

```
1. 创建 compaction 边界消息 (user 消息 + compaction part)
2. select(): 选择保留最近 N 轮 (默认 2 轮) 作为 tail
   - tail 受 preserve_recent_tokens 预算约束 (默认 2K-8K)
   - 每轮估算 token 数，从最新向前累加直到超预算
3. process(): 
   - 对 head 部分调用 compaction agent 生成摘要
   - 摘要遵循模板: Goal/Instructions/Discoveries/Accomplished/Files
   - 尾部消息保持原文
4. auto-continue: 如果是自动触发，注入 "Continue if you have next steps" 合成消息
```

**Tail 选择算法**：
```
turns = 所有 user 消息位置（排除 compaction 边界）
recent = turns[-limit:]  (limit 默认 2)
对每轮估算 token 数
从最新向前累加，直到 total > budget
keep = 最后一个不超预算的轮次
head = messages[0:keep.start]
tail_start_id = keep.id
```

### 5.3 Prune (修剪)

`compaction.prune()` 在压缩前执行，释放工具输出占用的空间：

```
1. 从最新消息向前遍历
2. 跳过最近 2 轮 (turns < 2)
3. 对 completed 的 tool part:
   - 累加 token 估算
   - 超过 PRUNE_PROTECT (40K) 的部分标记为 compacted
4. 如果总修剪量 > PRUNE_MINIMUM (20K)，写入 DB
```

保护的工具：`skill`（不被 prune）

### 5.4 Checkpoint Rebuild (检查点重建)

当上下文溢出且无法通过普通压缩解决时：

```
1. checkpoint-writer 子 agent 生成 checkpoint.md
2. insertRebuildBoundary(): 注入合成 user 消息
   包含: checkpoint.md + MEMORY.md + notes.md + global/MEMORY.md + tasks + actors + recent user input
3. 重建上下文有 token 预算: TAIL_MIN=10K, TAIL_MAX=20K
4. 超出部分截断，提示 "Read(<path>, offset=L) for the rest"
```

### 5.5 Snapshot 系统 (`snapshot/index.ts`)

基于独立 Git 仓库的文件快照：

```
仓库位置: ~/.local/share/opencode/snapshot/<projectID>/<hash(worktree)>

track():    git add + write-tree → 返回 tree hash
patch(hash): diff --cached → 变更文件列表
diff(hash):  diff --cached → 完整 diff 文本
restore():   read-tree + checkout-index → 恢复文件
revert():    恢复特定文件到快照状态
cleanup():   gc --prune=7.days (每小时)
```

- 信号量 `Semaphore(1)` 防止并发 Git 操作
- 文件 > 2MB 排除在快照之外
- 每个 step 开始时 `track()`，结束时 `patch()` 记录变更

---

## 6. Agent 系统

### 6.1 Agent 定义 (`agent/agent.ts`)

```typescript
Info = {
  name: string,
  mode: "primary" | "subagent" | "all",
  description?: string,
  prompt?: string,           // 自定义系统提示词
  permission: Ruleset,       // 权限规则集
  hardPermission?: Ruleset,  // 不可覆盖的硬性规则
  model?: { providerID, modelID },
  variant?: string,
  temperature?: number,
  topP?: number,
  options: Record<string, any>,
  steps?: number,            // 最大步骤数
  toolAllowlist?: string[],  // 工具白名单
  hidden?: boolean,
  native?: boolean,
}
```

### 6.2 内置 Agent 列表

| Agent | Mode | 用途 | 权限特点 |
|-------|------|------|---------|
| `build` | primary | 默认，全工具访问 | `question: allow`, `plan_enter/exit: allow` |
| `plan` | primary | 只读设计模式 | `hardPermission: edit → deny` (仅允许 `.mimocode/plans/*.md`) |
| `compose` | primary | 技能编排 | `skill:compose:* → allow` |
| `max` | primary | 实验性并行候选 | 同 build (需 `experimental.maxMode`) |
| `orchestrator` | primary | 多 session 协调 | 需 `MIMOCODE_EXPERIMENTAL_ORCHESTRATOR` |
| `general` | subagent | 通用多步执行 | `change_directory: deny` |
| `explore` | subagent | 只读代码搜索 | 仅 `grep/glob/list/bash/webfetch/websearch/codesearch/read` |
| `title` | subagent | 标题生成 | `*: deny`, 无工具 |
| `summary` | subagent | Session 摘要 | `*: deny`, 无工具 |
| `compaction` | subagent | 上下文压缩 | `*: deny`, 无工具 |
| `checkpoint-writer` | subagent | 检查点写入 | Fork agent，继承父 agent 前缀 |
| `dream` | subagent | 记忆整合 | `read/write/edit/glob/grep/memory/bash` |
| `distill` | subagent | 工作流提取 | 同 dream |

### 6.3 权限模型

```
runtimePermission(agent, session) =
  agent.permission          ← Agent 基础权限
  → merge(session.permission) ← 用户/session 配置
  → merge(agent.hardPermission) ← 不可覆盖的硬性规则（最后生效）
```

**三层合并，hardPermission 最后生效**：这就是 plan 模式即使用户配置 `"*": "allow"` 也能保证写入被阻止的原因。

**权限决策**：`allow` / `ask` / `deny`

**权限路由** (`decideAskRouting`)：
- System agent (checkpoint-writer/dream/distill) → `interactive: false`（自动拒绝）
- Orchestrator peer → `forward`（转发给父 session 审批）
- 普通后台子 agent → `inherit`（继承父 agent 的已授权权限）
- 正常 → `interactive: true`（交互式询问用户）

### 6.4 Fork Agent (检查点写入器)

`checkpoint-writer` 是特殊的 fork agent：
- 不重新计算 system + tools + messages，而是**冻结父 agent 的 LLM 请求前缀**
- `ForkContext` 存储在 `ActorRegistry` 的内存 Map 中
- 工具访问通过 `actor.tools` 白名单限制（运行时限制，不修改 schema）
- 确保与父 agent 的 prefix cache 对齐，节省 token 开销

---

## 7. 工具系统

### 7.1 工具定义 (`tool/tool.ts`)

```typescript
Tool.define(id, Effect) → Info {
  id: string,
  init: () => Effect<Def>  // 延迟初始化
}

Def = {
  id: string,
  description: string,       // .txt 文件内容 + 动态替换
  parameters: ZodSchema,     // Zod 验证
  execute(args, ctx): Effect<ExecuteResult>,
  shell?: { description, parse, recover? },  // Shell 模式支持
}

ExecuteResult = {
  title: string,
  metadata: Record<string, any>,
  output: string,            // 返回给 LLM 的文本
  attachments?: FilePart[],  // 可选附件（图片/PDF）
}
```

### 7.2 工具注册表 (`tool/registry.ts`)

- **初始化**：`Effect.all()` 并行初始化所有内置工具
- **过滤**：按 agent/model/feature flag 动态过滤
- **自定义工具**：从文件系统 (`{tool,tools}/*.{ts,js}`) 和插件加载
- **MCP 工具**：通过 MCP 协议注入

**工具过滤规则**：
- `CodeSearchTool` / `WebSearchTool` → 仅限 `opencode` / `xiaomi` provider
- `ApplyPatchTool` vs `EditTool` + `WriteTool` → 按 model ID（GPT 用 patch）
- `SessionTool` → 仅 orchestrator agent
- 各 agent 的 `toolAllowlist` 过滤
- Feature flags 控制实验性工具

### 7.3 工具执行流程

```
LLM tool_call
  → ToolRegistry 查找
  → Tool.Def.execute(args, ctx)
    ├─ Zod 参数验证 → 失败时 RecoverableError (agent 可重试)
    ├─ 实际执行
    ├─ 输出截断 (Truncate.Service)
    │   ├─ 大输出写入临时文件 (~7 天保留)
    │   └─ 返回预览 + 提示用 Grep/Read 访问完整内容
    └─ OpenTelemetry span
```

### 7.4 工具列表

| 分类 | 工具 | 说明 |
|------|------|------|
| 文件 | `read`, `edit`, `multiedit`, `write`, `notebook-edit`, `apply_patch` | 文件操作 |
| 搜索 | `glob`, `grep`, `codesearch` | 文件/内容搜索 |
| Shell | `bash`, `bash-interactive`, `change-directory` | 命令执行 |
| 知识 | `webfetch`, `websearch`, `memory`, `history`, `lsp` | 信息获取 |
| 编排 | `actor`, `task`, `workflow`, `skill`, `skill-search` | Agent 编排 |
| 安全 | `plan-enter`, `plan-exit`, `question` | 模式/安全 |
| 其他 | `cron`, `session`, `fleet`, `tool-script` | 调度/可观测性 |

### 7.5 Shell 模式

支持 shell 风格调用的工具可以接收 `{ script: string }` 参数：
1. `shell-tokenize.ts` 解析 shell 脚本（处理 heredoc、注释、引号）
2. 提取每条命令的操作
3. 顺序执行，首个失败即停止
4. 输出包裹在 `<command index="N" operation="...">` XML 块中

---

## 8. Session 管理

### 8.1 Session 数据模型 (`session.sql.ts`)

```sql
session {
  id              TEXT PRIMARY KEY,  -- descending("session") 递减ID
  project_id      TEXT FK→project,
  workspace_id    TEXT,
  parent_id       TEXT,              -- 父 session (子 agent)
  context_from    TEXT,              -- 继承上下文的父 session
  context_watermark TEXT,            -- 继承上下文的水位线
  slug            TEXT,
  directory       TEXT,
  title           TEXT,
  version         INTEGER,
  share_url       TEXT,
  summary_*       TEXT,              -- 摘要相关字段
  revert          JSON,              -- 回滚状态
  permission      JSON,              -- 权限配置
  last_checkpoint_message_id TEXT,
  created_at / updated_at INTEGER
}

message {
  id              TEXT PRIMARY KEY,  -- ascending("message") 递增ID
  session_id      TEXT FK→session CASCADE,
  agent_id        TEXT DEFAULT "main",
  data            JSON,              -- MessageV2.Info (不含 id/sessionID)
  time_created / time_updated INTEGER
}

part {
  id              TEXT PRIMARY KEY,  -- ascending("part") 递增ID
  message_id      TEXT FK→message CASCADE,
  session_id      TEXT,
  data            JSON,              -- MessageV2.Part (不含 id/sessionID/messageID)
  time_created / time_updated INTEGER
}
```

### 8.2 Session 生命周期

```
创建: Session.create()
  → 生成 descending SessionID
  → 写入 DB
  → 注册到 ActorRegistry
  → 发布 Session.Event.Created

运行: SessionPrompt.prompt()
  → createUserMessage() → runLoop()
  → SessionRunState.ensureRunning() (fiber 级并发控制)

结束:
  → 标题生成 (title agent)
  → 摘要生成 (summary agent)
  → 状态设为 idle

删除: Session.remove()
  → CASCADE 删除所有 messages/parts
  → 发布 Session.Event.Deleted
```

### 8.3 并发控制 (`run-state.ts`)

- 每个 (sessionID, agentID) 对最多一个 Runner fiber
- `ensureRunning()` — 如果已有 runner 在运行，返回 409 Conflict
- 状态: `idle` → `busy` → `idle`（主 agent 控制 session 状态）

### 8.4 多 Agent Slice

同一 session 内可运行多个 agent（主 agent + 子 agent）：
- 消息按 `agentID` 过滤
- `undefined` 或 `"main"` → 仅主 agent 消息
- `"*"` → 所有 agent 消息
- 子 agent 通过 `ActorRegistry` 注册，可设置 `tools` 白名单

### 8.5 Parent-Child Session

- 子 session 的 `parent_id` 指向父 session
- `context_from` / `context_watermark` 定义从父 session 继承的上下文范围
- Fork agent (checkpoint-writer) 冻结父前缀用于 prefix cache 对齐

---

## 9. Provider 抽象层

### 9.1 Provider 发现链

```
1. models.dev 数据库 → 远程模型元数据 (成本/限制/能力)
2. mimocode.json 配置 → 自定义覆盖
3. 环境变量 → 自动检测 (ANTHROPIC_API_KEY, OPENAI_API_KEY 等)
4. API Keys → 已存储的认证记录
5. 插件钩子 → 自定义认证加载
6. 自定义加载器 → Provider 特定初始化 (20+ provider)
```

### 9.2 SDK 加载

`BUNDLED_PROVIDERS` Map 懒加载 20+ AI SDK 包：
`@ai-sdk/anthropic`, `@ai-sdk/openai`, `@ai-sdk/google`, `@ai-sdk/azure`, `@ai-sdk/amazon-bedrock`, `@ai-sdk/github-copilot` 等。

非内置 provider 通过 NPM 动态安装。

### 9.3 流式处理

- **SSE Chunk 超时** — 每个 chunk 480s (8 min) 超时，防止无限挂起
- **持久重试** — 指数退避 500ms × 2，最多 10 次，单次上限 5 min
- **Prefill 拒绝重试** — 检测 400 错误中的 prefill 拒绝，自动修剪尾部 assistant 并重试

### 9.4 错误分类

- **可重试瞬态错误** — 429, 5xx, 网络错误, SSE 超时
- **不可重试** — 401/403 (认证), 400/404/422 (客户端错误), 用户中止
- **Overflow 检测** — 15+ provider 的正则模式匹配 `context_length_exceeded`

---

## 10. 历史会话与记忆

### 10.1 History 搜索 (`history/`)

基于 SQLite FTS5 的全文搜索：

```sql
-- FTS 虚拟表
CREATE VIRTUAL TABLE history_fts USING fts5(
  session_id, message_id, part_id,
  content,        -- 提取的文本内容
  role,           -- user/assistant
  project_id,
  content=history_content, content_rowid=rowid
)
```

- **Writer** — 订阅 Bus 事件，队列化写入 FTS 索引
- **Part 提取** — 从 text/tool/reasoning part 提取可索引内容
- **Backfill** — 扫描并批量写入未索引的历史数据
- **搜索** — FTS5 MATCH 查询 + BM25 排序

### 10.2 Memory 系统 (`memory/`)

```typescript
// 存储路径
~/.local/share/opencode/memory/
  ├── global/MEMORY.md           // 全局记忆
  ├── projects/<pid>/MEMORY.md   // 项目记忆
  └── sessions/<sid>/
      ├── checkpoint.md          // Session 检查点
      ├── notes.md               // 草稿本
      └── tasks/<id>/progress.md // 任务进度
```

- **FTS5 索引** — 与 History 分开的独立 FTS 表
- **Reconciliation** — 磁盘文件与 FTS 索引的双向同步
- **Scoping** — `global` / `project` / `session` / `task` 四级作用域
- **Type Taxonomy** — `user` / `feedback` / `project` / `reference` 四种类型

### 10.3 Checkpoint 系统

Checkpoint Writer 子 agent 定期写入结构化快照（11 个章节）：

```
§1 Active intent     — 当前意图
§2 Next action       — 下一步动作
§3 Directives        — 指令
§4 Task tree         — 任务树
§5 Current work      — 当前工作
§6 Files             — 相关文件
§7 Discovered knowledge — 发现的知识
§8 Errors            — 错误记录
§9 Live resources    — 活跃资源
§10 Design decisions — 设计决策
§11 Open notes       — 开放笔记
```

**触发时机**：token 压力达到阈值 (20%/40%/60%/80%) 时自动触发。

**重建上下文**：checkpoint.md + MEMORY.md + notes.md + global/MEMORY.md 拼接为合成 user 消息注入上下文。

---

## 11. 事件总线 (`bus/`)

```typescript
// 类型化发布/订阅
Bus.publish(EventDefinition, properties) → void
Bus.subscribe(EventDefinition) → Stream
Bus.subscribeAll() → Stream  // 通配符

// 事件定义
BusEvent.define(type, ZodSchema) → EventDefinition
```

- 每个 project instance 独立的 PubSub 集
- `GlobalBus` — 跨 instance 的 EventEmitter 单例
- SSE Event Stream — 容量 10000，丢弃最旧策略

**关键事件**：
- `Session.Event.Created/Updated/Deleted/Error/RetryAttempt/TryBestDetected`
- `SessionCompaction.Event.Compacted`
- `Task.Event.Created/Updated`
- `Permission.Event.Requested/Replied`
- `Metrics.ModelCall/ToolCall/AgentRequest`

---

## 12. Server 层 (`server/`)

### 12.1 HTTP API

框架：Hono，双运行时支持（Bun WebSocket + Node.js）

**中间件栈**：
```
Error → CORS → Logger → Auth (Basic) → Compression → Instance → Fence → Route
```

**关键端点**：
- `POST /session/:id/message` — 核心 prompt 端点，SSE 流式响应
- `POST /session/:id/prompt_async` — 异步 prompt（限流）
- `POST /session/:id/revert` / `unrevert` — 基于 snapshot 的回滚
- `GET /session/:id/diff` — 文件 diff
- `GET /event` — SSE 事件流
- `POST /session/:id/ask` — 冻结 snapshot 上的侧问

### 12.2 Workspace 路由

多 workspace 支持：
- `WorkspaceRouterMiddleware` — 查找 session 的 workspaceID
- 本地 workspace → 直接处理
- 远程 workspace → `ServerProxy` 反向代理（HTTP + WebSocket）

---

## 13. 关键设计模式总结

1. **Effect-TS 全面使用** — 所有 I/O 通过 `Effect.Effect` 封装，提供类型化错误处理、依赖注入 (`Layer`)、结构化并发
2. **SyncEvent 持久化** — 所有变更通过 `SyncEvent.run()` 同时更新内存状态和 SQLite
3. **Branded ID** — 类型安全的 ID 生成：SessionID (descending), MessageID/PartID (ascending)
4. **Slice 架构** — 消息按 agentID 切片过滤，支持多 agent 并发
5. **两层记忆** — Session 级 (checkpoint.md, notes.md) + Project 级 (MEMORY.md)
6. **Prefix Cache 对齐** — Fork agent 冻结父前缀，确保字节级一致
7. **恢复策略阶梯** — 多级恢复（输出长度→无效输出→文本工具调用→文本重复→空步骤→目标门控）
8. **插件钩子** — `session.pre/post`, `session.userQuery.pre/post`, `tool.execute.before/after`, `chat.params/headers` 等扩展点
