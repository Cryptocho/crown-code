# Changelog

## Context 上下文缓冲模块

### Added
- `src/context.nim`：`Context` ref object 类型，提供 `newContext`、`addLine`、`clearContext` 三个 proc
- `tests/test_context.nim`：5 个测试用例，覆盖 create / edge cases / add line / reset / nil safety

### Changed
- `tests/test_runner.nim`：注册 `test_context` 测试模块
- `TODO.md`：标记 Context 模块已完成

- Affected files: `src/context.nim`, `tests/test_context.nim`, `tests/test_runner.nim`, `TODO.md`
