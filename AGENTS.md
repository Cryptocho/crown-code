# Development Guide

## Project Description

A vibe coding TUI tool. Core (daemon-style process) communicates with frontends (TUI, future GUI) via IPC (JSON-RPC over stdio/Unix socket).

Compared to cline, this project aims to deliver:
- **Finer-grained rollback**: checkpoint(using shadow git) after every file edit, not just at request boundaries
- **Detailed error info**: send error message to frontend and auto-retry for some errors
- **Multi-session**: single core process handles multiple frontend sessions simultaneously
- **Better TUI**: built with ratatui, full terminal UI with split panes, live streaming, with status of cost/latency/throughtout... lightweight and high performance
- **Workspace vector index**: build and query a vector index over the entire workspace for semantic search
- **Accurate session cost stats**: include subagent calls in total cost tracking, not just top-level LLM requests
- **Regenerate when stop**: button for users to regenerate llm response
- **SWE-Pruner**: self-adaptive context pruning framework designed for LLM coding agents

## Project Structure

```
.
├── Cargo.toml                   # Workspace root (members: core, tui; edition 2024)
├── Cargo.lock                   # Workspace-level lockfile
├── core/                        # Core daemon/library (crown-core)
│   ├── Cargo.toml
│   ├── build.rs                 # Re-runs when mock_mcp_server.rs changes
│   └── src/
│       ├── main.rs              # Entry point — runs agent loop
│       ├── lib.rs               # Module registry (18 pub mod declarations)
│       ├── agent/               # Agent loop subsystem
│       │   ├── mod.rs
│       │   ├── tools.rs         # Tool definitions + execution dispatch
│       │   ├── prompt.rs        # System prompt builder
│       │   └── loop.rs          # Agent loop scheduler (API call → tool → result)
│       ├── api/                 # API client subsystem
│       │   ├── mod.rs
│       │   ├── openai.rs        # OpenAI-compatible API client
│       │   └── types.rs         # API type definitions
│       ├── mcp/                 # MCP (Model Context Protocol) subsystem
│       │   ├── mod.rs
│       │   ├── client.rs        # MCP client core with thread-safe transport
│       │   ├── jsonrpc.rs       # JSON-RPC 2.0 message builder/parser
│       │   ├── registry.rs      # Multi-server registry with lazy init
│       │   ├── sse.rs           # SSE stream parsing
│       │   ├── transport_http.rs # HTTP/Streamable transport
│       │   └── transport_stdio.rs# STDIO transport
│       ├── bin/
│       │   └── mock_mcp_server.rs # Mock MCP server for testing
│       ├── command_exec.rs      # Command execution module
│       ├── context.rs           # Context buffer
│       ├── file_edit.rs         # Line-level exact string replacement
│       ├── file_reader.rs       # File reader with line-numbered output
│       ├── file_writer.rs       # File writer
│       ├── formatter.rs         # Code formatter
│       ├── glob.rs              # Glob pattern matching (fnmatch)
│       ├── ignore_rules.rs      # .crownignore / .clineignore rule processing
│       ├── list_files.rs        # Directory listing
│       ├── pathutils.rs         # Path resolution and normalization
│       ├── search.rs            # Regex search
│       ├── search_files.rs      # File content search
│       ├── search_json.rs       # JSON search output formatter
│       ├── shell_detect.rs      # Shell detection
│       └── xdiff.rs             # Unified diff engine
├── tui/                         # TUI frontend (crown-tui)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Entry point: event loop + key dispatch
│       ├── app.rs               # App state struct + enums (SessionStatus/AgentMode/FocusTarget)
│       ├── app_event.rs         # AppEvent enum (internal message bus)
│       ├── event.rs             # TuiEvent enum (Key/Paste/Resize)
│       ├── tui.rs               # Tui terminal abstraction (init/restore/draw)
│       ├── ipc.rs               # IPC client (connect to core daemon)
│       ├── chatwidget.rs        # ChatWidget state (cells/active_cell/textarea/scroll)
│       ├── history_cell.rs      # HistoryCell trait + 5 cell types
│       ├── renderable.rs        # Renderable trait + FlexRenderable layout
│       ├── keymap.rs            # Key bindings (input/chat keymaps)
│       └── ui/
│           ├── mod.rs           # render() entry — Layout 3-zone composition
│           ├── status.rs        # Status bar (priority-based right-to-left assembly)
│           ├── chat.rs          # Chat panel (buf.set_line + scroll)
│           ├── input.rs         # Input bar (prefix + textarea + reversed cursor)
│           ├── tools.rs         # Tool call rendering helper
│           └── streaming.rs     # StreamingRenderer (P2 two-region model interface)upstream Cline extension
├── flake.nix                    # Nix flake — build system + dev env (crane)
├── flake.lock                   # Dependency lock
├── rust-toolchain.toml          # Rust toolchain version/components
├── .kilo/                       # Kilo CLI configuration
├── .vscode/                     # VS Code workspace settings
├── .gitignore
├── AGENTS.md                    # This file
└── CHANGELOG.md
```

## Architecture

