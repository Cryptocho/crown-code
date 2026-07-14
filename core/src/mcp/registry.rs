use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::mcp::client::{McpClient, McpClientConfig, McpConnectionState, McpTransportKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpRegistryError {
    Ok,
    ServerNotFound,
    ServerDisabled,
    NotConnected,
    ConfigError,
}

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub transport: McpTransportKind,
    pub command: String,
    pub args: Vec<String>,
    pub server_url: String,
    pub auth_token: String,
    pub enabled: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        McpServerConfig {
            transport: McpTransportKind::Stdio,
            command: String::new(),
            args: Vec::new(),
            server_url: String::new(),
            auth_token: String::new(),
            enabled: true,
        }
    }
}

pub type McpStatusCallback = Box<dyn Fn(&str, McpConnectionState, &str) + Send + Sync>;

struct RegistryInner {
    configs: HashMap<String, McpServerConfig>,
    status_cb: Option<McpStatusCallback>,
    last_error: String,
}

pub struct McpRegistry {
    inner: Arc<Mutex<RegistryInner>>,
    clients: HashMap<String, McpClient>,
    destroyed: bool,
}

impl McpRegistry {
    pub fn new() -> Self {
        McpRegistry {
            inner: Arc::new(Mutex::new(RegistryInner {
                configs: HashMap::new(),
                status_cb: None,
                last_error: String::new(),
            })),
            clients: HashMap::new(),
            destroyed: false,
        }
    }

