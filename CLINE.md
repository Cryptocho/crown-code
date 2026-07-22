# AGENTS.md — Cline Extension

## Project Overview

Cline is a VS Code extension with three surfaces: VS Code extension (TypeScript), React webview (`webview-ui/`), and React Ink CLI (`cli/`). The extension communicates with the webview via gRPC-like protocol over VS Code message passing (Protobuf-defined).

---

## Architecture

```
extension.ts → WebviewProvider → Controller → Task
                                    ↓
                        ┌───────────┴───────────┐
                        McpHub      StateManager
```

- **Controller** (`src/core/controller/index.ts`) — single source of truth for extension state
- **Task** (`src/core/task/index.ts`) — agent loop: API request → parse → present → execute tool → repeat
- **WebviewProvider** (`src/core/webview/index.ts`) — manages webview lifecycle and message passing
- **McpHub** (`src/services/mcp/McpHub.ts`) — manages MCP server connections

**Communication**: Protobuf-defined gRPC over VS Code `postMessage`. Schemas in `proto/`.

**State Flow**: Controller → postMessage → ExtensionStateContext (webview) ↔ useExtensionState hook

---

## Available Tools

All tools are defined in `src/shared/tools.ts` (`ClineDefaultTool` enum) with handlers in `src/core/task/tools/handlers/`.

| Tool ID | Handler File | Description |
|---------|--------------|-------------|
| `ask_followup_question` | `AskFollowupQuestionToolHandler.ts` | Ask user a followup question |
| `execute_command` | `ExecuteCommandToolHandler.ts` | Execute bash commands |
| `replace_in_file` | `WriteToFileToolHandler.ts` | Edit existing files |
| `read_file` | `ReadFileToolHandler.ts` | Read file contents |
| `write_to_file` | `WriteToFileToolHandler.ts` | Create new files |
| `search_files` | `SearchFilesToolHandler.ts` | Search file contents |
| `list_files` | `ListFilesToolHandler.ts` | List directory contents |
| `list_code_definition_names` | `ListCodeDefinitionNamesToolHandler.ts` | List code definitions |
| `browser_action` | `BrowserToolHandler.ts` | Browser automation |
| `use_mcp_tool` | `UseMcpToolHandler.ts` | Use MCP server tool |
| `access_mcp_resource` | `AccessMcpResourceHandler.ts` | Access MCP resource |
| `load_mcp_documentation` | `LoadMcpDocumentationHandler.ts` | Load MCP documentation |
| `new_task` | `NewTaskHandler.ts` | Create new task |
| `plan_mode_respond` | `PlanModeRespondHandler.ts` | Plan mode response |
| `act_mode_respond` | `ActModeRespondHandler.ts` | Act mode response |
| `focus_chain` | — | Todo/focus chain |
| `web_fetch` | `WebFetchToolHandler.ts` | Fetch web content |
| `web_search` | `WebSearchToolHandler.ts` | Search the web |
| `condense` | `CondenseHandler.ts` | Condense context |
| `summarize_task` | `SummarizeTaskHandler.ts` | Summarize task |
| `report_bug` | `ReportBugHandler.ts` | Report bug |
| `new_rule` | — | New rule |
| `apply_patch` | `ApplyPatchHandler.ts` | Apply patch |
| `generate_explanation` | `GenerateExplanationToolHandler.ts` | Generate explanation |
| `use_skill` | `UseSkillToolHandler.ts` | Use skill |
| `use_subagents` | `SubagentToolHandler.ts` | Use subagents |

### Read-Only Tools (safe for parallel execution)
`LIST_FILES`, `FILE_READ`, `SEARCH`, `LIST_CODE_DEF`, `BROWSER`, `ASK`, `WEB_SEARCH`, `WEB_FETCH`, `USE_SKILL`, `USE_SUBAGENTS`

---

## Code Organization

```
src/
├── core/
│   ├── controller/       # Request handlers (one file per RPC)
│   │   ├── task/         # Task-related handlers
│   │   ├── state/        # Settings update handlers
│   │   └── models/       # API config handlers
│   ├── prompts/          # System prompt generation
│   │   ├── system-prompt/
│   │   │   ├── components/   # Shared prompt sections
│   │   │   ├── variants/    # Model-specific configs
│   │   │   └── tools/       # Tool definitions
│   │   └── commands.ts   # Slash commands
│   ├── task/             # Task execution
│   │   └── tools/handlers/
│   ├── storage/          # StateManager, disk I/O
│   └── webview/          # WebviewProvider
├── shared/
│   ├── proto/            # Generated proto types
│   ├── proto-conversions/# Proto <-> internal type conversions
│   └── api.ts            # API provider definitions
├── services/mcp/         # MCP integration
├── hosts/               # Platform abstractions (vscode, standalone)
└── standalone/          # Core library for CLI/JetBrains
```

---

