use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::mcp::jsonrpc;
use crate::mcp::transport_http::HttpTransport;
use crate::mcp::transport_stdio::{
    close as stdio_close, read_json_line, start_stdio_transport, write_json_line, StdioTransport,
    TransportError,
};

pub const DEFAULT_REQUEST_TIMEOUT: u64 = 30_000;
pub const DEFAULT_CONNECT_TIMEOUT: u64 = 10_000;
pub const DEFAULT_PING_INTERVAL: u64 = 30;
pub const DEFAULT_PONG_TIMEOUT: u64 = 10;
pub const DEFAULT_MAX_RECONNECT: u32 = 3;
pub const DEFAULT_RECONNECT_DELAY: u64 = 1_000;
pub const MAX_RECONNECT_DELAY: u64 = 60_000;
pub const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";
pub const DEFAULT_CLIENT_NAME: &str = "crown-code";
pub const DEFAULT_CLIENT_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransportKind {
    Stdio,
    Http,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct McpContent {
    pub kind: String,
    pub text: String,
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct McpCallToolResult {
    pub content: Vec<McpContent>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Default)]
pub struct McpTool {
    pub name: String,
    pub description: String,
}

pub struct McpClientConfig {
    pub transport: McpTransportKind,
    pub command: String,
    pub args: Vec<String>,
    pub server_url: String,
    pub auth_token: String,
    pub get_token: Option<Box<dyn Fn() -> String + Send + Sync>>,
    pub refresh_token: Option<Box<dyn Fn() -> String + Send + Sync>>,
    pub request_timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub ping_interval_sec: u64,
    pub pong_timeout_sec: u64,
    pub max_reconnect: u32,
    pub reconnect_delay_ms: u64,
    pub on_disconnect: Option<Box<dyn Fn() + Send + Sync>>,
    pub on_reconnect: Option<Box<dyn Fn() + Send + Sync>>,
    pub protocol_version: String,
    pub client_name: String,
    pub client_version: String,
}

impl Default for McpClientConfig {
    fn default() -> Self {
        McpClientConfig {
            transport: McpTransportKind::Stdio,
            command: String::new(),
            args: Vec::new(),
            server_url: String::new(),
            auth_token: String::new(),
            get_token: None,
            refresh_token: None,
            request_timeout_ms: DEFAULT_REQUEST_TIMEOUT,
            connect_timeout_ms: DEFAULT_CONNECT_TIMEOUT,
            ping_interval_sec: DEFAULT_PING_INTERVAL,
            pong_timeout_sec: DEFAULT_PONG_TIMEOUT,
            max_reconnect: DEFAULT_MAX_RECONNECT,
            reconnect_delay_ms: DEFAULT_RECONNECT_DELAY,
            on_disconnect: None,
            on_reconnect: None,
            protocol_version: DEFAULT_PROTOCOL_VERSION.to_string(),
            client_name: DEFAULT_CLIENT_NAME.to_string(),
            client_version: DEFAULT_CLIENT_VERSION.to_string(),
        }
    }
}

impl std::fmt::Debug for McpClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClientConfig")
            .field("transport", &self.transport)
            .field("command", &self.command)
            .field("args", &self.args)
            .field("server_url", &self.server_url)
            .field("auth_token", &if self.auth_token.is_empty() { "" } else { "***" })
            .field("request_timeout_ms", &self.request_timeout_ms)
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .field("ping_interval_sec", &self.ping_interval_sec)
            .field("pong_timeout_sec", &self.pong_timeout_sec)
            .field("max_reconnect", &self.max_reconnect)
            .field("reconnect_delay_ms", &self.reconnect_delay_ms)
            .field("protocol_version", &self.protocol_version)
            .field("client_name", &self.client_name)
            .field("client_version", &self.client_version)
            .finish()
    }
}

struct ClientInner {
    config: McpClientConfig,
    transport_kind: McpTransportKind,
    stdio: Mutex<Option<StdioTransport>>,
    http: Mutex<Option<HttpTransport>>,
    state: Mutex<McpConnectionState>,
    last_error: Mutex<String>,
    request_id_counter: Mutex<i64>,
    initialized: Mutex<bool>,
    heartbeat_running: AtomicBool,
    transport_lock: Mutex<()>,
}

