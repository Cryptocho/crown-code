# Changelog

## JSON 搜索输出格式化模块

### Added
- `src/search_json.nim`：JSON 搜索输出格式化，导出 4 个公共 API：`jsonEscape`（JSON 字符串转义，处理 `"`/`\`/`\n`/`\t`/`\r`）、`formatStartJson`（搜索起始标记）、`formatEndJson`（搜索结束标记）、`formatMatchJson`（匹配结果 JSON，支持 `context_before`/`context_after` 数组）
- `tests/test_search_json.nim`：25 个测试用例，4 个套件（jsonEscape / formatStartJson / formatEndJson / formatMatchJson），覆盖特殊字符转义、上下文输出、nil 安全

### Changed
- `tests/test_runner.nim`：注册 `test_search_json` 测试模块

- Affected files: `src/search_json.nim`, `tests/test_search_json.nim`, `tests/test_runner.nim`

## Search 正则搜索模块

### Added
- `src/search.nim`：使用 `std/re`（PCRE 封装）替代 C 的 PCRE2 FFI，导出 5 个公共 API：`newSearch`（编译正则，支持 `soCaseInsensitive`/`soMultiLine`/`soDotAll` 选项）、`matchFirst`（单次匹配，返回 `Option[Match]`）、`matchAll`（全部匹配，返回 `seq[Match]`）、`calcLineNumber`（偏移量 → 1-based 行号）、`getLine`（行号 → 行内容）
- `tests/test_search.nim`：29 个测试用例，7 个套件（newSearch / matchFirst / matchAll / calcLineNumber / getLine / options），覆盖无效正则、偏移匹配、跨行匹配、选项标志、边界情况

### Changed
- `tests/test_runner.nim`：注册 `test_search` 测试模块

- Affected files: `src/search.nim`, `tests/test_search.nim`, `tests/test_runner.nim`

## XDiff Unified Diff 引擎

### Added
- `src/xdiff.nim`：基于 `experimental/diff.diffText`（Myers O(ND)）的 unified diff 引擎，导出 `diff*` 公共 API（`diff*(a, b: string; ctxLen: int = 3): string`）。支持上下文窗口合并（间距 ≤ 2×ctxLen）、0 计数 hunk header（pure addition/deletion）、`\ No newline at end of file` 内联标记、尾部换行符差异检测
- `tests/test_diff.nim`：19 个测试用例，5 个套件（basic / single change / context window / edge cases），覆盖空输入、换行符边界、上下文窗口、hunk 合并、header 格式

### Changed
- `tests/test_runner.nim`：注册 `test_diff` 测试模块

- Affected files: `src/xdiff.nim`, `tests/test_diff.nim`, `tests/test_runner.nim`

## Glob 通配符匹配模块

### Added
- `src/glob.nim`：手动实现 fnmatch 算法（支持 `*`/`?`/`[...]` 回溯匹配），导出 `matchGlob`（单模式，`!` 前缀取反）和 `matchAnyGlob`（多模式，`!` 否定优先短路）
- `tests/test_glob.nim`：32 个测试用例，覆盖通配符、字符类、否定义前缀、多模式组合、边界情况

### Changed
- `tests/test_runner.nim`：注册 `test_glob` 测试模块

- Affected files: `src/glob.nim`, `tests/test_glob.nim`, `tests/test_runner.nim`

## Context 上下文缓冲模块

### Added
- `src/context.nim`：`Context` ref object 类型，提供 `newContext`、`addLine`、`clearContext` 三个 proc
- `tests/test_context.nim`：5 个测试用例，覆盖 create / edge cases / add line / reset / nil safety

### Changed
- `tests/test_runner.nim`：注册 `test_context` 测试模块

- Affected files: `src/context.nim`, `tests/test_context.nim`, `tests/test_runner.nim`
