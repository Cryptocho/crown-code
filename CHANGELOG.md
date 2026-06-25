# Changelog

## 代码格式化模块

### Added
- `src/formatter.nim`：文件代码格式化模块。依赖 `pathutils`（路径解析）。公共类型：`FormatterError`（枚举，Success/NullPath/ReadFailed/MemoryAlloc）、`FormatterResult`（error/errorMessage）。内部 `processContent`（逐字符迭代，行尾空白修剪 + 行首空白规范化：含制表符替换为 4 空格、只有空格完全移除，保留原有空行结构）。主入口 `formatFile`（6 步流程：参数验证 → 路径解析 → 文件读取 → processContent → 文件覆盖写入 → 返回结果）
- `tests/test_formatter.nim`：10 个测试用例，2 个套件（error handling / content formatting），覆盖空路径、文件不存在、行尾空格修剪、行首 Tab 替换、混合空白处理、空格+Tab 混合、纯空格行首移除、无末尾换行保留、空文件

### Changed
- `tests/test_runner.nim`：注册 `test_formatter` 测试模块

- Affected files: `src/formatter.nim`, `tests/test_formatter.nim`, `tests/test_runner.nim`

## 文件编辑模块

### Added
- `src/file_edit.nim`：文件行级精确替换模块。依赖 `pathutils`（路径解析）、`ignore_rules`（clineignore 检测）、`file_writer`（文件写入）。公共类型：`FileEditError`（枚举，Success/FileNotFound/OldStringNotFound/MultipleMatches/ReadFailed/WriteFailed）、`FileEditResult`（error/errorMessage/matchCount）。内部辅助函数 `splitIntoLines`（按 `\n` 拆分，保留末尾空行）和 `joinLines`（行间插入 `\n`，末尾不加 `\n`）。主入口 `editFile`（10 步流程：路径解析 → clineignore → 文件读取 → 按行拆分 → 精确匹配计数 → 未找到/多次匹配检测 → 行替换 → 按行合并 → 写入文件 → 返回结果）。`editFile` 的 `multiple` 参数控制是否替换所有匹配行
- `tests/test_file_edit.nim`：15 个测试用例，5 个套件（error handling / basic functionality / edge cases / access control），覆盖空路径、文件不存在、未找到 oldStr、多次匹配、单次精确替换、全部替换、首行/末行/单行替换、空 oldStr 匹配空行、newStr 含换行、尾随换行符、空文件、clineignore 拦截

### Changed
- `tests/test_runner.nim`：注册 `test_file_edit` 测试模块

- Affected files: `src/file_edit.nim`, `tests/test_file_edit.nim`, `tests/test_runner.nim`

## 文件写入模块

### Added
- `src/file_writer.nim`：文件写入与缓存失效模块。依赖 `pathutils`（路径解析）、`ignore_rules`（clineignore 检测）和 `file_reader`（`cacheInvalidate`）。公共类型：`FileWriterError`（枚举，Success/NullPath/FileNotFound/PermissionDenied/WriteFailed）、`FileWriterResult`（error/errorMessage）。主入口 `writeFileContent`（6 步流程：参数验证 → clineignore → 路径解析 → 文件写入 → 缓存失效 → 返回成功）。`writeFileContent` 使用 `content: string = ""` 默认参数，对应 C 的 NULL content 处理
- `tests/test_file_writer.nim`：8 个测试用例，5 个套件（error handling / basic functionality / caching / access control / write failure），覆盖空路径、文件写入、空内容写入、缓存失效验证、clineignore 拦截、不存在目录写入、只读目录写入、重复写入缓存行为

### Changed
- `tests/test_runner.nim`：注册 `test_file_writer` 测试模块

- Affected files: `src/file_writer.nim`, `tests/test_file_writer.nim`, `tests/test_runner.nim`

## 文件读取模块

