use serde_json::Value;

use crate::api::types::Tool;

pub fn get_tool_definitions() -> Vec<Tool> {
    vec![
        Tool {
            name: "read_file".to_string(),
            description: "Read the contents of a file at the specified path. Returns content with line numbers.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to workspace root"},
                    "start_line": {"type": "integer", "description": "1-based start line (default: 1)"},
                    "end_line": {"type": "integer", "description": "1-based end line (default: start_line + 1000)"}
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "write_to_file".to_string(),
            description: "Write content to a file at the specified path. Creates the file if it doesn't exist.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to workspace root"},
                    "content": {"type": "string", "description": "Content to write to the file"}
                },
                "required": ["path", "content"]
            }),
        },
        Tool {
            name: "replace_in_file".to_string(),
            description: "Replace exact string matches in a file. Used for editing existing files.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to workspace root"},
                    "old_string": {"type": "string", "description": "The exact line to match and replace"},
                    "new_string": {"type": "string", "description": "The replacement line content"}
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        Tool {
            name: "execute_command".to_string(),
            description: "Execute a shell command on the system. Returns stdout, stderr, and exit code.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"}
                },
                "required": ["command"]
            }),
        },
        Tool {
            name: "search_files".to_string(),
            description: "Search for regex pattern matches in files within a directory. Supports glob file pattern filtering.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "directory": {"type": "string", "description": "Directory to search in"},
                    "regex": {"type": "string", "description": "Regular expression pattern to search for"},
                    "file_pattern": {"type": "string", "description": "Optional glob pattern to filter files (e.g. *.rs)"}
                },
                "required": ["directory", "regex"]
            }),
        },
        Tool {
            name: "list_files".to_string(),
            description: "List files and directories at the specified path.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path to list"}
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "attempt_completion".to_string(),
            description: "Signal that the task is complete. Provide a result summary for the user.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string", "description": "Summary of what was accomplished"}
                },
                "required": ["result"]
            }),
        },
    ]
}

pub async fn execute_tool(name: &str, args: &Value) -> String {
    match name {
        "read_file" => execute_read_file(args),
        "write_to_file" => execute_write_to_file(args),
        "replace_in_file" => execute_replace_in_file(args),
        "execute_command" => execute_execute_command(args).await,
        "search_files" => execute_search_files(args),
        "list_files" => execute_list_files(args),
        "attempt_completion" => execute_attempt_completion(args),
        _ => format!("Error: Unknown tool: {}", name),
    }
}

fn execute_read_file(args: &Value) -> String {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        return "Error: path parameter is required".to_string();
    }
    let start_line = args.get("start_line").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let end_line = args.get("end_line").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let res = crate::file_reader::read_file_range(path, start_line, end_line);
    if res.error != crate::file_reader::FileReaderError::Success {
        return format!("Error: {}", res.error_message);
    }
    res.content
}

fn execute_write_to_file(args: &Value) -> String {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        return "Error: path parameter is required".to_string();
    }
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let res = crate::file_writer::write_file_content(path, content);
    if res.error != crate::file_writer::FileWriterError::Success {
        return format!("Error: {}", res.error_message);
    }
    "File written successfully.".to_string()
}

fn execute_replace_in_file(args: &Value) -> String {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        return "Error: path parameter is required".to_string();
    }
    let old_str = args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
    if old_str.is_empty() {
        return "Error: old_string parameter is required".to_string();
    }
    let new_str = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
    let res = crate::file_edit::edit_file(path, old_str, new_str, false);
    if res.error != crate::file_edit::FileEditError::Success {
        return format!("Error: {}", res.error_message);
    }
    format!("File updated. {} match(es) replaced.", res.match_count)
}

