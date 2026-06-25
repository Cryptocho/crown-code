# Changelog

## Glob 通配符匹配模块

### Added
- `src/glob.nim`：手动实现 fnmatch 算法（支持 `*`/`?`/`[...]` 回溯匹配），导出 `matchGlob`（单模式，`!` 前缀取反）和 `matchAnyGlob`（多模式，`!` 否定优先短路）
- `tests/test_glob.nim`：32 个测试用例，覆盖通配符、字符类、否定义前缀、多模式组合、边界情况

### Changed
- `tests/test_runner.nim`：注册 `test_glob` 测试模块
- `TODO.md`：标记 Glob 模块已完成

- Affected files: `src/glob.nim`, `tests/test_glob.nim`, `tests/test_runner.nim`, `TODO.md`

## Context 上下文缓冲模块

### Added
- `src/context.nim`：`Context` ref object 类型，提供 `newContext`、`addLine`、`clearContext` 三个 proc
- `tests/test_context.nim`：5 个测试用例，覆盖 create / edge cases / add line / reset / nil safety

### Changed
- `tests/test_runner.nim`：注册 `test_context` 测试模块
- `TODO.md`：标记 Context 模块已完成

- Affected files: `src/context.nim`, `tests/test_context.nim`, `tests/test_runner.nim`, `TODO.md`