impl ClientInner {
    fn send_json_rpc(&self, method: &str, params: &Value) -> Option<Value> {
        let _transport_guard = self.transport_lock.lock().unwrap();
        let timeout_ms = if self.config.request_timeout_ms > 0 {
            self.config.request_timeout_ms
        } else {
            DEFAULT_REQUEST_TIMEOUT
        };

        let id = {
            let mut counter = self.request_id_counter.lock().unwrap();
            let current = *counter;
            *counter += 1;
            current
        };

        let req_json = jsonrpc::build_request(method, Some(params), id);

        match self.transport_kind {
            McpTransportKind::Stdio => {
                let write_ok = {
                    let mut stdio_guard = self.stdio.lock().unwrap();
                    let stdio = match stdio_guard.as_mut() {
                        Some(s) => s,
                        None => {
                            *self.last_error.lock().unwrap() =
                                "sendJsonRpc: stdio not available".to_string();
                            return None;
                        }
                    };
                    let write_err = write_json_line(stdio, &req_json);
                    write_err == TransportError::Ok
                };

                if !write_ok {
                    *self.last_error.lock().unwrap() =
                        "sendJsonRpc: write error".to_string();
                    return None;
                }

                let read_result = {
                    let mut stdio_guard = self.stdio.lock().unwrap();
                    let stdio = match stdio_guard.as_mut() {
                        Some(s) => s,
                        None => {
                            *self.last_error.lock().unwrap() =
                                "sendJsonRpc: stdio not available".to_string();
                            return None;
                        }
                    };
                    read_json_line(stdio, timeout_ms)
                };

                if read_result.error != TransportError::Ok {
                    *self.last_error.lock().unwrap() =
                        format!("sendJsonRpc: read error: {:?}", read_result.error);
                    return None;
                }

                match jsonrpc::parse_response(&read_result.line) {
                    Ok(resp) => {
                        if resp.get("error").is_some() {
                            *self.last_error.lock().unwrap() = format!(
                                "sendJsonRpc: {}",
                                resp["error"]["message"]
                                    .as_str()
                                    .unwrap_or("unknown error")
                            );
                            return None;
                        }
                        if let Some(resp_id) = resp["id"].as_i64()
                            && resp_id != id
                        {
                            *self.last_error.lock().unwrap() =
                                "sendJsonRpc: response id mismatch".to_string();
                            return None;
                        }
                        Some(resp.get("result").cloned().unwrap_or(Value::Null))
                    }
                    Err(_) => {
                        *self.last_error.lock().unwrap() =
                            "sendJsonRpc: invalid JSON response".to_string();
                        None
                    }
                }
            }
            McpTransportKind::Http => {
                let mut http_guard = self.http.lock().unwrap();
                let http = match http_guard.as_mut() {
                    Some(h) => h,
                    None => {
                        *self.last_error.lock().unwrap() =
                            "sendJsonRpc: http not available".to_string();
                        return None;
                    }
                };

                let http_resp = http.post_json(&req_json);

                if http_resp.status_code == 401 {
                    if let Some(ref refresh) = self.config.refresh_token {
                        let new_token = refresh();
                        if !new_token.is_empty() {
                            http.bearer_token = new_token.clone();
                            let retry_resp = http.post_json(&req_json);
                            if retry_resp.status_code == 200 && retry_resp.error.is_empty() {
                                return match jsonrpc::parse_response(&retry_resp.body) {
                                    Ok(resp) => {
                                        if resp.get("error").is_some() {
                                            *self.last_error.lock().unwrap() = format!(
                                                "sendJsonRpc: {}",
                                                resp["error"]["message"]
                                                    .as_str()
                                                    .unwrap_or("unknown error")
                                            );
                                            return None;
                                        }
if let Some(resp_id) = resp["id"].as_i64()
                                && resp_id != id
                            {
                                *self.last_error.lock().unwrap() =
                                    "sendJsonRpc: response id mismatch".to_string();
                                return None;
                            }
                            Some(resp.get("result").cloned().unwrap_or(Value::Null))
                                    }
                                    Err(_) => {
                                        *self.last_error.lock().unwrap() =
                                            "sendJsonRpc: invalid JSON response".to_string();
                                        None
                                    }
                                };
                            }
                        }
                    }
                    *self.last_error.lock().unwrap() =
                        "sendJsonRpc: HTTP 401 (token rejected)".to_string();
                    return None;
                }

                if http_resp.status_code != 200 || !http_resp.error.is_empty() {
                    let err_details = if !http_resp.error.is_empty() {
                        http_resp.error.clone()
                    } else {
                        format!("HTTP {}", http_resp.status_code)
                    };
                    *self.last_error.lock().unwrap() =
                        format!("sendJsonRpc: {}", err_details);
                    return None;
                }

                match jsonrpc::parse_response(&http_resp.body) {
                    Ok(resp) => {
                        if resp.get("error").is_some() {
                            *self.last_error.lock().unwrap() = format!(
                                "sendJsonRpc: {}",
                                resp["error"]["message"]
                                    .as_str()
                                    .unwrap_or("unknown error")
                            );
                            return None;
                        }
                        if let Some(resp_id) = resp["id"].as_i64()
                            && resp_id != id
                        {
                            *self.last_error.lock().unwrap() =
                                "sendJsonRpc: response id mismatch".to_string();
                            return None;
                        }
                        Some(resp.get("result").cloned().unwrap_or(Value::Null))
                    }
                    Err(_) => {
                        *self.last_error.lock().unwrap() =
                            "sendJsonRpc: invalid JSON response".to_string();
                        None
                    }
                }
            }
        }
    }

