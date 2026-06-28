# Development Guide

## Project description
A vibe coding tui tool written in nim, similar to cline but with some additional features

## Project Structure

```
.
├── src/                          # Nim source files (.nim)
│   ├── crown_code.nim            # Main entry point
│   ├── context.nim               # Context buffer (linesBefore/linesAfter)
│   ├── glob.nim                  # Glob pattern matching (fnmatch)
│   ├── ignore_rules.nim          # .clineignore rule matching
│   ├── pathutils.nim             # Path resolution and normalization
│   ├── file_reader.nim           # File reading with line numbering + cache
│   ├── file_writer.nim           # File writing + cache invalidation
│   ├── file_edit.nim             # Line-level exact replace
│   ├── formatter.nim             # Code formatting (tabs→spaces, trailing trim)
│   ├── shell_detect.nim          # Shell detection ($SHELL / PATH)
│   ├── command_exec.nim          # Process spawn, output capture, timeout
│   ├── search.nim                # Regex search (std/re)
│   ├── search_json.nim           # JSON search output formatting
│   ├── xdiff.nim                 # Unified diff engine (Myers O(ND))
│   └── mcp/                      # MCP client protocol stack
│       ├── jsonrpc.nim           # JSON-RPC 2.0 message builder/parser
│       ├── transport_stdio.nim   # Stdio transport (fork/exec/pipe + select I/O)
│       ├── transport_http.nim    # HTTP/SSE transport (TLS, chunked, event-stream)
│       └── sse.nim               # W3C Server-Sent Events parser
├── tests/                        # Nim test files (.nim)
│   ├── test_runner.nim           # Test entry point (imports all suites)
│   ├── test_file_reader.nim      # File reader tests (15 cases)
│   ├── test_file_writer.nim      # File writer tests (8 cases)
│   ├── test_file_edit.nim        # File edit tests (15 cases)
│   ├── test_formatter.nim        # Formatter tests (10 cases)
│   ├── test_shell_detect.nim     # Shell detect tests (6 cases)
│   ├── test_command_exec.nim     # Command exec tests (27 cases)
│   ├── config.nims               # Test config (--path:src)
│   ├── test_template.nim         # Bootstrap template test
│   ├── test_context.nim          # Context tests (5 cases)
│   ├── test_glob.nim             # Glob tests (32 cases)
│   ├── test_pathutils.nim        # Path utils tests (14 cases)
│   ├── test_diff.nim             # XDiff tests (19 cases)
│   ├── test_search.nim           # Search tests (29 cases)
│   ├── test_search_json.nim      # JSON search output tests (25 cases)
│   ├── test_mcp_jsonrpc.nim      # JSON-RPC tests (14 cases)
│   ├── test_mcp_stdio.nim        # Stdio transport tests (7 cases)
│   ├── test_mcp_http.nim         # HTTP transport tests
│   └── test_mcp_sse.nim          # SSE parser tests (33 cases)
├── build/                        # Build output directory
│   ├── debug/                    # Debug binary
│   ├── release/                  # Release binary
│   └── test/                     # Test runner binary
├── ratatui-ffi/                  # Rust ratatui binding for future TUI (FFI submodule)
├── temp/                         # C code migration reference (historical)
├── crown_code.nimble             # Nimble package file (build, test, deps)
├── config.nims                   # Project-level Nim config (mm:orc, threads:on)
├── Makefile                      # Build script (wraps nimble, moves binary)
├── .gitignore                    # Git ignore rules
├── .kilo/                        # Kilo planning and config
│   └── plans/                    # Implementation plans
├── TODO.md                       # Migration progress tracker
├── CHANGELOG.md                  # Feature-level changelog
├── AGENTS.md                     # This file
├── cline/                        # cline source code for reference
└── CLINE.md                      # cline content description
```

## Workflow

### Building the Project

Use Make (wraps nimble for dependency management):

```bash
make          # Debug build and run
make debug    # Debug build, output to build/debug/crown-code
make release  # Optimized build, output to build/release/crown-code
make test     # Run all tests, test binary in build/test/
make clean    # Remove build artifacts
```

DO NOT use `nim c` command directly in project root — use `make` instead

### Development Process
1. Propose a plan and wait for approval
2. Implement the plan, if the plan is found to be unworkable at any time, you should stop and report
3. Use subagent to review uncommitted code for correctness, elegance, consistency, and absence of logic errors
4. Update TODO.md (if exist)
5. After review or upon user request, update CHANGELOG.md. Modifying CHANGELOG.md before review is prohibited
6. Check whether AGENTS.md needs to be updated
7. Ask the user if they want to write a commit message; if so, present an English commit message preview for confirmation before committing. Direct submission is prohibited
8. After confirmation, commit **ALL** changes (git add -A) and **push**
> - Plans must include detailed steps and specifics, including steps in the development process(from 3 to 8 all written in plan)
> - Before creating a plan, you **MUST** spawn a subagent to review it for feasibility and completeness
> - CHANGELOG and commit messages must not contain internal milestone numbers (e.g. "Phase 2.1")
> - Commit message only lists project code files (`src/`, `tests/`, `config.nims`, etc.), excluding management files like `TODO.md`, `AGENTS.md`, `CHANGELOG.md`

