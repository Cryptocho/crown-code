use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::SystemTime;

use crate::ignore_rules::check_ignore_path;
use crate::pathutils::resolve_workspace_path;

const CACHE_SIZE: usize = 256;
const DEFAULT_MAX_LINES: i32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileReaderError {
    Success,
    NullPath,
    FileNotFound,
    PermissionDenied,
    ReadFailed,
}

#[derive(Debug, Clone, Copy)]
pub struct LineRange {
    pub start_line: i32,
    pub end_line: i32,
    pub total_lines: i32,
    pub truncated: i32,
}

#[derive(Debug, Clone)]
pub struct FileReaderResult {
    pub content: String,
    pub range: LineRange,
    pub error: FileReaderError,
    pub error_message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct FileCacheEntry {
    pub(crate) key: String,
    pub(crate) read_count: u32,
    pub(crate) mtime: SystemTime,
}

static CACHE: OnceLock<Mutex<Vec<FileCacheEntry>>> = OnceLock::new();

fn cache() -> &'static Mutex<Vec<FileCacheEntry>> {
    CACHE.get_or_init(|| {
        let mut v = Vec::with_capacity(CACHE_SIZE);
        for _ in 0..CACHE_SIZE {
            v.push(FileCacheEntry {
                key: String::new(),
                read_count: 0,
                mtime: SystemTime::UNIX_EPOCH,
            });
        }
        Mutex::new(v)
    })
}

fn cache_hash(path: &str) -> usize {
    let mut h: u64 = 0;
    for c in path.chars() {
        let lower = c.to_ascii_lowercase() as u64;
        h = h.wrapping_mul(31).wrapping_add(lower);
    }
    (h % CACHE_SIZE as u64) as usize
}

pub(crate) fn cache_get(absolute_path: &str) -> Option<FileCacheEntry> {
    let lock = cache().lock().unwrap();
    let idx = cache_hash(absolute_path);
    let entry = &lock[idx];
    if entry.read_count > 0 && entry.key == absolute_path {
        Some(entry.clone())
    } else {
        None
    }
}

pub(crate) fn cache_set(absolute_path: &str, mtime: SystemTime, read_count: u32) {
    let mut lock = cache().lock().unwrap();
    let idx = cache_hash(absolute_path);
    lock[idx] = FileCacheEntry {
        key: absolute_path.to_string(),
        read_count,
        mtime,
    };
}

fn cache_increment(absolute_path: &str) -> u32 {
    let mut lock = cache().lock().unwrap();
    let idx = cache_hash(absolute_path);
    let entry = &mut lock[idx];
    if entry.read_count > 0 && entry.key == absolute_path {
        entry.read_count += 1;
        entry.read_count
    } else {
        0
    }
}

pub fn cache_invalidate(absolute_path: &str) {
    let mut lock = cache().lock().unwrap();
    let idx = cache_hash(absolute_path);
    if lock[idx].key == absolute_path {
        lock[idx].read_count = 0;
    }
}

fn get_file_mtime(path: &str) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn count_lines(content: &str) -> i32 {
    let mut count = 0;
    for c in content.chars() {
        if c == '\n' {
            count += 1;
        }
    }
    count
}

fn parse_line_range(requested_start: i32, requested_end: i32) -> LineRange {
    let start_line = if requested_start > 0 {
        requested_start
    } else {
        1
    };
    let end_line = if requested_end > 0 {
        requested_end
    } else {
        start_line + DEFAULT_MAX_LINES - 1
    };
    let (start_line, end_line) = if requested_end > 0 && end_line < start_line {
        (end_line, start_line)
    } else {
        (start_line, end_line)
    };
    LineRange {
        start_line,
        end_line,
        total_lines: 0,
        truncated: 0,
    }
}