    fn send_notification(&self, method: &str, params: &Value) -> bool {
        let _transport_guard = self.transport_lock.lock().unwrap();
        let notif_json = jsonrpc::build_notification(method, Some(params));

        match self.transport_kind {
            McpTransportKind::Stdio => {
                let mut stdio_guard = self.stdio.lock().unwrap();
                let stdio = match stdio_guard.as_mut() {
                    Some(s) => s,
                    None => return false,
                };
                write_json_line(stdio, &notif_json) == TransportError::Ok
            }
            McpTransportKind::Http => {
                let mut http_guard = self.http.lock().unwrap();
                let http = match http_guard.as_mut() {
                    Some(h) => h,
                    None => return false,
                };
                let http_resp = http.post_json(&notif_json);
                http_resp.status_code == 200 && http_resp.error.is_empty()
            }
        }
    }

    fn initialize(&self) -> bool {
        let params = json!({
            "protocolVersion": self.config.protocol_version,
            "clientInfo": {
                "name": self.config.client_name,
                "version": self.config.client_version
            },
            "capabilities": {}
        });
        let resp = self.send_json_rpc("initialize", &params);
        if resp.is_none() {
            return false;
        }
        if !self.send_notification("notifications/initialized", &Value::Null) {
            *self.last_error.lock().unwrap() = "initialize: notification failed".to_string();
            return false;
        }
        *self.initialized.lock().unwrap() = true;
        true
    }