async fn execute_execute_command(args: &Value) -> String {
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
    if command.is_empty() {
        return "Error: command parameter is required".to_string();
    }
    let res = crate::command_exec::exec_command(command, &[]).await;
    let mut output = String::new();
    if !res.stdout.is_empty() {
        output.push_str("STDOUT:\n");
        output.push_str(&res.stdout);
        output.push('\n');
    }
    if !res.stderr.is_empty() {
        output.push_str("STDERR:\n");
        output.push_str(&res.stderr);
        output.push('\n');
    }
    output.push_str(&format!("Exit code: {}", res.exit_code));
    if res.abnormal_exit {
        output.push_str(" (abnormal exit)");
    }
    if res.execution_time > 0.0 {
        output.push_str(&format!("\nExecution time: {:.2}s", res.execution_time));
    }
    output
}

fn execute_search_files(args: &Value) -> String {
    let directory = args.get("directory").and_then(|v| v.as_str()).unwrap_or("");
    if directory.is_empty() {
        return "Error: directory parameter is required".to_string();
    }
    let regex = args.get("regex").and_then(|v| v.as_str()).unwrap_or("");
    if regex.is_empty() {
        return "Error: regex parameter is required".to_string();
    }
    let file_pattern = args.get("file_pattern").and_then(|v| v.as_str()).unwrap_or("*");
    let res = crate::search_files::search_files(directory, regex, file_pattern);
    if res.error != crate::search_files::SearchFilesError::Success {
        return format!("Error: {}", res.error_message);
    }
    if res.match_count == 0 {
        return "No matches found.".to_string();
    }
    format!("{}\n({} matches found)", res.results, res.match_count)
}

fn execute_list_files(args: &Value) -> String {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        return "Error: path parameter is required".to_string();
    }
    let res = crate::list_files::list_files(path);
    if res.error != crate::list_files::ListFilesError::Success {
        return format!("Error: {}", res.error_message);
    }
    if res.count == 0 {
        return "(empty directory)".to_string();
    }
    let lines: Vec<String> = res.entries.iter().map(|e| format!("  {}", e)).collect();
    let mut result = lines.join("\n");
    result.push_str(&format!("\n\n{} entries", res.count));
    if res.did_hit_limit {
        result.push_str(&format!(" (list truncated at {})", crate::list_files::MAX_LIST_ENTRIES));
    }
    result
}