fn format_content_with_line_numbers(content: &str, range: &LineRange, total_lines: i32) -> String {
    let bytes = content.as_bytes();
    let mut pos = 0;
    let mut current_line = 1;

    while current_line < range.start_line && pos < bytes.len() {
        if bytes[pos] == b'\n' {
            current_line += 1;
        }
        pos += 1;
    }
    let start_pos = pos;

    while current_line <= range.end_line && pos < bytes.len() {
        if bytes[pos] == b'\n' {
            current_line += 1;
        }
        pos += 1;
    }
    let mut end_pos = pos;
    if end_pos > start_pos && end_pos > 0 && bytes[end_pos - 1] == b'\n' {
        end_pos -= 1;
    }

    let estimated_cap =
        (end_pos - start_pos) + (range.end_line - range.start_line + 1) as usize * 15 + 200;
    let mut result = String::with_capacity(estimated_cap);

    let mut out_line_num = range.start_line;
    let mut out_pos = start_pos;

    while out_pos < end_pos {
        result.push_str(&out_line_num.to_string());
        result.push_str(" | ");
        while out_pos < end_pos && bytes[out_pos] != b'\n' {
            result.push(bytes[out_pos] as char);
            out_pos += 1;
        }
        if out_pos < end_pos && bytes[out_pos] == b'\n' {
            result.push('\n');
            out_pos += 1;
        }
        out_line_num += 1;
    }

    result.push_str("\n\n");
    if range.end_line < total_lines {
        result.push_str(&format!(
            "(Showing lines {}-{} of {} total. Use start_line={} to continue reading.)",
            range.start_line,
            range.end_line,
            total_lines,
            range.end_line + 1
        ));
    } else {
        result.push_str(&format!("(File has {} lines total.)", total_lines));
    }

    result
}