    fn reconnect(&self) -> bool {
        *self.state.lock().unwrap() = McpConnectionState::Reconnecting;

        match self.transport_kind {
            McpTransportKind::Stdio => {
                let mut stdio_guard = self.stdio.lock().unwrap();
                if let Some(ref mut t) = *stdio_guard {
                    stdio_close(t);
                }
            }
            McpTransportKind::Http => {
                let mut http_guard = self.http.lock().unwrap();
                if let Some(ref mut t) = *http_guard {
                    t.close();
                }
            }
        }

        let delay_ms = if self.config.reconnect_delay_ms > 0 {
            self.config.reconnect_delay_ms
        } else {
            DEFAULT_RECONNECT_DELAY
        };
        let max_retries = if self.config.max_reconnect > 0 {
            self.config.max_reconnect
        } else {
            DEFAULT_MAX_RECONNECT
        };
        let mut current_delay = delay_ms;

        for _ in 0..max_retries {
            thread::sleep(Duration::from_millis(current_delay));

            match self.transport_kind {
                McpTransportKind::Stdio => {
                    let args: Vec<&str> =
                        self.config.args.iter().map(|s| s.as_str()).collect();
                    match start_stdio_transport(&self.config.command, &args) {
                        Ok(t) => {
                            *self.stdio.lock().unwrap() = Some(t);
                        }
                        Err(_) => {
                            current_delay =
                                std::cmp::min(current_delay * 2, MAX_RECONNECT_DELAY);
                            continue;
                        }
                    }
                }
                McpTransportKind::Http => {
                    let t = HttpTransport::new(&self.config.server_url, &self.config.auth_token);
                    *self.http.lock().unwrap() = Some(t);
                }
            }

            if self.initialize() {
                *self.state.lock().unwrap() = McpConnectionState::Connected;
                return true;
            }

            current_delay = std::cmp::min(current_delay * 2, MAX_RECONNECT_DELAY);
        }

        *self.state.lock().unwrap() = McpConnectionState::Error;
        *self.last_error.lock().unwrap() =
            format!("reconnect failed after {} attempts", max_retries);
        false
    }

    fn heartbeat_proc(inner: Arc<ClientInner>) {
        let ping_interval_ms = if inner.config.ping_interval_sec > 0 {
            inner.config.ping_interval_sec * 1000
        } else {
            DEFAULT_PING_INTERVAL * 1000
        };

        while inner.heartbeat_running.load(Ordering::Acquire) {
            let mut waited: u64 = 0;
            while waited < ping_interval_ms {
                thread::sleep(Duration::from_millis(100));
                waited += 100;
                if !inner.heartbeat_running.load(Ordering::Acquire) {
                    return;
                }
            }

            if !inner.heartbeat_running.load(Ordering::Acquire) {
                return;
            }

            {
                let state = *inner.state.lock().unwrap();
                if state != McpConnectionState::Connected {
                    continue;
                }
            }

            let resp = inner.send_json_rpc("ping", &Value::Null);
            if resp.is_none() {
                let should_reconnect = {
                    let state = *inner.state.lock().unwrap();
                    state == McpConnectionState::Connected
                };
                if should_reconnect
                    && let Some(ref cb) = inner.config.on_disconnect
                {
                    cb();
                }
                if inner.reconnect()
                    && let Some(ref cb) = inner.config.on_reconnect
                {
                    cb();
                }
            }
        }
    }
}

pub struct McpClient {
    inner: Arc<ClientInner>,
    heartbeat_thread: Option<thread::JoinHandle<()>>,
}

