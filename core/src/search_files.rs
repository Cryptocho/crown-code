use crate::glob::match_glob;
use crate::ignore_rules::check_ignore_path;
use crate::search::{Search, get_line, new_search};

pub const MAX_SEARCH_DEPTH: usize = 10;
pub const MAX_SEARCH_OUTPUT: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFilesError {
    Success,
    NullParam,
    DirNotFound,
    RegexError,
}

#[derive(Debug, Clone)]
pub struct SearchFilesResult {
    pub results: String,
    pub match_count: usize,
    pub error: SearchFilesError,
    pub error_message: String,
}

fn search_file(file_path: &str, rel_path: &str, search: &Search, result: &mut SearchFilesResult) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    if content.is_empty() {
        return;
    }

    let matches = search.match_all(&content, 0);
    if matches.is_empty() {
        return;
    }

    result.match_count += matches.len();

    for m in &matches {
        let needed = rel_path.len() + m.line.len() + 100;
        if result.results.len() + needed >= MAX_SEARCH_OUTPUT {
            result.results.push_str("\n[Results truncated...]\n");
            return;
        }

        result.results.push_str(&format!("\n{}\n│----\n", rel_path));

        if m.line_number > 1
            && let Some(prev_line) = get_line(&content, m.line_number - 1)
        {
            result.results.push_str(&format!("│{}\n", prev_line));
        }

        result.results.push_str(&format!("│{}\n", m.line));

        if let Some(next_line) = get_line(&content, m.line_number + 1) {
            result.results.push_str(&format!("│{}\n", next_line));
        }

        result.results.push_str("│----\n");
    }
}

fn search_dir(
    dir: &str,
    base_dir: &str,
    file_pattern: &str,
    search: &Search,
    depth: usize,
    result: &mut SearchFilesResult,
) {
    if depth > MAX_SEARCH_DEPTH {
        return;
    }

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    for entry_result in read_dir {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let filename = entry.file_name().to_str().unwrap_or("").to_string();
        if filename == "." || filename == ".." {
            continue;
        }

        let abs_path = entry.path().to_str().unwrap_or("").to_string();

        if !match_glob(&filename, file_pattern) {
            continue;
        }

        if check_ignore_path(&abs_path) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            search_dir(&abs_path, base_dir, file_pattern, search, depth + 1, result);
        } else if file_type.is_file() {
            let rel_path = abs_path
                .strip_prefix(&format!("{}/", base_dir))
                .unwrap_or(&abs_path)
                .to_string();
            search_file(&abs_path, &rel_path, search, result);
        }
    }
}

