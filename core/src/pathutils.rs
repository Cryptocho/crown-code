use std::path::{Path, PathBuf};

pub const MAX_PATH_LENGTH: usize = 4096;

pub fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

pub fn resolve_workspace_path(relative_path: &str) -> String {
    if relative_path.is_empty() {
        return String::new();
    }
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return normalize_slashes(relative_path);
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let joined = cwd.join(relative_path);
    normalize_slashes(joined.to_str().unwrap_or(relative_path))
}

pub fn to_rel_path(path: &str, cwd: Option<&str>) -> String {
    let base_dir = match cwd {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::current_dir().unwrap_or_default(),
    };
    let p = Path::new(path);
    if let Ok(rest) = p.strip_prefix(&base_dir) {
        let rest_str = rest.to_str().unwrap_or(path);
        let trimmed = rest_str.trim_start_matches('/').trim_start_matches('\\');
        normalize_slashes(trimmed)
    } else {
        normalize_slashes(path)
    }
}

pub fn resolve_path(relative_path: &str) -> (String, String) {
    let abs_path = resolve_workspace_path(relative_path);
    (abs_path.clone(), abs_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_slashes_forward_unchanged() {
        assert_eq!(normalize_slashes("/a/b/c"), "/a/b/c");
    }

    #[test]
    fn test_normalize_slashes_backslash_replaced() {
        assert_eq!(normalize_slashes(r"\a\b\c"), "/a/b/c");
    }

    #[test]
    fn test_normalize_slashes_mixed() {
        assert_eq!(normalize_slashes(r"a\b/c\d"), "a/b/c/d");
    }

    #[test]
    fn test_normalize_slashes_empty() {
        assert_eq!(normalize_slashes(""), "");
    }

    #[test]
    fn test_resolve_workspace_path_absolute() {
        let result = resolve_workspace_path("/absolute/path");
        assert_eq!(result, "/absolute/path");
    }

    #[test]
    fn test_resolve_workspace_path_relative() {
        let result = resolve_workspace_path("relative/path");
        assert!(!result.is_empty());
        assert!(result.contains("relative/path"));
    }

    #[test]
    fn test_resolve_workspace_path_empty() {
        assert_eq!(resolve_workspace_path(""), "");
    }

    #[test]
    fn test_to_rel_path_strips_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let cwd_str = cwd.to_str().unwrap();
        let full = format!("{}/some/file.txt", cwd_str);
        let rel = to_rel_path(&full, None);
        assert_eq!(rel, "some/file.txt");
    }

    #[test]
    fn test_to_rel_path_outside_cwd() {
        assert_eq!(to_rel_path("/some/other/path", None), "/some/other/path");
    }

    #[test]
    fn test_to_rel_path_exact_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let cwd_str = cwd.to_str().unwrap();
        assert_eq!(to_rel_path(cwd_str, None), "");
    }

    #[test]
    fn test_to_rel_path_custom_cwd() {
        let rel = to_rel_path("/custom/dir/file.txt", Some("/custom/dir"));
        assert_eq!(rel, "file.txt");
    }

    #[test]
    fn test_resolve_path() {
        let (abs, display) = resolve_path("some/path");
        assert_eq!(abs, display);
        assert!(!abs.is_empty());
    }

    #[test]
    fn test_resolve_path_absolute() {
        let (abs, display) = resolve_path("/usr/bin");
        assert_eq!(abs, "/usr/bin");
        assert_eq!(display, "/usr/bin");
    }

    #[test]
    fn test_resolve_path_relative() {
        let cwd = std::env::current_dir().unwrap();
        let cwd_str = cwd.to_str().unwrap();
        let (abs, _display) = resolve_path("foo/bar");
        assert!(abs.starts_with(cwd_str));
        assert!(abs.ends_with("foo/bar"));
    }
}