impl McpClient {
    pub fn new(config: McpClientConfig) -> Result<McpClient, String> {
        let transport_kind = config.transport;
        let mut request_timeout_ms = config.request_timeout_ms;
        let mut connect_timeout_ms = config.connect_timeout_ms;
        let mut ping_interval_sec = config.ping_interval_sec;
        let mut pong_timeout_sec = config.pong_timeout_sec;
        let mut max_reconnect = config.max_reconnect;
        let mut reconnect_delay_ms = config.reconnect_delay_ms;
        let mut protocol_version = config.protocol_version.clone();
        let mut client_name = config.client_name.clone();
        let mut client_version = config.client_version.clone();

        if request_timeout_ms == 0 {
            request_timeout_ms = DEFAULT_REQUEST_TIMEOUT;
        }
        if connect_timeout_ms == 0 {
            connect_timeout_ms = DEFAULT_CONNECT_TIMEOUT;
        }
        if ping_interval_sec == 0 {
            ping_interval_sec = DEFAULT_PING_INTERVAL;
        }
        if pong_timeout_sec == 0 {
            pong_timeout_sec = DEFAULT_PONG_TIMEOUT;
        }
        if max_reconnect == 0 {
            max_reconnect = DEFAULT_MAX_RECONNECT;
        }
        if reconnect_delay_ms == 0 {
            reconnect_delay_ms = DEFAULT_RECONNECT_DELAY;
        }
        if protocol_version.is_empty() {
            protocol_version = DEFAULT_PROTOCOL_VERSION.to_string();
        }
        if client_name.is_empty() {
            client_name = DEFAULT_CLIENT_NAME.to_string();
        }
        if client_version.is_empty() {
            client_version = DEFAULT_CLIENT_VERSION.to_string();
        }

        let inner = Arc::new(ClientInner {
            config: McpClientConfig {
                transport: transport_kind,
                command: config.command,
                args: config.args,
                server_url: config.server_url,
                auth_token: config.auth_token,
                get_token: config.get_token,
                refresh_token: config.refresh_token,
                request_timeout_ms,
                connect_timeout_ms,
                ping_interval_sec,
                pong_timeout_sec,
                max_reconnect,
                reconnect_delay_ms,
                on_disconnect: config.on_disconnect,
                on_reconnect: config.on_reconnect,
                protocol_version,
                client_name,
                client_version,
            },
            transport_kind,
            stdio: Mutex::new(None),
            http: Mutex::new(None),
            state: Mutex::new(McpConnectionState::Disconnected),
            last_error: Mutex::new(String::new()),
            request_id_counter: Mutex::new(1),
            initialized: Mutex::new(false),
            heartbeat_running: AtomicBool::new(false),
            transport_lock: Mutex::new(()),
        });

        *inner.state.lock().unwrap() = McpConnectionState::Connecting;

        match transport_kind {
            McpTransportKind::Stdio => {
                let args: Vec<&str> = inner.config.args.iter().map(|s| s.as_str()).collect();
                match start_stdio_transport(&inner.config.command, &args) {
                    Ok(t) => {
                        *inner.stdio.lock().unwrap() = Some(t);
                    }
                    Err(e) => {
                        *inner.last_error.lock().unwrap() = e;
                        *inner.state.lock().unwrap() = McpConnectionState::Error;
                        return Ok(McpClient {
                            inner,
                            heartbeat_thread: None,
                        });
                    }
                }
            }
            McpTransportKind::Http => {
                let t = HttpTransport::new(&inner.config.server_url, &inner.config.auth_token);
                *inner.http.lock().unwrap() = Some(t);
            }
        }

        if !inner.initialize() {
            *inner.state.lock().unwrap() = McpConnectionState::Error;
            return Ok(McpClient {
                inner,
                heartbeat_thread: None,
            });
        }

        *inner.state.lock().unwrap() = McpConnectionState::Connected;
        *inner.initialized.lock().unwrap() = true;
        inner.heartbeat_running.store(true, Ordering::Release);

        let inner_clone = Arc::clone(&inner);
        let heartbeat_thread = match thread::Builder::new()
            .name("mcp-heartbeat".to_string())
            .spawn(move || {
                ClientInner::heartbeat_proc(inner_clone);
            })
        {
            Ok(h) => Some(h),
            Err(e) => {
                *inner.last_error.lock().unwrap() =
                    format!("failed to create heartbeat thread: {}", e);
                *inner.state.lock().unwrap() = McpConnectionState::Error;
                None
            }
        };

        Ok(McpClient {
            inner,
            heartbeat_thread,
        })
    }

    pub fn call_tool(&self, tool_name: &str, arguments: &Value) -> McpCallToolResult {
        {
            let state = *self.inner.state.lock().unwrap();
            if state != McpConnectionState::Connected {
                *self.inner.last_error.lock().unwrap() = "client not connected".to_string();
                return McpCallToolResult::default();
            }
        }

        let args = if arguments.is_null() {
            json!({})
        } else {
            arguments.clone()
        };

        let params = json!({"name": tool_name, "arguments": args});
        let resp = self.inner.send_json_rpc("tools/call", &params);
        match resp {
            None => McpCallToolResult::default(),
            Some(val) => {
                let mut result = McpCallToolResult::default();
                if let Some(content_array) = val["content"].as_array() {
                    for item in content_array {
                        let kind = item["type"].as_str().unwrap_or("").to_string();
                        let (text, data, mime_type) = if kind == "text" || kind == "resource" {
                            (item["text"].as_str().unwrap_or("").to_string(), String::new(), String::new())
                        } else if kind == "image" {
                            (String::new(), item["data"].as_str().unwrap_or("").to_string(), item["mimeType"].as_str().unwrap_or("").to_string())
                        } else {
                            (String::new(), String::new(), String::new())
                        };
                        let content = McpContent {
                            kind,
                            text,
                            data,
                            mime_type,
                        };
                        result.content.push(content);
                    }
                }
                result.is_error = val["isError"].as_bool().unwrap_or(false);
                result
            }
        }
    }