pub fn search_files(directory: &str, regex: &str, file_pattern: &str) -> SearchFilesResult {
    if directory.is_empty() || regex.is_empty() || file_pattern.is_empty() {
        return SearchFilesResult {
            results: String::new(),
            match_count: 0,
            error: SearchFilesError::NullParam,
            error_message: "parameters cannot be empty".to_string(),
        };
    }

    let dir_path = std::path::Path::new(directory);
    if !dir_path.exists() || !dir_path.is_dir() {
        return SearchFilesResult {
            results: String::new(),
            match_count: 0,
            error: SearchFilesError::DirNotFound,
            error_message: format!("directory not found: {}", directory),
        };
    }

    let search = match new_search(regex, false, false, false) {
        Ok(s) => s,
        Err(e) => {
            return SearchFilesResult {
                results: String::new(),
                match_count: 0,
                error: SearchFilesError::RegexError,
                error_message: format!("invalid regex: {}", e),
            };
        }
    };

    let base_dir = std::path::absolute(dir_path)
        .unwrap_or_else(|_| dir_path.to_path_buf())
        .to_str()
        .unwrap_or(directory)
        .to_string();

    let mut result = SearchFilesResult {
        results: String::new(),
        match_count: 0,
        error: SearchFilesError::Success,
        error_message: String::new(),
    };

    search_dir(&base_dir, &base_dir, file_pattern, &search, 0, &mut result);

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_file(dir: &std::path::Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{}", content).unwrap();
    }

    #[test]
    fn test_search_files_empty_directory() {
        let result = search_files("", "pattern", "*");
        assert_eq!(result.error, SearchFilesError::NullParam);
    }

    #[test]
    fn test_search_files_empty_regex() {
        let result = search_files("/tmp", "", "*");
        assert_eq!(result.error, SearchFilesError::NullParam);
    }

    #[test]
    fn test_search_files_empty_file_pattern() {
        let result = search_files("/tmp", "pattern", "");
        assert_eq!(result.error, SearchFilesError::NullParam);
    }

    #[test]
    fn test_search_files_nonexistent_dir() {
        let result = search_files("/nonexistent_path_xyz_98765", "test", "*");
        assert_eq!(result.error, SearchFilesError::DirNotFound);
    }

    #[test]
    fn test_search_files_invalid_regex() {
        let dir = std::env::temp_dir().join("test_search_invalid_regex");
        let _ = std::fs::create_dir_all(&dir);
        let result = search_files(dir.to_str().unwrap(), "[invalid", "*");
        assert_eq!(result.error, SearchFilesError::RegexError);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_search_files_single_match() {
        let dir = std::env::temp_dir().join("test_search_single_match");
        let _ = std::fs::create_dir_all(&dir);
        create_test_file(&dir, "test.txt", "hello world\nfoo bar\n");
        let result = search_files(dir.to_str().unwrap(), "hello", "*");
        assert_eq!(result.error, SearchFilesError::Success);
        assert_eq!(result.match_count, 1);
        assert!(result.results.contains("test.txt"));
        assert!(result.results.contains("│hello world"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_multiple_matches_single_file() {
        let dir = std::env::temp_dir().join("test_search_multi_match");
        let _ = std::fs::create_dir_all(&dir);
        create_test_file(&dir, "test.txt", "aaa\nbbb\naaa\n");
        let result = search_files(dir.to_str().unwrap(), "aaa", "*");
        assert_eq!(result.error, SearchFilesError::Success);
        assert_eq!(result.match_count, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_glob_filter() {
        let dir = std::env::temp_dir().join("test_search_glob_filter");
        let _ = std::fs::create_dir_all(&dir);
        create_test_file(&dir, "match.txt", "hello\n");
        create_test_file(&dir, "ignore.md", "hello\n");
        let result = search_files(dir.to_str().unwrap(), "hello", "*.txt");
        assert_eq!(result.error, SearchFilesError::Success);
        assert_eq!(result.match_count, 1);
        assert!(result.results.contains("match.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_empty_file() {
        let dir = std::env::temp_dir().join("test_search_empty_file");
        let _ = std::fs::create_dir_all(&dir);
        create_test_file(&dir, "empty.txt", "");
        let result = search_files(dir.to_str().unwrap(), "anything", "*");
        assert_eq!(result.error, SearchFilesError::Success);
        assert_eq!(result.match_count, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_depth_limit() {
        let dir = std::env::temp_dir().join("test_search_depth");
        let _ = std::fs::create_dir_all(&dir);
        let mut current = dir.clone();
        for i in 0..MAX_SEARCH_DEPTH + 2 {
            current = current.join(format!("sub{}", i));
            let _ = std::fs::create_dir_all(&current);
        }
        create_test_file(&current, "deep.txt", "target\n");
        let result = search_files(dir.to_str().unwrap(), "target", "*");
        assert_eq!(result.match_count, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_output_format() {
        let dir = std::env::temp_dir().join("test_search_format");
        let _ = std::fs::create_dir_all(&dir);
        create_test_file(&dir, "file.txt", "line1\nmatch_this\nline3\n");
        let result = search_files(dir.to_str().unwrap(), "match_this", "*");
        assert_eq!(result.error, SearchFilesError::Success);
        assert!(result.results.contains("│----\n"));
        assert!(result.results.contains("│line1\n"));
        assert!(result.results.contains("│match_this\n"));
        assert!(result.results.contains("│line3\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_output_truncation() {
        let dir = std::env::temp_dir().join("test_search_truncate");
        let _ = std::fs::create_dir_all(&dir);
        let mut content = String::with_capacity(MAX_SEARCH_OUTPUT + 50000);
        for i in 0..10000 {
            content.push_str(&format!("match line {} with some padding text\n", i));
        }
        create_test_file(&dir, "big.txt", &content);
        let result = search_files(dir.to_str().unwrap(), "match line", "*");
        assert!(result.results.contains("[Results truncated...]"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_context_lines_before_after() {
        let dir = std::env::temp_dir().join("test_search_context");
        let _ = std::fs::create_dir_all(&dir);
        create_test_file(&dir, "ctx.txt", "before\ntarget\nafter\n");
        let result = search_files(dir.to_str().unwrap(), "target", "*");
        assert_eq!(result.error, SearchFilesError::Success);
        assert!(result.results.contains("│before\n"));
        assert!(result.results.contains("│target\n"));
        assert!(result.results.contains("│after\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_files_first_line_no_before_context() {
        let dir = std::env::temp_dir().join("test_search_first_line");
        let _ = std::fs::create_dir_all(&dir);
        create_test_file(&dir, "first.txt", "target\nnext\n");
        let result = search_files(dir.to_str().unwrap(), "target", "*");
        let lines: Vec<&str> = result.results.lines().collect();
        let target_idx = lines.iter().position(|l| *l == "│target").unwrap();
        let prev = lines[target_idx - 1];
        assert_eq!(prev, "│----");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