```
┌─────────────────┐
│  tui (ratatui)  │  ←── JSON-RPC over stdio/socket ──→   ┌──────────────────────────┐
├─────────────────┤                                       │  core (daemon)           │
│ gui (future)    │  ←── JSON-RPC over socket ──────────→ │                          │
└─────────────────┘                                       │  ┌────────────────────┐  │
                                                          │  │ agent/             │  │
                                                          │  │  - tools           │  │
                                                          │  │  - prompt          │  │
                                                          │  │  - loop            │  │
                                                          │  └────────────────────┘  │
                                                          │  ┌────────────────────┐  │
                                                          │  │ api/               │  │
                                                          │  │  - openai          │  │
                                                          │  │  - types           │  │
                                                          │  └────────────────────┘  │
                                                          │  ┌────────────────────┐  │
                                                          │  │ mcp/               │  │
                                                          │  │  - client          │  │
                                                          │  │  - jsonrpc         │  │
                                                          │  │  - registry        │  │
                                                          │  │  - sse             │  │
                                                          │  │  - transport_http  │  │
                                                          │  │  - transport_stdio │  │
                                                          │  └────────────────────┘  │
                                                          │                          │
                                                          │  File operations:        │
                                                          │   file_reader            │
                                                          │   file_writer            │
                                                          │   file_edit              │
                                                          │   list_files             │
                                                          │   search_files           │
                                                          │                          │
                                                          │  Utilities:              │
                                                          │   command_exec           │
                                                          │   glob / xdiff / search  │
                                                          │   formatter              │
                                                          │   pathutils / context    │
                                                          │   shell_detect           │
                                                          │   ignore_rules           │
                                                          │                          │
                                                          │  IPC subsystem:          │
                                                          │   ipc/message            │
                                                          │   ipc/transport          │
                                                          │   ipc/session_manager    │
                                                          │   ipc/server             │
                                                          │                          │
                                                          │  [Planned]               │
                                                          │   Vector index           │
                                                          │   Cost tracking          │
                                                          │   Checkpoint system      │
                                                          └──────────────────────────┘
```

- **Multi-session**: core assigns each frontend connection a session ID; sessions are isolated
- **Checkpoint on every file write**: each edit creates a git-like checkpoint for rollback (planned)
- **Agent loop**: reads stdin user input → calls OpenAI-compatible API → executes tool calls → feeds results back → repeats until completion
- **MCP subsystem**: stdio + HTTP/SSE transports, JSON-RPC messaging, multi-server registry with lazy initialization

## Development Process

### Building the Project

All dependencies (Rust toolchain, openssl, pkg-config) managed by Nix via `crane`:

```bash
# Nix builds (fully reproducible, for CI/deployment)
nix build .#core              # Build core binary → ./result/bin/crown-core
nix build .#tui               # Build tui binary  → ./result/bin/crown-tui
nix build                     # Default: core

# Development shell (inherits all deps from nix build)
nix develop

# Inside nix develop:
cargo build -p crown-core
cargo build -p crown-tui
cargo test -p crown-core
cargo test -p crown-tui
cargo test --workspace        # Test all workspace members
cargo clippy -p crown-core
cargo clippy -p crown-tui

# Additional tools available in dev shell:
cargo add <dependency>                 # Add dependencies via cargo-edit
```

### Testing Best Practices

- **NEVER use `cargo test 2>&1 | tail -10`** — `tail` only shows the last test suite result, silently hiding failures in earlier suites when there are multiple crates. Use this instead:

```bash
cargo test 2>&1 | grep 'test result'
```

This prints **all** `test result:` lines across every test binary, so any failing suite is visible. The workspace currently has multiple test binaries (e.g. `crown-core` has unit tests + integration tests + doctests).

### Code Coverage

Line coverage via `cargo-llvm-cov`. Run inside `nix develop` or via `nix develop --command`:

```bash
# Full coverage with summary table
cargo llvm-cov --workspace

# Summary only (per-file coverage percentages)
cargo llvm-cov report --summary-only
```

### Development Process

1. Propose a plan after self verification and wait for approval
2. Implement the plan; if unworkable at any step, stop and report
3. Use subagent to review uncommitted code
4. Update TODO.md and CHANGELOG.md (after review, not before)
5. Check whether AGENTS.md needs updating
6. Ask the user about commit message; present preview in English
7. After confirmation, commit **ALL** changes (`git add -A`) and push

## Communication

- Reply in Chinese; Mermaid diagrams may be used where helpful

## Coding Style

- Rust naming conventions: snake_case for functions/variables, PascalCase for types
- 4-space indentation
- `cargo fmt` before committing
- `cargo clippy` — no warnings
- Module structure: one module per file, nested modules in directories
- Use `r#` raw identifiers (e.g. `r#loop`) when a module name conflicts with a Rust keyword
- Use `pub(crate)` visibility to expose items across sibling modules without making them public
- **Do not use phase numbers or TODO.md in commit messages or CHANGELOG entries.** Commit messages and changelogs describe what functionally changed, not which planning phase produced the change. Phase numbers belong in plan files (`.kilo/plans/`) and TODO.md only.

## CHANGELOG Format Specification

Organize changes by feature module, using `## Feature Description` as section title.

Required: `- Affected files:` list all changed file paths (backtick-wrapped). Newer changes first.

`Affected files` only includes project code (`core/`, `tui/`, `flake.nix`), not management files.

Common subheadings:
- `### Added` — new features/files
- `### Refactored` — refactoring
- `### Bug Fixes` — bug fixes
- `### Architecture` — architectural decisions
- `### Breaking Changes` — breaking changes