    pub fn list_tools(&self) -> Vec<McpTool> {
        {
            let state = *self.inner.state.lock().unwrap();
            if state != McpConnectionState::Connected {
                *self.inner.last_error.lock().unwrap() = "client not connected".to_string();
                return Vec::new();
            }
        }

        let resp = self.inner.send_json_rpc("tools/list", &Value::Null);
        match resp {
            None => Vec::new(),
            Some(val) => {
                let mut tools = Vec::new();
                if let Some(tools_array) = val["tools"].as_array() {
                    for item in tools_array {
                        let tool = McpTool {
                            name: item["name"].as_str().unwrap_or("").to_string(),
                            description: item["description"].as_str().unwrap_or("").to_string(),
                        };
                        tools.push(tool);
                    }
                }
                tools
            }
        }
    }

    pub fn state(&self) -> McpConnectionState {
        *self.inner.state.lock().unwrap()
    }

    pub fn last_error(&self) -> String {
        self.inner.last_error.lock().unwrap().clone()
    }

    pub fn destroy(&mut self) {
        self.inner
            .heartbeat_running
            .store(false, Ordering::Release);

        match self.inner.transport_kind {
            McpTransportKind::Stdio => {
                let mut stdio_guard = self.inner.stdio.lock().unwrap();
                if let Some(ref mut t) = *stdio_guard {
                    stdio_close(t);
                }
            }
            McpTransportKind::Http => {
                let mut http_guard = self.inner.http.lock().unwrap();
                if let Some(ref mut t) = *http_guard {
                    t.close();
                }
            }
        }

        if let Some(handle) = self.heartbeat_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.inner
            .heartbeat_running
            .store(false, Ordering::Release);
    }
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("state", &self.state())
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

    mod null_handling {
        use super::*;

        #[test]
        fn test_empty_config_produces_error_state() {
            let config = McpClientConfig::default();
            let client = McpClient::new(config).unwrap();
            assert_eq!(client.state(), McpConnectionState::Error);
            assert!(!client.last_error().is_empty());
        }

