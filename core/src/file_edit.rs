use crate::file_writer::{FileWriterError, write_file_content};
use crate::ignore_rules::check_ignore_path;
use crate::pathutils::resolve_workspace_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEditError {
    Success,
    FileNotFound,
    OldStringNotFound,
    MultipleMatches,
    ReadFailed,
    WriteFailed,
}

#[derive(Debug, Clone)]
pub struct FileEditResult {
    pub error: FileEditError,
    pub error_message: String,
    pub match_count: i32,
}

fn split_into_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, c) in content.char_indices() {
        if c == '\n' {
            lines.push(content[start..i].to_string());
            start = i + 1;
        }
    }
    lines.push(content[start..].to_string());
    lines
}

fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut result = lines[0].clone();
    for line in &lines[1..] {
        result.push('\n');
        result.push_str(line);
    }
    result
}

pub fn edit_file(path: &str, old_str: &str, new_str: &str, multiple: bool) -> FileEditResult {
    let absolute_path = resolve_workspace_path(path);
    if absolute_path.is_empty() {
        return FileEditResult {
            error: FileEditError::FileNotFound,
            error_message: "Could not resolve path".to_string(),
            match_count: 0,
        };
    }

    if check_ignore_path(path) {
        return FileEditResult {
            error: FileEditError::ReadFailed,
            error_message: "Access denied by .crownignore rules".to_string(),
            match_count: 0,
        };
    }

    let content = match std::fs::read_to_string(&absolute_path) {
        Ok(c) => c,
        Err(e) => {
            return FileEditResult {
                error: FileEditError::ReadFailed,
                error_message: format!("Error reading file: {}", e),
                match_count: 0,
            };
        }
    };

    let lines = split_into_lines(&content);

    let match_count = lines.iter().filter(|line| *line == old_str).count() as i32;

    if match_count == 0 {
        return FileEditResult {
            error: FileEditError::OldStringNotFound,
            error_message: "Could not find exact match for oldStr in file".to_string(),
            match_count: 0,
        };
    }

    if !multiple && match_count > 1 {
        return FileEditResult {
            error: FileEditError::MultipleMatches,
            error_message: format!(
                "Found multiple matches ({}), but multiple is false",
                match_count
            ),
            match_count,
        };
    }

    let mut new_lines = lines.clone();
    for line in &mut new_lines {
        if *line == old_str {
            *line = new_str.to_string();
            if !multiple {
                break;
            }
        }
    }

    let new_content = join_lines(&new_lines);

    let write_result = write_file_content(path, &new_content);
    match write_result.error {
        FileWriterError::Success => FileEditResult {
            error: FileEditError::Success,
            error_message: String::new(),
            match_count,
        },
        FileWriterError::WriteFailed => FileEditResult {
            error: FileEditError::WriteFailed,
            error_message: write_result.error_message,
            match_count,
        },
        _ => FileEditResult {
            error: FileEditError::ReadFailed,
            error_message: write_result.error_message,
            match_count,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_split_into_lines_basic() {
        let lines = split_into_lines("hello\nworld\n");
        assert_eq!(lines, vec!["hello", "world", ""]);
    }

    #[test]
    fn test_split_into_lines_no_trailing_newline() {
        let lines = split_into_lines("hello\nworld");
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn test_split_into_lines_empty() {
        let lines = split_into_lines("");
        assert_eq!(lines, vec![""]);
    }

    #[test]
    fn test_split_into_lines_single_line() {
        let lines = split_into_lines("hello");
        assert_eq!(lines, vec!["hello"]);
    }

    #[test]
    fn test_join_lines_basic() {
        let result = join_lines(&["hello".to_string(), "world".to_string()]);
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn test_join_lines_with_trailing_empty() {
        let result = join_lines(&["hello".to_string(), "".to_string()]);
        assert_eq!(result, "hello\n");
    }

    #[test]
    fn test_join_lines_single() {
        let result = join_lines(&["hello".to_string()]);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_join_lines_empty_vec() {
        let result = join_lines(&[]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_split_roundtrip() {
        let input = "abc\n";
        let lines = split_into_lines(input);
        assert_eq!(lines, vec!["abc", ""]);
        let output = join_lines(&lines);
        assert_eq!(output, "abc\n");
    }

    #[test]
    fn test_edit_file_not_found() {
        let result = edit_file("", "old", "new", false);
        assert_eq!(result.error, FileEditError::FileNotFound);
    }

    #[test]
    fn test_edit_file_nonexistent() {
        let result = edit_file("/nonexistent_path_xyz/edit.txt", "old", "new", false);
        assert_eq!(result.error, FileEditError::ReadFailed);
    }

    #[test]
    fn test_edit_file_old_string_not_found() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_old_not_found.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "hello").unwrap();
        writeln!(f, "world").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "nonexistent", "new", false);
        assert_eq!(result.error, FileEditError::OldStringNotFound);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_single_replace() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_single_replace.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "hello").unwrap();
        writeln!(f, "world").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "hello", "hi", false);
        assert_eq!(result.error, FileEditError::Success);
        assert_eq!(result.match_count, 1);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hi\nworld\n");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_replace_all() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_replace_all.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "hello").unwrap();
        writeln!(f, "world").unwrap();
        writeln!(f, "hello").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "hello", "hi", true);
        assert_eq!(result.error, FileEditError::Success);
        assert_eq!(result.match_count, 2);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hi\nworld\nhi\n");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_multiple_matches_error() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_multiple_error.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "hello").unwrap();
        writeln!(f, "hello").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "hello", "hi", false);
        assert_eq!(result.error, FileEditError::MultipleMatches);
        assert_eq!(result.match_count, 2);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_first_line() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_first_line.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "first").unwrap();
        writeln!(f, "second").unwrap();
        writeln!(f, "third").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "first", "changed", false);
        assert_eq!(result.error, FileEditError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "changed\nsecond\nthird\n");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_last_line() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_last_line.txt");
        std::fs::write(&file_path, "first\nmiddle\ntarget").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "target", "replaced", false);
        assert_eq!(result.error, FileEditError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "first\nmiddle\nreplaced");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_single_line_replace() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_single_line.txt");
        std::fs::write(&file_path, "only").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "only", "replaced", false);
        assert_eq!(result.error, FileEditError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "replaced");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_empty_old_str_matches_empty_lines() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_empty_old.txt");
        std::fs::write(&file_path, "a\n\nc\n").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "", "FILLED", true);
        assert_eq!(result.error, FileEditError::Success);
        assert_eq!(result.match_count, 2);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "a\nFILLED\nc\nFILLED");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_new_str_with_newline() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_newline_new.txt");
        std::fs::write(&file_path, "before\ntarget\nafter\n").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "target", "line1\nline2", false);
        assert_eq!(result.error, FileEditError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "before\nline1\nline2\nafter\n");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_trailing_newline() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_trailing_nl.txt");
        std::fs::write(&file_path, "abc\n").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "abc", "xyz", false);
        assert_eq!(result.error, FileEditError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "xyz\n");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_edit_file_multiple_matches_error_file_unchanged() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_multi_unchanged.txt");
        std::fs::write(&file_path, "hello\nworld\nhello\n").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "hello", "hi", false);
        assert_eq!(result.error, FileEditError::MultipleMatches);
        assert_eq!(result.match_count, 2);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello\nworld\nhello\n");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_empty_file_returns_old_string_not_found() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_edit_empty_file.txt");
        std::fs::write(&file_path, "").unwrap();
        let result = edit_file(file_path.to_str().unwrap(), "anything", "new", false);
        assert_eq!(result.error, FileEditError::OldStringNotFound);
        let _ = std::fs::remove_file(&file_path);
    }
}
