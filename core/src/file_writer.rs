use crate::file_reader::cache_invalidate;
use crate::ignore_rules::check_ignore_path;
use crate::pathutils::resolve_workspace_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWriterError {
    Success,
    NullPath,
    FileNotFound,
    PermissionDenied,
    WriteFailed,
}

#[derive(Debug, Clone)]
pub struct FileWriterResult {
    pub error: FileWriterError,
    pub error_message: String,
}

pub fn write_file_content(path: &str, content: &str) -> FileWriterResult {
    if path.is_empty() {
        return FileWriterResult {
            error: FileWriterError::NullPath,
            error_message: "Path parameter is required".to_string(),
        };
    }

    if check_ignore_path(path) {
        return FileWriterResult {
            error: FileWriterError::PermissionDenied,
            error_message: "Access denied by .crownignore rules".to_string(),
        };
    }

    let absolute_path = resolve_workspace_path(path);
    if absolute_path.is_empty() {
        return FileWriterResult {
            error: FileWriterError::FileNotFound,
            error_message: "Could not resolve path".to_string(),
        };
    }

    if let Err(e) = std::fs::write(&absolute_path, content) {
        return FileWriterResult {
            error: FileWriterError::WriteFailed,
            error_message: format!("Error writing file: {}", e),
        };
    }

    cache_invalidate(&absolute_path);

    FileWriterResult {
        error: FileWriterError::Success,
        error_message: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;

    fn get_read_count(absolute_path: &str) -> u32 {
        crate::file_reader::cache_get(absolute_path)
            .map(|e| e.read_count)
            .unwrap_or(0)
    }

    #[test]
    fn test_null_path() {
        let result = write_file_content("", "content");
        assert_eq!(result.error, FileWriterError::NullPath);
        assert_eq!(result.error_message, "Path parameter is required");
    }

    #[test]
    fn test_file_not_found() {
        let result = write_file_content("/nonexistent_dir/file.txt", "content");
        assert_eq!(result.error, FileWriterError::WriteFailed);
    }

    #[test]
    fn test_write_success() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_write_success.txt");
        let result = write_file_content(file_path.to_str().unwrap(), "hello world");
        assert_eq!(result.error, FileWriterError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_write_empty_content() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_write_empty.txt");
        let result = write_file_content(file_path.to_str().unwrap(), "");
        assert_eq!(result.error, FileWriterError::Success);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "");
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_cache_invalidation_after_write() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_cache_inval_write.txt");
        let abs_path = resolve_workspace_path(file_path.to_str().unwrap());

        crate::file_reader::cache_set(&abs_path, std::time::SystemTime::UNIX_EPOCH, 5);
        assert!(get_read_count(&abs_path) > 0);

        let result = write_file_content(file_path.to_str().unwrap(), "modified");
        assert_eq!(result.error, FileWriterError::Success);

        assert_eq!(get_read_count(&abs_path), 0);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_repeated_writes_invalidate_cache_each_time() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_repeat_write.txt");
        let abs_path = resolve_workspace_path(file_path.to_str().unwrap());

        assert_eq!(
            write_file_content(file_path.to_str().unwrap(), "v1\n").error,
            FileWriterError::Success
        );

        crate::file_reader::cache_set(&abs_path, std::time::SystemTime::UNIX_EPOCH, 3);
        assert!(get_read_count(&abs_path) > 0);

        assert_eq!(
            write_file_content(file_path.to_str().unwrap(), "v2\n").error,
            FileWriterError::Success
        );
        assert_eq!(get_read_count(&abs_path), 0);

        crate::file_reader::cache_set(&abs_path, std::time::SystemTime::UNIX_EPOCH, 7);
        assert!(get_read_count(&abs_path) > 0);

        assert_eq!(
            write_file_content(file_path.to_str().unwrap(), "v3\n").error,
            FileWriterError::Success
        );
        assert_eq!(get_read_count(&abs_path), 0);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    #[serial]
    fn test_write_file_denied_by_crownignore() {
        crate::ignore_rules::reset_ignore_rules();
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = std::env::temp_dir().join("crown_test_writer_crownignore");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_current_dir(&temp_dir).unwrap();
        let ignore_path = temp_dir.join(".crownignore");
        let mut f = std::fs::File::create(&ignore_path).unwrap();
        writeln!(f, "secret.txt").unwrap();
        f.flush().unwrap();
        drop(f);
        crate::ignore_rules::reset_ignore_rules();
        let result = write_file_content("secret.txt", "content");
        assert_eq!(result.error, FileWriterError::PermissionDenied);
        assert_eq!(result.error_message, "Access denied by .crownignore rules");
        let _ = std::fs::remove_file(&ignore_path);
        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