        #[test]
        fn test_call_tool_on_disconnected() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent".to_string(),
                ..Default::default()
            };
            let client = McpClient::new(config).unwrap();
            let result = client.call_tool("echo", &json!({"message": "hi"}));
            assert!(result.content.is_empty());
            assert!(!result.is_error);
            let _ = client; // drop
        }

        #[test]
        fn test_list_tools_on_disconnected() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent".to_string(),
                ..Default::default()
            };
            let client = McpClient::new(config).unwrap();
            let tools = client.list_tools();
            assert!(tools.is_empty());
        }

        #[test]
        fn test_get_state_on_error() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent".to_string(),
                ..Default::default()
            };
            let client = McpClient::new(config).unwrap();
            assert_eq!(client.state(), McpConnectionState::Error);
        }

        #[test]
        fn test_get_last_error_on_error() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent".to_string(),
                ..Default::default()
            };
            let client = McpClient::new(config).unwrap();
            let err = client.last_error();
            assert!(!err.is_empty());
        }
    }

    mod error_state {
        use super::*;

        #[test]
        fn test_nonexistent_command_produces_error_state() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent/path".to_string(),
                ..Default::default()
            };
            let client = McpClient::new(config).unwrap();
            assert_eq!(client.state(), McpConnectionState::Error);
            assert!(!client.last_error().is_empty());
        }

        #[test]
        fn test_call_tool_on_error_returns_empty() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent/path".to_string(),
                ..Default::default()
            };
            let client = McpClient::new(config).unwrap();
            let result = client.call_tool("echo", &json!({"message": "hi"}));
            assert!(result.content.is_empty());
            assert!(!result.is_error);
        }

        #[test]
        fn test_list_tools_on_error_returns_empty() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent/path".to_string(),
                ..Default::default()
            };
            let client = McpClient::new(config).unwrap();
            assert!(client.list_tools().is_empty());
        }

        #[test]
        fn test_destroy_on_error_does_not_panic() {
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: "/nonexistent/path".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            client.destroy();
        }
    }

    mod default_values {
        use super::*;

        #[test]
        fn test_mcp_call_tool_result_defaults() {
            let r = McpCallToolResult::default();
            assert!(r.content.is_empty());
            assert!(!r.is_error);
        }

        #[test]
        fn test_mcp_tool_defaults() {
            let t = McpTool::default();
            assert!(t.name.is_empty());
            assert!(t.description.is_empty());
        }
    }

    mod mock_server_integration {
        use super::*;

        #[test]
        fn test_connect_and_list_tools() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            assert_eq!(client.state(), McpConnectionState::Connected);

            let tools = client.list_tools();
            assert_eq!(tools.len(), 3);
            assert_eq!(tools[0].name, "echo");
            assert_eq!(tools[1].name, "add");
            assert_eq!(tools[2].name, "greet");

            client.destroy();
        }

        #[test]
        fn test_call_echo_tool() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            let result = client.call_tool("echo", &json!({"message": "hello world"}));
            assert_eq!(result.content.len(), 1);
            assert_eq!(result.content[0].text, "hello world");
            assert!(!result.is_error);
            client.destroy();
        }

        #[test]
        fn test_call_add_tool() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            let result = client.call_tool("add", &json!({"a": 3, "b": 4}));
            assert_eq!(result.content.len(), 1);
            assert_eq!(result.content[0].text, "7");
            client.destroy();
        }

        #[test]
        fn test_call_greet_tool() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            let result = client.call_tool("greet", &json!({"name": "Kilo"}));
            assert_eq!(result.content.len(), 1);
            assert_eq!(result.content[0].text, "Hello, Kilo!");
            client.destroy();
        }

        #[test]
        fn test_call_image_tool() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            let result = client.call_tool("image_tool", &json!({}));
            assert_eq!(result.content.len(), 2);
            assert_eq!(result.content[0].kind, "image");
            assert_eq!(result.content[0].data, "iVBORw0KGgo");
            assert_eq!(result.content[0].mime_type, "image/png");
            assert_eq!(result.content[1].kind, "text");
            client.destroy();
        }

        #[test]
        fn test_call_error_tool() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            let result = client.call_tool("error_tool", &json!({}));
            assert!(result.content.is_empty());
            assert!(result.is_error);
            client.destroy();
        }

        #[test]
        fn test_call_empty_tool() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            let result = client.call_tool("empty_tool", &json!({}));
            assert!(result.content.is_empty());
            assert!(!result.is_error);
            client.destroy();
        }

        #[test]
        fn test_call_unknown_tool() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            let result = client.call_tool("unknown_tool", &json!({}));
            assert!(result.content.is_empty());
            client.destroy();
        }
    }

    mod heartbeat_lifecycle {
        use super::*;

        #[test]
        fn test_heartbeat_starts_and_stops_cleanly() {
            let path = match mock_server_path() {
                Some(p) => p,
                None => return,
            };
            let config = McpClientConfig {
                transport: McpTransportKind::Stdio,
                command: path,
                ping_interval_sec: 1,
                request_timeout_ms: 5000,
                client_name: "test".to_string(),
                client_version: "1.0".to_string(),
                ..Default::default()
            };
            let mut client = McpClient::new(config).unwrap();
            assert_eq!(client.state(), McpConnectionState::Connected);
            client.destroy();
        }
    }
}