pub fn read_file_range(path: &str, start_line: i32, end_line: i32) -> FileReaderResult {
    if path.is_empty() {
        return FileReaderResult {
            content: String::new(),
            range: LineRange {
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                truncated: 0,
            },
            error: FileReaderError::NullPath,
            error_message: "Path parameter is required".to_string(),
        };
    }

    if check_ignore_path(path) {
        return FileReaderResult {
            content: String::new(),
            range: LineRange {
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                truncated: 0,
            },
            error: FileReaderError::PermissionDenied,
            error_message: "Access denied by .crownignore rules".to_string(),
        };
    }

    let absolute_path = resolve_workspace_path(path);
    if absolute_path.is_empty() {
        return FileReaderResult {
            content: String::new(),
            range: LineRange {
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                truncated: 0,
            },
            error: FileReaderError::FileNotFound,
            error_message: "Could not resolve path".to_string(),
        };
    }

    let mut cached = cache_get(&absolute_path);
    if let Some(ref entry) = cached
        && entry.read_count > 0
    {
        let current_mtime = get_file_mtime(&absolute_path);
        if current_mtime != entry.mtime {
            cached = None;
        }
    }

    let mut dup_warning = String::new();
    if cached.is_some() {
        let new_count = cache_increment(&absolute_path);
        if new_count >= 3 {
            dup_warning = format!(
                "[DUPLICATE READ] You have already read '{}' {} times in this conversation. The content has not changed since your last read. Please use the information you already have and proceed with your task.\n\n",
                path, new_count
            );
        } else if new_count == 2 {
            dup_warning = format!(
                "[File already read] The file '{}' was already read earlier in this conversation. Returning content:\n\n",
                path
            );
        }
    }

    let content = match std::fs::read_to_string(&absolute_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = match e.kind() {
                std::io::ErrorKind::NotFound => "Error reading file: File not found",
                std::io::ErrorKind::PermissionDenied => "Error reading file: Permission denied",
                _ => "Error reading file: Read failed",
            };
            return FileReaderResult {
                content: String::new(),
                range: LineRange {
                    start_line: 0,
                    end_line: 0,
                    total_lines: 0,
                    truncated: 0,
                },
                error: FileReaderError::ReadFailed,
                error_message: msg.to_string(),
            };
        }
    };

    let total_lines = count_lines(&content);
    let mut range = parse_line_range(start_line, end_line);
    range.total_lines = total_lines;

    if range.start_line > total_lines {
        range.start_line = total_lines;
    }
    if range.end_line > total_lines {
        range.end_line = total_lines;
    }

    if cached.is_none() {
        let mtime = get_file_mtime(&absolute_path);
        cache_set(&absolute_path, mtime, 1);
    }

    let formatted_content = format_content_with_line_numbers(&content, &range, total_lines);

    let final_content = if dup_warning.is_empty() {
        formatted_content
    } else {
        format!("{}{}", dup_warning, formatted_content)
    };

    FileReaderResult {
        content: final_content,
        range,
        error: FileReaderError::Success,
        error_message: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;

    #[test]
    fn test_null_path() {
        let result = read_file_range("", 1, 10);
        assert_eq!(result.error, FileReaderError::NullPath);
        assert_eq!(result.error_message, "Path parameter is required");
    }

    #[test]
    fn test_file_not_found() {
        let result = read_file_range("/nonexistent_path_xyz/file.txt", 1, 10);
        assert_eq!(result.error, FileReaderError::ReadFailed);
    }

    #[test]
    fn test_read_failed_not_found() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_read_failed_not_found.txt");
        let result = read_file_range(path.to_str().unwrap(), 1, 10);
        assert_eq!(result.error, FileReaderError::ReadFailed);
        assert!(result.error_message.contains("File not found"));
    }

    #[test]
    fn test_read_failed_permission_denied() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_read_permission_denied.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "content").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o000)).unwrap();
            let result = read_file_range(file_path.to_str().unwrap(), 1, 10);
            assert_eq!(result.error, FileReaderError::ReadFailed);
            assert!(result.error_message.contains("Permission denied"));
            std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        #[cfg(not(unix))]
        {
            let _ = f;
        }
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_basic_line_number_format() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_basic_line_number.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "line1").unwrap();
        writeln!(f, "line2").unwrap();
        writeln!(f, "line3").unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 1, 3);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("1 | line1"));
        assert!(result.content.contains("2 | line2"));
        assert!(result.content.contains("3 | line3"));
        assert!(result.content.contains("(File has 3 lines total.)"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_line_range_subset() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_line_range_subset.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        for i in 1..=10 {
            writeln!(f, "line{}", i).unwrap();
        }
        let result = read_file_range(file_path.to_str().unwrap(), 3, 5);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(!result.content.contains("1 | line1"));
        assert!(result.content.contains("3 | line3"));
        assert!(result.content.contains("5 | line5"));
        assert!(!result.content.contains("6 | line6"));
        assert!(result.content.contains("(Showing lines 3-5 of 10 total."));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_range_auto_swap() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_range_swap.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        for i in 1..=15 {
            writeln!(f, "line{}", i).unwrap();
        }
        let result = read_file_range(file_path.to_str().unwrap(), 10, 3);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("3 | line3"));
        assert!(result.content.contains("10 | line10"));
        assert!(!result.content.contains("1 | line1"));
        assert!(!result.content.contains("2 | line2"));
        assert!(result.content.contains("(Showing lines 3-10 of 15 total."));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_range_beyond_eof() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_range_beyond_eof.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "only one line").unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 1, 100);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("1 | only one line"));
        assert!(result.content.contains("(File has 1 lines total.)"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_single_line_no_newline() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_single_line_nonl.txt");
        std::fs::write(&file_path, "hello").unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("(File has 0 lines total.)"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_single_line_with_newline() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_single_line_nl.txt");
        std::fs::write(&file_path, "hello\n").unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("1 | hello"));
        assert!(result.content.contains("(File has 1 lines total.)"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_duplicate_read_warning_second() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_dup_read_second.txt");
        std::fs::write(&file_path, "content\n").unwrap();
        let abs_path = resolve_workspace_path(file_path.to_str().unwrap());

        cache_set(&abs_path, get_file_mtime(&abs_path), 1);

        let result = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("[File already read]"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_duplicate_read_warning_third() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_dup_read_third.txt");
        std::fs::write(&file_path, "content\n").unwrap();
        let abs_path = resolve_workspace_path(file_path.to_str().unwrap());

        cache_set(&abs_path, get_file_mtime(&abs_path), 2);

        let result = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("[DUPLICATE READ]"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_cache_mtime_invalidation() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_cache_mtime_inval.txt");
        std::fs::write(&file_path, "original\n").unwrap();

        let old_mtime = SystemTime::UNIX_EPOCH;
        let abs_path = resolve_workspace_path(file_path.to_str().unwrap());
        cache_set(&abs_path, old_mtime, 5);

        let result = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(!result.content.contains("[DUPLICATE READ]"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_absolute_path() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_absolute_path.txt");
        std::fs::write(&file_path, "test\n").unwrap();
        let abs_path = file_path.to_str().unwrap();
        let result = read_file_range(abs_path, 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("1 | test"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_relative_path() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_relative_path.txt");
        std::fs::write(&file_path, "test\n").unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("1 | test"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_cache_first_read_no_warning() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_cache_first_read.txt");
        std::fs::write(&file_path, "content\n").unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(!result.content.contains("[File already read]"));
        assert!(!result.content.contains("[DUPLICATE READ]"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_start_line_zero_defaults_to_one() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_start_line_zero.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "line1").unwrap();
        writeln!(f, "line2").unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 0, 2);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("1 | line1"));
        assert!(result.content.contains("2 | line2"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_cache_get_set_roundtrip() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_cache_roundtrip.txt");
        let abs_path = resolve_workspace_path(file_path.to_str().unwrap());
        cache_set(&abs_path, SystemTime::UNIX_EPOCH, 42);
        let entry = cache_get(&abs_path);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().read_count, 42);
        cache_invalidate(&abs_path);
        assert!(cache_get(&abs_path).is_none());
    }

    #[test]
    fn test_cache_warning_three_reads_sequential() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_cache_three_reads.txt");
        std::fs::write(&file_path, "content\n").unwrap();
        let r1 = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(r1.error, FileReaderError::Success);
        assert!(!r1.content.contains("[File already read]"));
        assert!(!r1.content.contains("[DUPLICATE READ]"));
        let r2 = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(r2.error, FileReaderError::Success);
        assert!(r2.content.contains("[File already read]"));
        let r3 = read_file_range(file_path.to_str().unwrap(), 1, 1);
        assert_eq!(r3.error, FileReaderError::Success);
        assert!(r3.content.contains("[DUPLICATE READ]"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_large_file_reports_correct_total() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_large_file.txt");
        let mut content = String::new();
        for i in 1..=2000 {
            content.push_str(&format!("line{}\n", i));
        }
        std::fs::write(&file_path, &content).unwrap();
        let result = read_file_range(file_path.to_str().unwrap(), 1, 2000);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("(File has 2000 lines total.)"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_read_file_range_shows_showing_lines_message() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_showing_lines.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        for i in 1..=10 {
            writeln!(f, "line{}", i).unwrap();
        }
        let result = read_file_range(file_path.to_str().unwrap(), 2, 5);
        assert_eq!(result.error, FileReaderError::Success);
        assert!(result.content.contains("2 | line2"));
        assert!(result.content.contains("5 | line5"));
        assert!(!result.content.contains("1 | line1"));
        assert!(result.content.contains("(Showing lines 2-5 of 10 total."));
        assert!(result.content.contains("start_line=6"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    #[serial]
    fn test_read_file_denied_by_crownignore() {
        crate::ignore_rules::reset_ignore_rules();
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = std::env::temp_dir().join("crown_test_reader_crownignore");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_current_dir(&temp_dir).unwrap();
        let ignore_path = temp_dir.join(".crownignore");
        let mut f = std::fs::File::create(&ignore_path).unwrap();
        writeln!(f, "secret.txt").unwrap();
        f.flush().unwrap();
        drop(f);
        crate::ignore_rules::reset_ignore_rules();
        let result = read_file_range("secret.txt", 1, 10);
        assert_eq!(result.error, FileReaderError::PermissionDenied);
        assert_eq!(result.error_message, "Access denied by .crownignore rules");
        let _ = std::fs::remove_file(&ignore_path);
        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