fn execute_attempt_completion(args: &Value) -> String {
    let result = args.get("result").and_then(|v| v.as_str()).unwrap_or("");
    format!("[COMPLETION]{}", result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tool_definitions_returns_seven() {
        let tools = get_tool_definitions();
        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn test_tool_names() {
        let tools = get_tool_definitions();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_to_file"));
        assert!(names.contains(&"replace_in_file"));
        assert!(names.contains(&"execute_command"));
        assert!(names.contains(&"search_files"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"attempt_completion"));
    }

    #[test]
    fn test_all_tools_have_descriptions() {
        let tools = get_tool_definitions();
        for tool in &tools {
            assert!(!tool.description.is_empty(), "Tool {} has no description", tool.name);
        }
    }

    #[test]
    fn test_all_tools_have_parameters() {
        let tools = get_tool_definitions();
        for tool in &tools {
            assert_eq!(tool.parameters["type"].as_str(), Some("object"));
        }
    }

    #[test]
    fn test_tool_parameter_structure() {
        let tools = get_tool_definitions();
        for tool in &tools {
            let props = tool.parameters.get("properties");
            assert!(props.is_some(), "Tool {} has no properties", tool.name);
            let required = tool.parameters.get("required");
            assert!(required.is_some(), "Tool {} has no required", tool.name);
        }
    }

    #[test]
    fn test_read_file_requires_path() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "read_file").unwrap();
        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("path")));
    }

    #[test]
    fn test_write_to_file_requires_path_and_content() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "write_to_file").unwrap();
        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("path")));
        assert!(required.iter().any(|r| r.as_str() == Some("content")));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let args = serde_json::json!({});
        let result = execute_tool("nonexistent_tool", &args).await;
        assert_eq!(result, "Error: Unknown tool: nonexistent_tool");
    }

    #[tokio::test]
    async fn test_read_file_empty_path() {
        let args = serde_json::json!({"path": ""});
        let result = execute_tool("read_file", &args).await;
        assert_eq!(result, "Error: path parameter is required");
    }

    #[tokio::test]
    async fn test_write_to_file_empty_path() {
        let args = serde_json::json!({"path": "", "content": "test"});
        let result = execute_tool("write_to_file", &args).await;
        assert_eq!(result, "Error: path parameter is required");
    }

    #[tokio::test]
    async fn test_replace_in_file_empty_path() {
        let args = serde_json::json!({"path": "", "old_string": "a", "new_string": "b"});
        let result = execute_tool("replace_in_file", &args).await;
        assert_eq!(result, "Error: path parameter is required");
    }

    #[tokio::test]
    async fn test_replace_in_file_empty_old_string() {
        let args = serde_json::json!({"path": "/tmp/test.txt", "old_string": "", "new_string": "b"});
        let result = execute_tool("replace_in_file", &args).await;
        assert_eq!(result, "Error: old_string parameter is required");
    }

    #[tokio::test]
    async fn test_execute_command_empty_command() {
        let args = serde_json::json!({"command": ""});
        let result = execute_tool("execute_command", &args).await;
        assert_eq!(result, "Error: command parameter is required");
    }

    #[tokio::test]
    async fn test_search_files_empty_directory() {
        let args = serde_json::json!({"directory": "", "regex": "pattern"});
        let result = execute_tool("search_files", &args).await;
        assert_eq!(result, "Error: directory parameter is required");
    }

    #[tokio::test]
    async fn test_search_files_empty_regex() {
        let args = serde_json::json!({"directory": "/tmp", "regex": ""});
        let result = execute_tool("search_files", &args).await;
        assert_eq!(result, "Error: regex parameter is required");
    }

    #[tokio::test]
    async fn test_list_files_empty_path() {
        let args = serde_json::json!({"path": ""});
        let result = execute_tool("list_files", &args).await;
        assert_eq!(result, "Error: path parameter is required");
    }

    #[tokio::test]
    async fn test_execute_command_echo() {
        let args = serde_json::json!({"command": "echo hello"});
        let result = execute_tool("execute_command", &args).await;
        assert!(result.contains("STDOUT:"));
        assert!(result.contains("hello"));
        assert!(result.contains("Exit code: 0"));
    }

    #[tokio::test]
    async fn test_execute_command_stderr() {
        let args = serde_json::json!({"command": "echo stderr test >&2"});
        let result = execute_tool("execute_command", &args).await;
        assert!(result.contains("STDERR:"));
        assert!(result.contains("stderr test"));
        assert!(result.contains("Exit code: 0"));
    }

    #[tokio::test]
    async fn test_execute_command_exit_code() {
        let args = serde_json::json!({"command": "false"});
        let result = execute_tool("execute_command", &args).await;
        assert!(result.contains("Exit code: 1"));
    }

    #[tokio::test]
    async fn test_attempt_completion_mark() {
        let args = serde_json::json!({"result": "Task done"});
        let result = execute_tool("attempt_completion", &args).await;
        assert_eq!(result, "[COMPLETION]Task done");
    }

    #[tokio::test]
    async fn test_attempt_completion_empty_result() {
        let args = serde_json::json!({"result": ""});
        let result = execute_tool("attempt_completion", &args).await;
        assert_eq!(result, "[COMPLETION]");
    }

    #[test]
    fn test_replace_in_file_requires_path_old_string_new_string() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "replace_in_file").unwrap();
        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("path")));
        assert!(required.iter().any(|r| r.as_str() == Some("old_string")));
        assert!(required.iter().any(|r| r.as_str() == Some("new_string")));
    }

    #[test]
    fn test_execute_command_requires_command() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "execute_command").unwrap();
        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("command")));
    }

    #[test]
    fn test_search_files_requires_directory_regex() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "search_files").unwrap();
        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("directory")));
        assert!(required.iter().any(|r| r.as_str() == Some("regex")));
    }

    #[test]
    fn test_list_files_requires_path() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "list_files").unwrap();
        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("path")));
    }

    #[test]
    fn test_attempt_completion_requires_result() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "attempt_completion").unwrap();
        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("result")));
    }

    #[test]
    fn test_read_file_has_start_end_line_properties() {
        let tools = get_tool_definitions();
        let tool = tools.iter().find(|t| t.name == "read_file").unwrap();
        let props = tool.parameters["properties"].as_object().unwrap();
        assert!(props.contains_key("start_line"));
        assert!(props.contains_key("end_line"));
        assert!(props.contains_key("path"));
    }

    #[tokio::test]
    async fn test_read_file_integration() {
        let dir = std::env::temp_dir().join("crown_test_read_file");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_read.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        let args = serde_json::json!({"path": path_str});
        let result = execute_tool("read_file", &args).await;
        assert!(result.contains("1 | line1"));
        assert!(result.contains("2 | line2"));
        assert!(result.contains("3 | line3"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_write_to_file_integration() {
        let dir = std::env::temp_dir().join("crown_test_write_file");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_write.txt");
        let path_str = file_path.to_string_lossy().to_string();
        let args = serde_json::json!({"path": path_str, "content": "hello world"});
        let result = execute_tool("write_to_file", &args).await;
        assert_eq!(result, "File written successfully.");
        let content = std::fs::read_to_string(&file_path).unwrap_or_default();
        assert_eq!(content, "hello world");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_list_files_integration() {
        let dir = std::env::temp_dir().join("crown_test_list_files");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let args = serde_json::json!({"path": dir_str});
        let result = execute_tool("list_files", &args).await;
        assert!(result.contains("a.txt"));
        assert!(result.contains("b.txt"));
        assert!(result.contains("2 entries"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_search_files_finds_pattern() {
        let dir = std::env::temp_dir().join("crown_test_search_files");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("test.nim"), "proc hello =\n  echo \"hello\"\n").unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let args = serde_json::json!({"directory": dir_str, "regex": "hello", "file_pattern": "*"});
        let result = execute_tool("search_files", &args).await;
        assert!(result.contains("hello"));
        assert!(result.contains("matches found"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_search_files_no_matches() {
        let dir = std::env::temp_dir().join("crown_test_search_nomatch");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("test.txt"), "hello world").unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let args = serde_json::json!({"directory": dir_str, "regex": "zzz_not_found", "file_pattern": "*"});
        let result = execute_tool("search_files", &args).await;
        assert_eq!(result, "No matches found.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_replace_in_file_edits_content() {
        let dir = std::env::temp_dir().join("crown_test_replace_file");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_replace.txt");
        std::fs::write(&file_path, "old line\nkeep line\n").unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        let args = serde_json::json!({"path": path_str, "old_string": "old line", "new_string": "new line"});
        let result = execute_tool("replace_in_file", &args).await;
        assert!(result.contains("File updated"));
        assert!(result.contains("match(es) replaced"));
        let content = std::fs::read_to_string(&file_path).unwrap_or_default();
        assert!(content.contains("new line"));
        assert!(content.contains("keep line"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_replace_in_file_nonexistent_file() {
        let args = serde_json::json!({"path": "/tmp/crown_test_nonexistent_file_xyz.txt", "old_string": "a", "new_string": "b"});
        let result = execute_tool("replace_in_file", &args).await;
        assert!(result.contains("Error:"));
    }

    #[tokio::test]
    async fn test_replace_in_file_non_matching_old_string() {
        let dir = std::env::temp_dir().join("crown_test_replace_nomatch");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_nomatch.txt");
        std::fs::write(&file_path, "existing content\n").unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        let args = serde_json::json!({"path": path_str, "old_string": "nonexistent line", "new_string": "new"});
        let result = execute_tool("replace_in_file", &args).await;
        assert!(result.contains("Error:"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}