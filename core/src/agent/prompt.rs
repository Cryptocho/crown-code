use crate::shell_detect::detect_shells;

pub fn build_system_prompt(cwd: &str) -> String {
    let shells = detect_shells();
    let default_shell = if !shells.is_empty() && shells[0].found {
        shells[0].name.clone()
    } else {
        "unknown".to_string()
    };
    let os_name = std::env::consts::OS;

    let mut prompt = String::new();
    prompt.push_str(
        "You are Crown Code, a highly skilled software engineer with extensive knowledge ",
    );
    prompt.push_str(
        "in many programming languages, frameworks, design patterns, and best practices.\n\n",
    );

    prompt.push_str("TOOL USE\n");
    prompt
        .push_str("You have access to a set of tools that are executed upon the user's approval. ");
    prompt.push_str(
        "To use a tool, respond with a tool call in the OpenAI function calling format. ",
    );
    prompt
        .push_str("After each tool use, you will receive the result. Continue using tools until ");
    prompt.push_str("the task is complete, then use the attempt_completion tool.\n\n");

    prompt.push_str("AVAILABLE TOOLS\n");
    prompt.push_str("- read_file(path, start_line?, end_line?): Read the contents of a file at the specified path.\n");
    prompt.push_str(
        "- write_to_file(path, content): Write content to a file at the specified path.\n",
    );
    prompt.push_str("- replace_in_file(path, old_string, new_string): Replace exact string matches in a file.\n");
    prompt.push_str("- execute_command(command): Execute a shell command on the system.\n");
    prompt.push_str("- search_files(directory, regex, file_pattern?): Search for regex pattern matches in files.\n");
    prompt.push_str("- list_files(path): List files and directories at the specified path.\n");
    prompt.push_str("- attempt_completion(result): Signal that the task is complete.\n\n");

    prompt.push_str("RULES\n");
    prompt.push_str("- Always use tools to accomplish tasks; do not fabricate results.\n");
    prompt.push_str("- Use read_file to examine files before editing them.\n");
    prompt.push_str("- Prefer replace_in_file for editing existing files.\n");
    prompt.push_str("- Use write_to_file only for creating new files.\n");
    prompt.push_str("- Verify changes after making them.\n\n");

    prompt.push_str("SYSTEM INFORMATION\n");
    prompt.push_str("Operating System: ");
    prompt.push_str(os_name);
    prompt.push('\n');
    prompt.push_str("Default Shell: ");
    prompt.push_str(&default_shell);
    prompt.push('\n');
    prompt.push_str("Current Working Directory: ");
    prompt.push_str(cwd);
    prompt.push('\n');

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_role_description() {
        let prompt = build_system_prompt("/test");
        assert!(prompt.contains("Crown Code"));
        assert!(prompt.contains("software engineer"));
    }

    #[test]
    fn test_contains_cwd() {
        let prompt = build_system_prompt("/test/cwd");
        assert!(prompt.contains("/test/cwd"));
    }

    #[test]
    fn test_contains_os_info() {
        let prompt = build_system_prompt("/test");
        assert!(prompt.contains("Operating System:"));
        let os_name = std::env::consts::OS;
        assert!(prompt.contains(os_name));
    }

    #[test]
    fn test_contains_shell_info() {
        let prompt = build_system_prompt("/test");
        assert!(prompt.contains("Default Shell:"));
    }

    #[test]
    fn test_contains_tool_use_section() {
        let prompt = build_system_prompt("/test");
        assert!(prompt.contains("TOOL USE"));
        assert!(prompt.contains("attempt_completion"));
    }

    #[test]
    fn test_contains_rules_section() {
        let prompt = build_system_prompt("/test");
        assert!(prompt.contains("RULES"));
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("replace_in_file"));
    }

    #[test]
    fn test_empty_cwd() {
        let prompt = build_system_prompt("");
        assert!(prompt.contains("Current Working Directory:"));
    }

    #[test]
    fn test_sections_in_order() {
        let prompt = build_system_prompt("/test");
        let role_pos = prompt.find("software engineer").unwrap();
        let tool_use_pos = prompt.find("TOOL USE").unwrap();
        let tools_pos = prompt.find("AVAILABLE TOOLS").unwrap();
        let rules_pos = prompt.find("RULES").unwrap();
        let sys_info_pos = prompt.find("SYSTEM INFORMATION").unwrap();
        assert!(role_pos < tool_use_pos);
        assert!(tool_use_pos < tools_pos);
        assert!(tools_pos < rules_pos);
        assert!(rules_pos < sys_info_pos);
    }
}
