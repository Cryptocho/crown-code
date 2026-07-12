# Development Guide

## Project Description

A vibe coding TUI tool. Core (daemon-style process) communicates with frontends (TUI, future GUI/WebUI) via IPC (JSON-RPC over stdio/Unix socket). Replaces the original Nim prototype.

Compared to cline, this project aims to deliver:
- **Finer-grained rollback**: checkpoint after every file edit, not just at request boundaries
- **Detailed error info**: structured JSON error responses with context, not plain strings
- **Multi-session**: single core process handles multiple frontend sessions simultaneously
- **Better TUI**: built with ratatui, full terminal UI with split panes, live streaming
- **Workspace vector index**: build and query a vector index over the entire workspace for semantic search
- **Accurate session cost stats**: include subagent calls in total cost tracking, not just top-level LLM requests

## Project Structure

```
.
├── Cargo.toml                   # Workspace root
├── Cargo.lock                   # Workspace-level lockfile
├── core/                        # Core daemon binary (crown-core)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── tui/                         # TUI frontend binary (crown-tui)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── nim/                         # Original Nim prototype (archived)
│   ├── src/
│   ├── tests/
│   └── ...
├── flake.nix                    # Nix flake — build system + dev env
├── flake.lock                   # Dependency lock
├── rust-toolchain.toml          # Rust toolchain version/components
├── .gitignore
├── AGENTS.md                    # This file
└── CHANGELOG.md
```

## Architecture

```
┌─────────────────┐
│  tui (ratatui)  │  ←── JSON-RPC over stdio/socket ──→  ┌────────────────────────┐
├─────────────────┤                                       │  core (daemon)         │
│ gui (future)    │  ←── JSON-RPC over socket ──────────→  │                        │
├─────────────────┤                                       │  - Agent loop          │
│ webui (future)  │  ←── JSON-RPC over WebSocket ──────→  │  - File operations     │
└─────────────────┘                                       │  - MCP client          │
                                                          │  - Session manager     │
                                                          │  - Vector index        │
                                                          │  - Cost tracking       │
                                                          │  - Checkpoint system   │
                                                          └────────────────────────┘
```

- **JSON-RPC 2.0** over stdio for TUI, Unix domain sockets for GUI/WebUI
- **Multi-session**: core assigns each frontend connection a session ID; sessions are isolated
- **Checkpoint on every file write**: each edit creates a git-like checkpoint for rollback

## Development Process

### Building the Project

All dependencies (Rust toolchain, openssl, pkg-config) managed by Nix:

```bash
# Sandbox build (fully reproducible, for CI/deployment)
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
cargo clippy -p crown-core
cargo clippy -p crown-tui
```

### Development Process

1. Propose a plan and wait for approval
2. Implement the plan; if unworkable at any step, stop and report
3. Use subagent to review uncommitted code
4. Update CHANGELOG.md (after review, not before)
5. Check whether AGENTS.md needs updating
6. Ask the user about commit message; present preview in English
7. After confirmation, commit **ALL** changes (`git add -A`) and push

## Coding Style

- Rust naming conventions: snake_case for functions/variables, PascalCase for types
- 4-space indentation
- `cargo fmt` before committing
- `cargo clippy` — no warnings
- Module structure: one module per file, nested modules in directories

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