### Adding New Features

1. Implement the feature in `src/`
2. Create a corresponding test file in `tests/`
  Build and verify: `make debug` or `make test`

### Adding Tests

After creating a new test file (e.g., `tests/test_new_feature.nim`), just run `make test`. 

### Test Log Format

`std/unittest` default output format:

```
[Suite] suite name
  [OK] test name          # passed
  [FAILED] test name      # failed (includes file:line:col and expression)
  [SKIPPED] test name     # skipped (via skip())
```

- `[OK]` — passed, no extra output
- `[FAILED]` — outputs file:line:col and the failed expression, e.g. `tests/test_foo.nim(9, 18): Check failed: 1 + 1 == 3`
- `[SKIPPED]` — skipped by `skip()` call

## Key Conventions

- Source files go in `src/`
- Test files go in `tests/`
- Always build using `make` in the project root
- DO NOT use `Glob` tool cause it can't see files in `.gitignore`, use terminal tools instead(`ls`)

## Coding Style

- camelCase for procs/vars, PascalCase for types
- Module names use snake_case (file `my_module.nim` → `import my_module`)
- Use 2-space indentation
- Prefer `func` (no side effects) over `proc` when possible
- Avoid `using` statement; pass context explicitly

## Respond Style

- Always respond in Chinese, do not use mermaid (flowchart)

## CHANGELOG Format Specification
Organize changes by feature module, using `## Feature Description` as the section title.

Required field: `- Affected files:` list all changed file paths (wrapped in backticks). Newer changes come first.

Note: `Affected files` only includes project code files (`src/`, `tests/`, `config.nims`, etc.), excluding management files like `TODO.md`, `AGENTS.md`, `CHANGELOG.md`.

Common subheadings:
- `### Added` — new features/files
- `### Refactored` — refactoring
- `### Bug Fixes` — bug fixes
- `### Architecture` — architectural decisions
- `### Breaking Changes` — breaking changes

---

## Nim Quick Reference

### Language Overview

Nim is a statically-typed, compiled systems language with Python-like syntax:
- Compiles to C (default), C++, or JavaScript via `nim c` / `nim cpp` / `nim js`
- Memory management: ORC in this project
- Powerful macro system and compile-time metaprogramming
- Zero-cost abstractions via template/macro expansion at compile time

### Import Convention

```nim
# Module file: src/foo/bar_baz.nim
# Import it as:
import foo/bar_baz
```

Import path maps to filesystem path relative to `src/`. The `src/` dir must be in Nim's search path (passed via `--path:src` or nimble).

### std/unittest — Core API

**Templates (test structure):**

| API | Description |
|-----|-------------|
| `suite(name, body)` | Define a test suite with optional `setup` / `teardown` sections |
| `test(name, body)` | Define a single test case |

**Assertions:**

| API | Description |
|-----|-------------|
| `check(conditions)` | Assert condition, continue on failure and print error |
| `require(conditions)` | Assert condition, **quit immediately** on failure |
| `expect(Exception1, Exception2, body)` | Assert body raises one of the listed exceptions |
| `fail()` | Manually mark test as failed |
| `skip()` | Skip current test (still executes, just marks skipped) |
| `checkpoint(msg)` | Set a named checkpoint, printed on test failure |

**Example:**

```nim
suite "math operations":
  setup:
    let x = 4

  test "addition works":
    check 2 + 2 == x

  test "division by zero raises":
    expect(DivByZeroDefect):
      discard 1 div 0
```

### Gotchas

- **DO NOT name procs `main`**: Nim treats `main` as a special identifier; it is invisible inside template-generated scopes like `suite`/`test`. Use `run`, `start`, `runApp`, etc. instead.
- **Export marker `*`**: Needed for symbols to be visible when the module is re-exported. Not required for direct `import mod; mod.proc()` calls, but recommended for public API.
- `suite`/`test` templates are `{.dirty.}` — they capture the enclosing module scope, but module-qualified names (`mod.proc()`) are safest.

### Running Tests

- `make test` — run test runner that imports all test modules
- When adding a new test file (`tests/test_feature.nim`), add `import test_feature` to `tests/test_runner.nim`
- Individual test file: `nim c -r --path:src tests/test_feature.nim`

### Nim Compiler Flags Quick Reference

| Flag | Effect |
|------|--------|
| `--out:PATH` | Output binary to PATH |
| `-d:release` | Release mode (optimizations, runtime checks off) |
| `-d:debug` | Debug mode (default) |
| `--path:DIR` | Add DIR to module search path |
| `-r` | Run after compilation |

### Project Layout Convention

```
src/             # Source files (.nim), module root
tests/           # Test files (.nim), import from src/ via config.nims
tests/config.nims # Nim config for tests (adds src/ to search path)
tests/test_runner.nim # Imports all test modules, single entry for `make test`
build/           # Build output directory
```