#[derive(Debug, Clone)]
pub struct ShellInfo {
    pub name: String,
    pub path: String,
    pub found: bool,
}

pub fn detect_shells() -> Vec<ShellInfo> {
    let mut result = Vec::new();

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(shell_env) = std::env::var("SHELL")
            && !shell_env.is_empty()
        {
            let name = std::path::Path::new(&shell_env)
                .file_name()
                .map(|f| f.to_str().unwrap_or(""))
                .unwrap_or("")
                .to_string();
            result.push(ShellInfo {
                name,
                path: shell_env,
                found: true,
            });
        }
    }

    #[cfg(target_os = "windows")]
    {
        let shell_names = ["bash.exe", "pwsh.exe", "powershell.exe", "cmd.exe"];
        let extra_bash_paths = [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
            r"C:\msys64\usr\bin\bash.exe",
            r"C:\mingw64\usr\bin\bash.exe",
            r"C:\Git\bin\bash.exe",
        ];

        for name in &shell_names {
            if let Ok(found_path) = which::which(name) {
                result.push(ShellInfo {
                    name: name.to_string(),
                    path: found_path.to_str().unwrap_or("").to_string(),
                    found: true,
                });
            } else if *name == "bash.exe" {
                for extra_path in &extra_bash_paths {
                    if std::path::Path::new(extra_path).exists() {
                        result.push(ShellInfo {
                            name: name.to_string(),
                            path: extra_path.to_string(),
                            found: true,
                        });
                        break;
                    }
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shells_non_empty() {
        let shells = detect_shells();
        assert!(!shells.is_empty());
    }

    #[test]
    fn test_shell_name_not_empty() {
        let shells = detect_shells();
        for shell in &shells {
            assert!(!shell.name.is_empty());
        }
    }

    #[test]
    fn test_shell_path_not_empty() {
        let shells = detect_shells();
        for shell in &shells {
            assert!(!shell.path.is_empty());
        }
    }

    #[test]
    fn test_shell_found_flag() {
        let shells = detect_shells();
        for shell in &shells {
            assert!(shell.found);
        }
    }

    #[test]
    fn test_shell_path_exists() {
        let shells = detect_shells();
        for shell in &shells {
            assert!(std::path::Path::new(&shell.path).exists());
        }
    }

    #[test]
    fn test_common_shells() {
        let shells = detect_shells();
        let names: Vec<&str> = shells.iter().map(|s| s.name.as_str()).collect();
        let common = ["bash", "zsh", "sh", "fish"];
        let has_common = common.iter().any(|c| names.contains(c));
        assert!(has_common);
    }
}
