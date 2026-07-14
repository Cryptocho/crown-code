use crate::ignore_rules::check_ignore_path;
use crate::pathutils::resolve_workspace_path;

pub const MAX_LIST_ENTRIES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFilesError {
    Success,
    NullPath,
    DirNotFound,
    PermissionDenied,
    ReadFailed,
}

#[derive(Debug, Clone)]
pub struct ListFilesResult {
    pub entries: Vec<String>,
    pub count: usize,
    pub did_hit_limit: bool,
    pub error: ListFilesError,
    pub error_message: String,
}

pub fn list_files(path: &str) -> ListFilesResult {
    if path.is_empty() {
        return ListFilesResult {
            entries: Vec::new(),
            count: 0,
            did_hit_limit: false,
            error: ListFilesError::NullPath,
            error_message: "Path is empty".to_string(),
        };
    }

    if check_ignore_path(path) {
        return ListFilesResult {
            entries: Vec::new(),
            count: 0,
            did_hit_limit: false,
            error: ListFilesError::PermissionDenied,
            error_message: "Access denied by .crownignore rules".to_string(),
        };
    }

    let abs_path = resolve_workspace_path(path);
    if abs_path.is_empty() {
        return ListFilesResult {
            entries: Vec::new(),
            count: 0,
            did_hit_limit: false,
            error: ListFilesError::DirNotFound,
            error_message: "Path not found".to_string(),
        };
    }

    let home_dir = std::env::home_dir()
        .map(|h| h.to_str().unwrap_or("").to_string())
        .unwrap_or_default();
    if abs_path == "/" || abs_path == home_dir {
        return ListFilesResult {
            entries: Vec::new(),
            count: 0,
            did_hit_limit: false,
            error: ListFilesError::Success,
            error_message: String::new(),
        };
    }

    let dir_path = std::path::Path::new(&abs_path);
    if !dir_path.exists() || !dir_path.is_dir() {
        return ListFilesResult {
            entries: Vec::new(),
            count: 0,
            did_hit_limit: false,
            error: ListFilesError::DirNotFound,
            error_message: format!("Directory not found: {}", abs_path),
        };
    }

    let mut entries: Vec<(String, bool)> = Vec::new();
    let mut did_hit_limit = false;

    match std::fs::read_dir(dir_path) {
        Ok(read_dir) => {
            for entry_result in read_dir {
                match entry_result {
                    Ok(entry) => {
                        let name = entry.file_name().to_str().unwrap_or("").to_string();
                        if name == "." || name == ".." {
                            continue;
                        }

                        let entry_abs_path = entry.path().to_str().unwrap_or("").to_string();
                        if check_ignore_path(&entry_abs_path) {
                            continue;
                        }

                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        entries.push((name, is_dir));

                        if entries.len() > MAX_LIST_ENTRIES {
                            did_hit_limit = true;
                            entries.truncate(MAX_LIST_ENTRIES);
                            break;
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
        Err(e) => {
            return ListFilesResult {
                entries: Vec::new(),
                count: 0,
                did_hit_limit: false,
                error: ListFilesError::ReadFailed,
                error_message: format!("Failed to read directory: {}", e),
            };
        }
    }

    entries.sort_by(|a, b| {
        if a.1 && !b.1 {
            std::cmp::Ordering::Less
        } else if !a.1 && b.1 {
            std::cmp::Ordering::Greater
        } else {
            a.0.cmp(&b.0)
        }
    });

    let result_entries: Vec<String> = entries.into_iter().map(|e| e.0).collect();
    let count = result_entries.len();

    ListFilesResult {
        entries: result_entries,
        count,
        did_hit_limit,
        error: ListFilesError::Success,
        error_message: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_files_null_path() {
        let result = list_files("");
        assert_eq!(result.error, ListFilesError::NullPath);
        assert_eq!(result.error_message, "Path is empty");
    }

    #[test]
    fn test_list_files_nonexistent_path() {
        let result = list_files("/nonexistent_path_xyz_12345");
        assert_eq!(result.error, ListFilesError::DirNotFound);
    }

    #[test]
    fn test_list_files_root() {
        let result = list_files("/");
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_list_files_home() {
        let result = list_files(&std::env::home_dir().unwrap().to_str().unwrap().to_string());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_list_files_empty_directory() {
        let dir = std::env::temp_dir().join("test_list_empty_dir");
        let _ = std::fs::create_dir_all(&dir);
        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 0);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_list_files_mixed() {
        let dir = std::env::temp_dir().join("test_list_mixed");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("b_file.txt"), "b");
        let _ = std::fs::create_dir(dir.join("a_dir"));
        let _ = std::fs::write(dir.join("c_file.txt"), "c");
        let _ = std::fs::create_dir(dir.join("d_dir"));

        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 4);
        assert!(result.entries[0].ends_with("dir") || result.entries[0].ends_with("dir"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_files_directory_first_sort() {
        let dir = std::env::temp_dir().join("test_list_sort");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("z_file.txt"), "z");
        let _ = std::fs::create_dir(dir.join("a_dir"));
        let _ = std::fs::write(dir.join("m_file.txt"), "m");
        let _ = std::fs::create_dir(dir.join("b_dir"));

        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 4);
        assert_eq!(result.entries[0], "a_dir");
        assert_eq!(result.entries[1], "b_dir");
        assert_eq!(result.entries[2], "m_file.txt");
        assert_eq!(result.entries[3], "z_file.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_files_alphabetical_order() {
        let dir = std::env::temp_dir().join("test_list_alpha");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("delta.txt"), "d");
        let _ = std::fs::write(dir.join("alpha.txt"), "a");
        let _ = std::fs::write(dir.join("beta.txt"), "b");
        let _ = std::fs::write(dir.join("gamma.txt"), "g");

        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 4);
        assert_eq!(result.entries[0], "alpha.txt");
        assert_eq!(result.entries[1], "beta.txt");
        assert_eq!(result.entries[2], "delta.txt");
        assert_eq!(result.entries[3], "gamma.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_files_limit() {
        let dir = std::env::temp_dir().join("test_list_limit");
        let _ = std::fs::create_dir_all(&dir);
        for i in 0..MAX_LIST_ENTRIES + 50 {
            let _ = std::fs::write(dir.join(format!("file_{}.txt", i)), "data");
        }

        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert!(result.count <= MAX_LIST_ENTRIES);
        assert!(result.did_hit_limit);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_files_single_entry() {
        let dir = std::env::temp_dir().join("test_list_single");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("single.txt"), "content");
        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 1);
        assert_eq!(result.entries[0], "single.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_files_hidden_files() {
        let dir = std::env::temp_dir().join("test_list_hidden");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(".hidden"), "content");
        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 1);
        assert_eq!(result.entries[0], ".hidden");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_files_dir_entry_no_trailing_slash() {
        let dir = std::env::temp_dir().join("test_list_notrailing");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("subdir"));
        let result = list_files(dir.to_str().unwrap());
        assert_eq!(result.error, ListFilesError::Success);
        assert_eq!(result.count, 1);
        assert!(!result.entries[0].ends_with('/'));
        assert_eq!(result.entries[0], "subdir");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