    pub async fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;
        for (_, mut client) in self.clients.drain() {
            client.destroy().await;
        }
        let mut inner = self.inner.lock().unwrap();
        inner.configs.clear();
        inner.last_error.clear();
        inner.status_cb = None;
    }

    pub fn server_count(&self) -> usize {
        if self.destroyed {
            return 0;
        }
        self.clients.len()
    }

    pub fn last_error(&self) -> String {
        if self.destroyed {
            return "null registry".to_string();
        }
        let inner = self.inner.lock().unwrap();
        inner.last_error.clone()
    }

    fn set_error(&self, msg: &str) {
        if self.destroyed {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        inner.last_error = msg.to_string();
    }

    pub fn server_names(&self) -> Vec<String> {
        if self.destroyed {
            return Vec::new();
        }
        let inner = self.inner.lock().unwrap();
        let mut names: Vec<String> = inner.configs.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn load_json_config(&mut self, json_str: &str) -> McpRegistryError {
        if self.destroyed {
            return McpRegistryError::ConfigError;
        }

        let root: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                self.set_error(&format!("invalid JSON: {}", e));
                return McpRegistryError::ConfigError;
            }
        };

        let servers = match root.get("servers") {
            Some(s) if s.is_object() => s.as_object().unwrap(),
            Some(_) => {
                self.set_error("missing 'servers' field");
                return McpRegistryError::ConfigError;
            }
            None => {
                self.set_error("missing 'servers' field");
                return McpRegistryError::ConfigError;
            }
        };

        for (server_name, server_val) in servers {
            if server_name.is_empty() {
                self.set_error("empty server name");
                return McpRegistryError::ConfigError;
            }

            let transport_str = server_val["transport"].as_str().unwrap_or("stdio");
            let transport = match transport_str {
                "stdio" => McpTransportKind::Stdio,
                "http" => McpTransportKind::Http,
                other => {
                    self.set_error(&format!(
                        "unknown transport '{}' for server '{}'",
                        other, server_name
                    ));
                    return McpRegistryError::ConfigError;
                }
            };

            let mut config = McpServerConfig {
                transport,
                command: server_val["command"].as_str().unwrap_or("").to_string(),
                server_url: server_val["url"].as_str().unwrap_or("").to_string(),
                auth_token: server_val["authToken"].as_str().unwrap_or("").to_string(),
                enabled: true,
                ..Default::default()
            };

            if let Some(args_val) = server_val["args"].as_array() {
                for arg in args_val {
                    config.args.push(arg.as_str().unwrap_or("").to_string());
                }
            }

            if let Some(enabled_val) = server_val["enabled"].as_bool() {
                config.enabled = enabled_val;
            }

            if config.transport == McpTransportKind::Stdio && config.command.is_empty() {
                self.set_error(&format!("stdio server '{}' missing 'command'", server_name));
                return McpRegistryError::ConfigError;
            }

            if config.transport == McpTransportKind::Http && config.server_url.is_empty() {
                self.set_error(&format!("http server '{}' missing 'url'", server_name));
                return McpRegistryError::ConfigError;
            }

            let mut inner = self.inner.lock().unwrap();
            inner.configs.insert(server_name.clone(), config);
        }

        McpRegistryError::Ok
    }

    pub async fn get_client(&mut self, name: &str) -> Option<&McpClient> {
        if self.destroyed {
            return None;
        }

        let config = {
            let inner = self.inner.lock().unwrap();
            let cfg = match inner.configs.get(name) {
                Some(c) => c.clone(),
                None => {
                    drop(inner);
                    self.set_error(&format!("server '{}' not found", name));
                    return None;
                }
            };
            if !cfg.enabled {
                drop(inner);
                self.set_error(&format!("server '{}' is disabled", name));
                return None;
            }
            cfg
        };

        if self.clients.contains_key(name) {
            return self.clients.get(name);
        }

        let weak = Arc::downgrade(&self.inner);
        let sname = name.to_string();

        let on_disconnect = {
            let weak = weak.clone();
            let sname_clone = sname.clone();
            Box::new(move || {
                if let Some(inner) = weak.upgrade() {
                    let guard = inner.lock().unwrap();
                    if let Some(ref cb) = guard.status_cb {
                        cb(
                            &sname_clone,
                            McpConnectionState::Disconnected,
                            "connection lost",
                        );
                    }
                }
            }) as Box<dyn Fn() + Send + Sync>
        };

        let on_reconnect = {
            let sname_clone = sname.clone();
            Box::new(move || {
                if let Some(inner) = weak.upgrade() {
                    let guard = inner.lock().unwrap();
                    if let Some(ref cb) = guard.status_cb {
                        cb(&sname_clone, McpConnectionState::Connected, "");
                    }
                }
            }) as Box<dyn Fn() + Send + Sync>
        };

        let client_config = McpClientConfig {
            transport: config.transport,
            command: config.command,
            args: config.args,
            server_url: config.server_url,
            auth_token: config.auth_token,
            on_disconnect: Some(on_disconnect),
            on_reconnect: Some(on_reconnect),
            ..Default::default()
        };

        let client = match McpClient::new(client_config).await {
            Ok(c) => c,
            Err(_) => return None,
        };

        self.clients.insert(name.to_string(), client);
        self.clients.get(name)
    }

    pub fn set_status_callback(&mut self, cb: McpStatusCallback) {
        if self.destroyed {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        inner.status_cb = Some(cb);
    }
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for McpRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpRegistry")
            .field("server_count", &self.server_count())
            .field("destroyed", &self.destroyed)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_server_path() -> Option<String> {
        if let Ok(path) = std::env::var("CARGO_BIN_EXE_mock_mcp_server") {
            return Some(path);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join("mock_mcp_server");
                if candidate.exists() {
                    return Some(candidate.to_string_lossy().to_string());
                }
                let candidate_exe = dir.join("mock_mcp_server.exe");
                if candidate_exe.exists() {
                    return Some(candidate_exe.to_string_lossy().to_string());
                }
            }
        }
        None
    }

    mod nil_safety {
        use super::*;

        #[tokio::test]
        async fn test_destroy_on_empty_registry() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_load_json_config_after_destroy() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
            let result = reg.load_json_config(r#"{"servers": {"srv": {"command": "/bin/echo"}}}"#);
            assert_eq!(result, McpRegistryError::ConfigError);
        }

        #[tokio::test]
        async fn test_get_client_after_destroy() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
            assert!(reg.get_client("test").await.is_none());
        }

        #[tokio::test]
        async fn test_server_count_after_destroy() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
            assert_eq!(reg.server_count(), 0);
        }

        #[tokio::test]
        async fn test_get_last_error_after_destroy() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
            assert_eq!(reg.last_error(), "null registry");
        }

        #[tokio::test]
        async fn test_set_status_callback_after_destroy() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
            reg.set_status_callback(Box::new(|_, _, _| {}));
        }

        #[tokio::test]
        async fn test_server_names_after_destroy() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
            assert!(reg.server_names().is_empty());
        }
    }

    mod config_parsing {
        use super::*;

        #[tokio::test]
        async fn test_valid_json_with_stdio_server() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"my-server": {"transport": "stdio", "command": "/path/to/server", "args": ["--flag"]}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let names = reg.server_names();
            assert_eq!(names, vec!["my-server"]);
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_valid_json_with_http_server() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"http-server": {"transport": "http", "url": "https://example.com/mcp", "authToken": "token123"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let names = reg.server_names();
            assert_eq!(names, vec!["http-server"]);
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_valid_json_with_both() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"stdio-srv": {"transport": "stdio", "command": "/bin/echo"}, "http-srv": {"transport": "http", "url": "https://example.com/mcp"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let names = reg.server_names();
            assert_eq!(names.len(), 2);
            assert!(names.contains(&"stdio-srv".to_string()));
            assert!(names.contains(&"http-srv".to_string()));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_invalid_json_string() {
            let mut reg = McpRegistry::new();
            assert_eq!(
                reg.load_json_config("{invalid"),
                McpRegistryError::ConfigError
            );
            assert!(!reg.last_error().is_empty());
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_missing_servers_field() {
            let mut reg = McpRegistry::new();
            assert_eq!(
                reg.load_json_config(r#"{"other": {}}"#),
                McpRegistryError::ConfigError
            );
            assert!(reg.last_error().contains("servers"));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_stdio_missing_command() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"no-cmd": {"transport": "stdio"}}}"#;
            assert_eq!(
                reg.load_json_config(json_str),
                McpRegistryError::ConfigError
            );
            assert!(reg.last_error().contains("command"));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_http_missing_url() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"no-url": {"transport": "http"}}}"#;
            assert_eq!(
                reg.load_json_config(json_str),
                McpRegistryError::ConfigError
            );
            assert!(reg.last_error().contains("url"));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_unknown_transport_value() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"bad": {"transport": "ws"}}}"#;
            assert_eq!(
                reg.load_json_config(json_str),
                McpRegistryError::ConfigError
            );
            assert!(reg.last_error().contains("transport"));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_empty_server_name() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"": {"transport": "stdio", "command": "/bin/echo"}}}"#;
            assert_eq!(
                reg.load_json_config(json_str),
                McpRegistryError::ConfigError
            );
            assert!(reg.last_error().contains("empty server"));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_enabled_false_parsed_correctly() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"disabled-srv": {"transport": "stdio", "command": "/bin/echo", "enabled": false}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let enabled = {
                let inner = reg.inner.lock().unwrap();
                inner.configs["disabled-srv"].enabled
            };
            assert!(!enabled);
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_enabled_defaults_to_true() {
            let mut reg = McpRegistry::new();
            let json_str =
                r#"{"servers": {"default-srv": {"transport": "stdio", "command": "/bin/echo"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let enabled = {
                let inner = reg.inner.lock().unwrap();
                inner.configs["default-srv"].enabled
            };
            assert!(enabled);
            reg.destroy().await;
        }
    }

    mod server_names {
        use super::*;

        #[tokio::test]
        async fn test_get_server_names_returns_correct_list() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"c": {"command": "/bin/c"}, "a": {"command": "/bin/a"}, "b": {"command": "/bin/b"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let names = reg.server_names();
            assert_eq!(names.len(), 3);
            assert!(names.contains(&"a".to_string()));
            assert!(names.contains(&"b".to_string()));
            assert!(names.contains(&"c".to_string()));
            reg.destroy().await;
        }

        #[test]
        fn test_empty_registry_returns_empty_vec() {
            let reg = McpRegistry::new();
            assert!(reg.server_names().is_empty());
        }
    }

    mod server_count {
        use super::*;

        #[test]
        fn test_empty_registry_returns_zero() {
            let reg = McpRegistry::new();
            assert_eq!(reg.server_count(), 0);
        }

        #[tokio::test]
        async fn test_after_load_json_config_returns_zero() {
            let mut reg = McpRegistry::new();
            let json_str =
                r#"{"servers": {"a": {"command": "/bin/a"}, "b": {"command": "/bin/b"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            assert_eq!(reg.server_count(), 0);
            reg.destroy().await;
        }
    }

    mod get_client {
        use super::*;

        #[tokio::test]
        async fn test_unknown_server_returns_none() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"known": {"command": "/bin/echo"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            assert!(reg.get_client("unknown").await.is_none());
            assert!(reg.last_error().contains("not found"));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_disabled_server_returns_none() {
            let mut reg = McpRegistry::new();
            let json_str =
                r#"{"servers": {"disabled-srv": {"command": "/bin/echo", "enabled": false}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            assert!(reg.get_client("disabled-srv").await.is_none());
            assert!(reg.last_error().contains("disabled"));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_normal_stdio_server_connection() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let mut reg = McpRegistry::new();
            let json_str = format!(
                r#"{{"servers": {{"mock": {{"command": "{}", "requestTimeoutMs": 5000}}}}}}"#,
                path
            );
            assert_eq!(reg.load_json_config(&json_str), McpRegistryError::Ok);
            let client = reg.get_client("mock").await;
            assert!(client.is_some());
            assert_eq!(client.unwrap().state(), McpConnectionState::Connected);
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_repeated_get_client_returns_same_instance() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let mut reg = McpRegistry::new();
            let json_str = format!(
                r#"{{"servers": {{"mock": {{"command": "{}", "requestTimeoutMs": 5000}}}}}}"#,
                path
            );
            assert_eq!(reg.load_json_config(&json_str), McpRegistryError::Ok);
            let c1 = reg.get_client("mock").await.unwrap() as *const McpClient;
            let c2 = reg.get_client("mock").await.unwrap() as *const McpClient;
            assert_eq!(c1, c2);
            reg.destroy().await;
        }
    }

    mod status_callback {
        use super::*;

        #[tokio::test]
        async fn test_set_status_callback_does_not_crash() {
            let mut reg = McpRegistry::new();
            reg.set_status_callback(Box::new(|_, _, _| {}));
            reg.destroy().await;
        }
    }

    mod error_handling {
        use super::*;

        #[tokio::test]
        async fn test_get_last_error_returns_correct_message() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"known": {"command": "/bin/echo"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let _ = reg.get_client("unknown").await;
            let err = reg.last_error();
            assert!(!err.is_empty());
            assert!(err.contains("not found"));
            reg.destroy().await;
        }
    }

    mod lifecycle {
        use super::*;

        #[tokio::test]
        async fn test_destroy_cleans_up_clients() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let mut reg = McpRegistry::new();
            let json_str = format!(
                r#"{{"servers": {{"mock": {{"command": "{}", "requestTimeoutMs": 5000}}}}}}"#,
                path
            );
            assert_eq!(reg.load_json_config(&json_str), McpRegistryError::Ok);
            let _ = reg.get_client("mock").await;
            assert_eq!(reg.server_count(), 1);
            reg.destroy().await;
            assert_eq!(reg.server_count(), 0);
        }

        #[tokio::test]
        async fn test_get_client_after_destroy_returns_none() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let mut reg = McpRegistry::new();
            let json_str = format!(
                r#"{{"servers": {{"mock": {{"command": "{}", "requestTimeoutMs": 5000}}}}}}"#,
                path
            );
            assert_eq!(reg.load_json_config(&json_str), McpRegistryError::Ok);
            let _ = reg.get_client("mock").await;
            reg.destroy().await;
            assert!(reg.get_client("mock").await.is_none());
        }
    }

    mod default_debug {
        use super::*;

        #[test]
        fn test_registry_default_impl() {
            let reg = McpRegistry::default();
            assert_eq!(reg.server_count(), 0);
            assert!(reg.server_names().is_empty());
        }

        #[test]
        fn test_registry_debug_impl() {
            let reg = McpRegistry::new();
            let debug = format!("{:?}", reg);
            assert!(debug.contains("server_count"));
            assert!(debug.contains("destroyed"));
        }

        #[test]
        fn test_server_config_default() {
            let cfg = McpServerConfig::default();
            assert_eq!(cfg.transport, McpTransportKind::Stdio);
            assert!(cfg.command.is_empty());
            assert!(cfg.enabled);
        }
    }

    mod config_args_auth {
        use super::*;

        #[tokio::test]
        async fn test_load_json_config_with_args() {
            let mut reg = McpRegistry::new();
            let json_str = r#"{"servers": {"srv": {"command": "/bin/echo", "args": ["--flag", "--port", "8080"]}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let args = {
                let inner = reg.inner.lock().unwrap();
                inner.configs["srv"].args.clone()
            };
            assert_eq!(args, vec!["--flag", "--port", "8080"]);
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_load_json_config_with_auth_token() {
            let mut reg = McpRegistry::new();
            let json_str =
                r#"{"servers": {"srv": {"command": "/bin/echo", "authToken": "secret"}}}"#;
            assert_eq!(reg.load_json_config(json_str), McpRegistryError::Ok);
            let auth_token = {
                let inner = reg.inner.lock().unwrap();
                inner.configs["srv"].auth_token.clone()
            };
            assert_eq!(auth_token, "secret");
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_load_json_config_reload() {
            let mut reg = McpRegistry::new();
            let json_a = r#"{"servers": {"srvA": {"command": "/bin/echo"}}}"#;
            assert_eq!(reg.load_json_config(json_a), McpRegistryError::Ok);
            let json_b = r#"{"servers": {"srvB": {"command": "/bin/echo"}}}"#;
            assert_eq!(reg.load_json_config(json_b), McpRegistryError::Ok);
            let names = reg.server_names();
            assert!(names.contains(&"srvA".to_string()));
            assert!(names.contains(&"srvB".to_string()));
            reg.destroy().await;
        }

        #[tokio::test]
        async fn test_get_client_after_destroy_idempotent() {
            let mut reg = McpRegistry::new();
            reg.destroy().await;
            reg.destroy().await;
            assert!(reg.get_client("x").await.is_none());
        }

        #[test]
        fn test_mcp_registry_error_variants() {
            assert_ne!(McpRegistryError::Ok, McpRegistryError::ServerNotFound);
            assert_ne!(McpRegistryError::Ok, McpRegistryError::ServerDisabled);
            assert_ne!(McpRegistryError::Ok, McpRegistryError::NotConnected);
            assert_ne!(McpRegistryError::Ok, McpRegistryError::ConfigError);
            assert_ne!(
                McpRegistryError::ServerNotFound,
                McpRegistryError::ServerDisabled
            );
            assert_ne!(
                McpRegistryError::ServerNotFound,
                McpRegistryError::NotConnected
            );
            assert_ne!(
                McpRegistryError::ServerNotFound,
                McpRegistryError::ConfigError
            );
            assert_ne!(
                McpRegistryError::ServerDisabled,
                McpRegistryError::NotConnected
            );
            assert_ne!(
                McpRegistryError::ServerDisabled,
                McpRegistryError::ConfigError
            );
            assert_ne!(
                McpRegistryError::NotConnected,
                McpRegistryError::ConfigError
            );
        }
    }
}
