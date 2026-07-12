use std::fs;

use crate::pathutils::resolve_workspace_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatterError {
    Success,
    NullPath,
    ReadFailed,
}

#[derive(Debug, Clone)]
pub struct FormatterResult {
    pub error: FormatterError,
    pub error_message: String,
}

pub fn process_content(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'\n' {
            result.push('\n');
            i += 1;
            continue;
        }

        let line_start = i;
        while i < len && bytes[i] != b'\n' {
            i += 1;
        }
        let line_end = i;

        let mut trim_end = line_end;
        while trim_end > line_start && (bytes[trim_end - 1] == b' ' || bytes[trim_end - 1] == b'\t')
        {
            trim_end -= 1;
        }

        let mut wp = line_start;
        let mut has_leading_tabs = false;
        while wp < trim_end && (bytes[wp] == b' ' || bytes[wp] == b'\t') {
            if bytes[wp] == b'\t' {
                has_leading_tabs = true;
            }
            wp += 1;
        }

        if has_leading_tabs {
            result.push_str("    ");
        }

        while wp < trim_end {
            result.push(bytes[wp] as char);
            wp += 1;
        }

        if i < len && bytes[i] == b'\n' {
            result.push('\n');
            i += 1;
        }
    }

    result
}

pub fn format_file(path: &str) -> FormatterResult {
    if path.is_empty() {
        return FormatterResult {
            error: FormatterError::NullPath,
            error_message: "Path parameter is required".to_string(),
        };
    }

    let absolute_path = resolve_workspace_path(path);
    if absolute_path.is_empty() {
        return FormatterResult {
            error: FormatterError::ReadFailed,
            error_message: "Could not resolve path".to_string(),
        };
    }

    let content = match fs::read_to_string(&absolute_path) {
        Ok(c) => c,
        Err(e) => {
            return FormatterResult {
                error: FormatterError::ReadFailed,
                error_message: format!("Error reading file: {}", e),
            };
        }
    };

    let processed = process_content(&content);

    match fs::write(&absolute_path, &processed) {
        Ok(_) => FormatterResult {
            error: FormatterError::Success,
            error_message: String::new(),
        },
        Err(e) => FormatterResult {
            error: FormatterError::ReadFailed,
            error_message: format!("Error writing file: {}", e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_content_trailing_spaces() {
        let input = "hello   \nworld\t\n";
        let output = process_content(input);
        assert_eq!(output, "hello\nworld\n");
    }

    #[test]
    fn test_process_content_leading_tab_to_spaces() {
        let input = "\tfoo\n\t\tbar\n";
        let output = process_content(input);
        assert_eq!(output, "    foo\n    bar\n");
    }

    #[test]
    fn test_process_content_tabs_and_spaces_mixed_leading() {
        let input = "\t \t hello\n";
        let output = process_content(input);
        assert_eq!(output, "    hello\n");
    }

    #[test]
    fn test_process_content_mixed_whitespace() {
        let input = "  \tfoo\n  bar\n";
        let output = process_content(input);
        assert_eq!(output, "    foo\nbar\n");
    }

    #[test]
    fn test_process_content_spaces_only_leading_removed() {
        let input = "  hello\n    world\n";
        let output = process_content(input);
        assert_eq!(output, "hello\nworld\n");
    }

    #[test]
    fn test_process_content_no_trailing_newline() {
        let input = "hello\nworld";
        let output = process_content(input);
        assert_eq!(output, "hello\nworld");
    }

    #[test]
    fn test_process_content_empty() {
        assert_eq!(process_content(""), "");
    }

    #[test]
    fn test_process_content_empty_lines_preserved() {
        let input = "foo\n\nbar\n";
        let output = process_content(input);
        assert_eq!(output, "foo\n\nbar\n");
    }

    #[test]
    fn test_process_content_no_change_needed() {
        let input = "hello\nworld\n";
        let output = process_content(input);
        assert_eq!(output, "hello\nworld\n");
    }

    #[test]
    fn test_process_content_only_whitespace_line() {
        let input = "   \n\t\n";
        let output = process_content(input);
        assert_eq!(output, "\n\n");
    }

    #[test]
    fn test_format_file_null_path() {
        let result = format_file("");
        assert_eq!(result.error, FormatterError::NullPath);
        assert_eq!(result.error_message, "Path parameter is required");
    }

    #[test]
    fn test_format_file_nonexistent() {
        let result = format_file("/nonexistent_path_12345/file.txt");
        assert_eq!(result.error, FormatterError::ReadFailed);
        assert!(result.error_message.contains("Error reading file"));
    }

    #[test]
    fn test_format_file_success() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_format_success.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "  hello   ").unwrap();
        let result = format_file(file_path.to_str().unwrap());
        assert_eq!(result.error, FormatterError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello\n");
        let _ = std::fs::remove_file(&file_path);
    }
}