### Added
- `src/file_reader.nim`：文件读取与格式化输出模块。依赖 `pathutils`（路径解析）和 `ignore_rules`（clineignore 检测）。公共类型：`FileReaderError`（枚举，SUCCESS/NULL_PATH/FILE_NOT_FOUND/PERMISSION_DENIED/READ_FAILED/MEMORY_ALLOC）、`LineRange`（startLine/endLine/totalLines/truncated）、`FileReaderResult`（content/range/error/errorMessage）、`FileCacheEntry`。缓存子系统：256 槽位哈希表（大小写不敏感 hash），`cacheGet`/`cacheSet`/`cacheInvalidate` 三接口。辅助函数：`getFileMtime`（`getLastModificationTime` 封装）、`countLines`（`\n` 计数）、`parseLineRange`（范围解析与自动交换）、`formatContentWithLineNumbers`（`行号 | 内容` 格式化 + 尾部文件统计）。主入口 `readFileRange`（12 步流程：参数验证 → clineignore → 路径解析 → 缓存 + mtime 检测 → 重复读取警告([File already read]/[DUPLICATE READ]) → 文件读取 → 行统计 → 格式化 → 拼接输出）
- `tests/test_file_reader.nim`：15 个测试用例，5 个套件（error handling / basic functionality / caching / large files / path resolution），覆盖 null 路径、空路径、文件不存在、基础行号格式化、行范围读取、范围交换、超出 EOF、单行文件、缓存重复读取警告（3 次语义）、mtime 变化驱逐、大文件（2000 行）、绝对路径、相对路径

- Affected files: `src/file_reader.nim`, `tests/test_file_reader.nim`, `tests/test_runner.nim`

## clineignore 忽略规则模块

### Added
- `src/ignore_rules.nim`：导入 `pathutils` 和 `glob`，依赖 3.1 路径解析。导出 3 个公共 API：`loadIgnoreFile`（读取 `.clineignore` 文件，跳过空行/`#` 注释，去除尾部空白）、`resetIgnoreRules`（测试用重置）、`checkIgnorePath`（主入口：绝对路径转相对 → `fnmatchPathname` 匹配 → 无 `/` 的 pattern 加 `*/` 前缀 → 全局规则 > 项目规则）。采用全局懒初始化状态（`initIgnoreRules`），与 C 代码一致
- `tests/test_ignore_rules.nim`：12 个测试用例，2 个套件（loadIgnoreFile / checkIgnorePath），覆盖非存在文件、注释跳过、尾部空白去除、空行跳过、简单 glob 匹配、`*/` 前缀子目录匹配（一级）、含 `/` pattern 精确路径匹配、`!` 否定规则、空路径安全、绝对路径转相对路径
- `src/glob.nim` 新增 `fnmatchPathname`（FNM_PATHNAME 语义：`*`/`?`/`[...]` 不匹配 `/`，`*` 回溯不跨越路径分隔符）和 `matchGlobPathname`（基于 `fnmatchPathname`）
- `tests/test_glob.nim` 新增 fnmatchPathname 测试套件（10 个测试用例），覆盖 star 不匹配 `/`、`*/` 前缀单级子目录、`?` 不匹配 `/`、段内 `*` 匹配、字符类不匹配 `/`、否定、精确路径、尾部 `*`、空 pattern、`*` 匹配空字符串

### Changed
- `tests/test_runner.nim`：注册 `test_ignore_rules` 测试模块

- Affected files: `src/glob.nim`, `src/ignore_rules.nim`, `tests/test_glob.nim`, `tests/test_ignore_rules.nim`, `tests/test_runner.nim`

## 路径解析模块

### Added
- `src/pathutils.nim`：三个路径解析 proc：`resolveWorkspacePath`（相对/绝对路径解析）、`toRelPath`（相对路径计算，反斜杠归一化为正斜杠）、`resolvePath`（返回 `(absolutePath, displayPath)` 元组）
- `tests/test_pathutils.nim`：14 test cases, 4 suites（normalizeSlashes / resolveWorkspacePath / toRelPath / resolvePath），covers separator normalization, CWD prefix stripping, backslash conversion, custom cwd parameter

### Changed
- `tests/test_runner.nim`：注册 `test_pathutils` 测试模块

- Affected files: `src/pathutils.nim`, `tests/test_pathutils.nim`, `tests/test_runner.nim`

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
