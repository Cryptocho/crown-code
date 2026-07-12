use std::sync::Mutex;
use std::sync::OnceLock;

use crate::glob::fnmatch_pathname;
use crate::pathutils::to_rel_path;

struct IgnoreRulesInner {
    global_patterns: Vec<String>,
    project_patterns: Vec<String>,
    initialized: bool,
}

static IGNORE_STATE: OnceLock<Mutex<IgnoreRulesInner>> = OnceLock::new();

fn state() -> &'static Mutex<IgnoreRulesInner> {
    IGNORE_STATE.get_or_init(|| {
        Mutex::new(IgnoreRulesInner {
            global_patterns: Vec::new(),
            project_patterns: Vec::new(),
            initialized: false,
        })
    })
}

pub fn load_ignore_file(path: &str) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut patterns = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_end_matches([' ', '\t', '\r', '\n']);
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        patterns.push(trimmed.to_string());
    }
    patterns
}

fn init_ignore_rules() {
    let mut lock = state().lock().unwrap();
    if lock.initialized {
        return;
    }

    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        let global_path = format!("{}/.crownignore", home);
        lock.global_patterns = load_ignore_file(&global_path);
    }

    lock.project_patterns = load_ignore_file(".crownignore");
    lock.initialized = true;
}

pub fn reset_ignore_rules() {
    let mut lock = state().lock().unwrap();
    lock.global_patterns.clear();
    lock.project_patterns.clear();
    lock.initialized = false;
}

fn match_ignore_pattern(pattern: &str, rel_path: &str) -> bool {
    let negate = pattern.starts_with('!');
    let pat = if negate { &pattern[1..] } else { pattern };

    let mut matched = fnmatch_pathname(pat, rel_path);

    if !matched && !pat.contains('/') {
        matched = fnmatch_pathname(&format!("*/{}", pat), rel_path);
    }

    if negate { !matched } else { matched }
}

pub fn check_ignore_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }

    init_ignore_rules();

    let lock = state().lock().unwrap();
    if lock.global_patterns.is_empty() && lock.project_patterns.is_empty() {
        return false;
    }

    let rel_path = to_rel_path(path, None);

    for pattern in &lock.global_patterns {
        if match_ignore_pattern(pattern, &rel_path) {
            return true;
        }
    }

    for pattern in &lock.project_patterns {
        if match_ignore_pattern(pattern, &rel_path) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_ignore_file_nonexistent() {
        let patterns = load_ignore_file("/nonexistent/path/.crownignore");
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_match_ignore_pattern_simple_glob() {
        assert!(match_ignore_pattern("*.txt", "foo.txt"));
        assert!(!match_ignore_pattern("*.txt", "foo.rs"));
    }

    #[test]
    fn test_match_ignore_pattern_star_slash_prefix() {
        assert!(match_ignore_pattern("*.txt", "sub/foo.txt"));
        assert!(match_ignore_pattern("node_modules", "project/node_modules"));
    }

    #[test]
    fn test_match_ignore_pattern_exact_path() {
        assert!(match_ignore_pattern("secret/file.txt", "secret/file.txt"));
        assert!(!match_ignore_pattern("secret/file.txt", "other/file.txt"));
    }

    #[test]
    fn test_match_ignore_pattern_negation() {
        assert!(!match_ignore_pattern("!*.txt", "foo.txt"));
        assert!(match_ignore_pattern("!*.txt", "foo.rs"));
    }

    #[test]
    fn test_match_ignore_pattern_negation_with_all_star() {
        assert!(!match_ignore_pattern("!*", "anything"));
    }

    #[test]
    fn test_check_ignore_path_empty() {
        assert!(!check_ignore_path(""));
    }

    #[test]
    fn test_reset_ignore_rules() {
        reset_ignore_rules();
        let lock = state().lock().unwrap();
        assert!(!lock.initialized);
        assert!(lock.global_patterns.is_empty());
        assert!(lock.project_patterns.is_empty());
    }

    #[test]
    fn test_load_ignore_file_only_comments_returns_empty() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_crownignore_only_comments");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "# comment").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "  ").unwrap();
        writeln!(f, "# another").unwrap();
        let patterns = load_ignore_file(file_path.to_str().unwrap());
        assert!(patterns.is_empty());
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_check_ignore_path_no_rules_file_returns_false() {
        reset_ignore_rules();
        assert!(!check_ignore_path("test.nim"));
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        std::env::temp_dir().join(format!("{}_{}_{}", name, pid, line!()))
    }

    #[test]
    fn test_check_ignore_path_with_project_crownignore() {
        reset_ignore_rules();
        use std::io::Write;
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = unique_temp_dir("crown_test_ignore_project");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_current_dir(&temp_dir).unwrap();
        let file_path = temp_dir.join(".crownignore");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "*.nim").unwrap();
        f.flush().unwrap();
        drop(f);
        reset_ignore_rules();
        assert!(check_ignore_path("test.nim"));
        assert!(!check_ignore_path("test.txt"));
        let _ = std::fs::remove_file(&file_path);
        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_check_ignore_path_absolute_converted_to_relative() {
        reset_ignore_rules();
        use std::io::Write;
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = unique_temp_dir("crown_test_ignore_absolute");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_current_dir(&temp_dir).unwrap();
        let file_path = temp_dir.join(".crownignore");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "*.log").unwrap();
        f.flush().unwrap();
        drop(f);
        reset_ignore_rules();
        let absolute_path = format!("{}/test.log", temp_dir.display());
        assert!(check_ignore_path(&absolute_path));
        let absolute_path_nim = format!("{}/test.nim", temp_dir.display());
        assert!(!check_ignore_path(&absolute_path_nim));
        let _ = std::fs::remove_file(&file_path);
        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_match_ignore_pattern_with_slash() {
        assert!(match_ignore_pattern("dir/*.txt", "dir/file.txt"));
        assert!(!match_ignore_pattern("dir/*.txt", "other/file.txt"));
    }

    #[test]
    fn test_load_ignore_file_with_comments_and_blanks() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_crownignore_comments");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "# comment").unwrap();
        writeln!(f, "  ").unwrap();
        writeln!(f, "*.txt").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "*.rs").unwrap();
        let patterns = load_ignore_file(file_path.to_str().unwrap());
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0], "*.txt");
        assert_eq!(patterns[1], "*.rs");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_load_ignore_file_trailing_whitespace() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_crownignore_trailing");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "*.txt   ").unwrap();
        writeln!(f, "*.rs\t").unwrap();
        let patterns = load_ignore_file(file_path.to_str().unwrap());
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0], "*.txt");
        assert_eq!(patterns[1], "*.rs");
        let _ = std::fs::remove_file(&file_path);
    }
}