## Key Patterns & Gotchas

### Protobuf RPC Workflow (4 steps)
1. Define in `proto/cline/*.proto` (PascalCase Service, camelCase RPCs, PascalCase Messages)
2. Run `npm run protos`
3. Handler: `src/core/controller/<domain>/<method>.ts`
4. Call from webview: `UiServiceClient.myMethod(Request.create({...}))`

### Adding a New API Provider — 3 Required Proto Conversion Places
Missing any causes silent reset to Anthropic:
1. `proto/cline/models.proto` — add to `ApiProvider` enum
2. `convertApiProviderToProto()` in `src/shared/proto-conversions/models/api-configuration-conversion.ts`
3. `convertProtoToApiProvider()` in the same file

Also update: `src/shared/api.ts`, `src/shared/providers/providers.json`, `src/core/api/index.ts`, `webview-ui/.../providerUtils.ts`, `webview-ui/.../validate.ts`, `webview-ui/.../ApiOptions.tsx`, `cli/src/components/ModelPicker.tsx`.

### Responses API Providers
Providers using OpenAI Responses API require:
- Add to `isNextGenModelProvider()` in `src/utils/model-utils.ts`
- Set `apiFormat: ApiFormat.OPENAI_RESPONSES` on models
Without these, falls back to XML tools → broken tool calling.

### Adding Tools to System Prompt (5+ files)
1. `src/shared/tools.ts` — add to `ClineDefaultTool` enum
2. `src/core/prompts/system-prompt/tools/` — create tool definition (export `[GENERIC]` at minimum)
3. `src/core/prompts/system-prompt/tools/init.ts` — register in `allToolVariants`
4. `src/core/prompts/system-prompt/variants/*/config.ts` — add to each model family's tools list
5. Handler in `src/core/task/tools/handlers/`, wire in `ToolExecutor.ts`
6. If UI feedback: proto → `ExtensionMessage.ts` → `cline-message.ts` → `ChatRow.tsx`
7. Regenerate snapshots: `UPDATE_SNAPSHOTS=true npm run test:unit`

### Global State Keys (3 required places)
1. `src/shared/storage/state-keys.ts` — type definition
2. `src/core/storage/utils/state-helpers.ts` — `readGlobalStateFromDisk()` must call `context.globalState.get()`
3. Add to return object

Missing the `.get()` call compiles but value is always `undefined`.

### Slash Commands (3 places)
- `src/core/slash-commands/index.ts` — definitions
- `src/core/prompts/commands.ts` — system prompt integration
- `webview-ui/src/utils/slash-commands.ts` — webview autocomplete

### Storage — NEVER use VSCode ExtensionContext APIs
Do NOT use `context.globalState`, `context.workspaceState`, or `context.secrets`. Data must work in CLI and JetBrains too. Use:
```typescript
StateManager.get().getGlobalStateKey("key")
StateManager.get().setGlobalState("key", value)
```
Storage location: `~/.cline/data/globalState.json`, `secrets.json`, `workspaces/<hash>/workspaceState.json`

### Networking — Proxy-Aware Fetch
In extension code, use `@/shared/net` instead of global `fetch` or default axios:
```typescript
import { fetch } from '@/shared/net'  // For fetch
import { getAxiosSettings } from '@/shared/net'  // For axios
```
Webview code uses global `fetch` (browser handles proxies).

### ChatRow Cancelled/Interrupted Detection
When `status === "generating"` and cancelled, check TWO conditions:
```typescript
const wasCancelled =
    status === "generating" &&
    (!isLast ||
        lastModifiedMessage?.ask === "resume_task" ||
        lastModifiedMessage?.ask === "resume_completed_task")
```
`!isLast` catches completed-but-stale messages; `ask === "resume_task"` catches just-cancelled.

### Settings Round-Trip Wiring
If a toggle appears stuck or reverts, check the full round-trip:
1. `proto/cline/state.proto` — add to `UpdateSettingsRequest`
2. `npm run protos`
3. `Controller.getStateToPostToWebview()` — include key
4. `ExtensionMessage.ts` — add type
5. `ExtensionStateContext.tsx` — include in defaults

### Modifying System Prompt
- Components in `components/` (shared), variants in `variants/*/` (model-specific)
- Variants override via `componentOverrides` in `config.ts` or custom `template.ts`
- XS variant: heavily condensed inline content in `template.ts`
- After changes: `UPDATE_SNAPSHOTS=true npm run test:unit`

---

## Directories

```
cline/          # Root project (VS Code extension)
├── src/        # Extension TypeScript source
├── webview-ui/ # React webview (Vite)
├── cli/        # React Ink CLI
├── proto/      # Protobuf definitions
├── standalone/ # Core library for non-VSCode surfaces
├── testing-platform/ # Test infrastructure
├── evals/      # Evaluation scripts
└── docs/       # Docusaurus documentation
```
