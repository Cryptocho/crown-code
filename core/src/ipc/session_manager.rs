use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock, mpsc};

use crate::agent::r#loop::{AgentEventHandler, AgentSession};
use crate::api::types::ApiClientConfig;
use crate::ipc::message::{
    JsonRpcMessage, METHOD_ASSISTANT_REASONING, METHOD_ASSISTANT_TEXT, METHOD_ERROR,
    METHOD_TASK_DONE, METHOD_TOOL_CALL_START, METHOD_TOOL_RESULT, METHOD_USAGE, make_notification,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub cwd: String,
    pub created_at: u64,
    pub message_count: usize,
}

pub struct SessionState {
    pub info: SessionInfo,
    pub agent: AgentSession,
    pub event_tx: mpsc::Sender<JsonRpcMessage>,
}

pub(crate) struct IpcEventHandler {
    session_id: String,
    event_tx: mpsc::Sender<JsonRpcMessage>,
}

impl IpcEventHandler {
    pub fn new(session_id: String, event_tx: mpsc::Sender<JsonRpcMessage>) -> Self {
        Self {
            session_id,
            event_tx,
        }
    }
}

impl AgentEventHandler for IpcEventHandler {
    fn on_assistant_text(&mut self, delta: &str) {
        let _ = self.event_tx.try_send(make_notification(
            METHOD_ASSISTANT_TEXT,
            serde_json::json!({"session_id": self.session_id, "delta": delta}),
        ));
    }

    fn on_reasoning(&mut self, delta: &str) {
        let _ = self.event_tx.try_send(make_notification(
            METHOD_ASSISTANT_REASONING,
            serde_json::json!({"session_id": self.session_id, "delta": delta}),
        ));
    }

    fn on_tool_call_start(&mut self, call_id: &str, name: &str, arguments: &str) {
        let _ = self.event_tx.try_send(make_notification(
            METHOD_TOOL_CALL_START,
            serde_json::json!({
                "session_id": self.session_id,
                "call_id": call_id,
                "name": name,
                "arguments": arguments,
            }),
        ));
    }

    fn on_tool_result(&mut self, call_id: &str, name: &str, content: &str, is_error: bool) {
        let _ = self.event_tx.try_send(make_notification(
            METHOD_TOOL_RESULT,
            serde_json::json!({
                "session_id": self.session_id,
                "call_id": call_id,
                "name": name,
                "content": content,
                "is_error": is_error,
            }),
        ));
    }

    fn on_usage(&mut self, input_tokens: i32, output_tokens: i32, cache_read_tokens: i32) {
        let _ = self.event_tx.try_send(make_notification(
            METHOD_USAGE,
            serde_json::json!({
                "session_id": self.session_id,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "cache_read_tokens": cache_read_tokens,
            }),
        ));
    }

    fn on_task_done(&mut self, summary: &str) {
        let _ = self.event_tx.try_send(make_notification(
            METHOD_TASK_DONE,
            serde_json::json!({"session_id": self.session_id, "summary": summary}),
        ));
    }

    fn on_error(&mut self, code: i32, message: &str) {
        let _ = self.event_tx.try_send(make_notification(
            METHOD_ERROR,
            serde_json::json!({
                "session_id": self.session_id,
                "code": code,
                "message": message,
            }),
        ));
    }
}

fn generate_session_id() -> String {
    format!("sess_{}", nanoid::nanoid!(12))
}

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Mutex<SessionState>>>>,
    cancel_flags: RwLock<HashMap<String, Arc<AtomicBool>>>,
    config: Mutex<ApiClientConfig>,
}

impl SessionManager {
    pub fn new(config: ApiClientConfig) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            cancel_flags: RwLock::new(HashMap::new()),
            config: Mutex::new(config),
        }
    }

    pub async fn create_session(
        &self,
        cwd: String,
        event_tx: mpsc::Sender<JsonRpcMessage>,
    ) -> String {
        let config = self.config.lock().await.clone();
        let session_id = generate_session_id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cancelled = Arc::new(AtomicBool::new(false));
        let agent = AgentSession::new(config, cwd.clone(), cancelled.clone());

        let state = SessionState {
            info: SessionInfo {
                session_id: session_id.clone(),
                cwd,
                created_at: now,
                message_count: 0,
            },
            agent,
            event_tx,
        };
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), Arc::new(Mutex::new(state)));
        self.cancel_flags
            .write()
            .await
            .insert(session_id.clone(), cancelled);
        session_id
    }

    pub async fn destroy_session(&self, session_id: &str) -> Result<(), ()> {
        self.sessions
            .write()
            .await
            .remove(session_id)
            .map(|_| ())
            .ok_or(())?;
        self.cancel_flags.write().await.remove(session_id);
        Ok(())
    }

    pub async fn cancel_session(&self, session_id: &str) -> bool {
        if let Some(flag) = self.cancel_flags.read().await.get(session_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub async fn get_session(&self, session_id: &str) -> Option<Arc<Mutex<SessionState>>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions: Vec<Arc<Mutex<SessionState>>> =
            self.sessions.read().await.values().cloned().collect();
        let mut result = Vec::with_capacity(sessions.len());
        for session in sessions {
            let state = session.lock().await;
            result.push(state.info.clone());
        }
        result
    }

    pub async fn update_config(&self, config: ApiClientConfig) {
        *self.config.lock().await = config;
    }

    pub async fn get_config(&self) -> ApiClientConfig {
        self.config.lock().await.clone()
    }

    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::ApiClientConfig;

    fn make_config() -> ApiClientConfig {
        ApiClientConfig {
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: String::new(),
            model: "gemma4:e4b".to_string(),
            temperature: 0.0,
            max_tokens: 4096,
            stream_options: None,
        }
    }

    #[tokio::test]
    async fn test_create_returns_unique_ids() {
        let sm = SessionManager::new(make_config());
        let (tx, _rx) = mpsc::channel(256);
        let id1 = sm.create_session("/tmp".to_string(), tx.clone()).await;
        let id2 = sm.create_session("/tmp".to_string(), tx.clone()).await;
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_get_after_create() {
        let sm = SessionManager::new(make_config());
        let (tx, _rx) = mpsc::channel(256);
        let id = sm.create_session("/tmp".to_string(), tx).await;
        assert!(sm.get_session(&id).await.is_some());
    }

    #[tokio::test]
    async fn test_get_after_destroy() {
        let sm = SessionManager::new(make_config());
        let (tx, _rx) = mpsc::channel(256);
        let id = sm.create_session("/tmp".to_string(), tx).await;
        sm.destroy_session(&id).await.unwrap();
        assert!(sm.get_session(&id).await.is_none());
    }

    #[tokio::test]
    async fn test_list_all() {
        let sm = SessionManager::new(make_config());
        let (tx, _rx) = mpsc::channel(256);
        let id1 = sm.create_session("/a".to_string(), tx.clone()).await;
        let id2 = sm.create_session("/b".to_string(), tx.clone()).await;
        let sessions = sm.list_sessions().await;
        let ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(ids.contains(&id1.as_str()));
        assert!(ids.contains(&id2.as_str()));
    }

    #[tokio::test]
    async fn test_list_excludes_destroyed() {
        let sm = SessionManager::new(make_config());
        let (tx, _rx) = mpsc::channel(256);
        let id = sm.create_session("/tmp".to_string(), tx).await;
        sm.destroy_session(&id).await.unwrap();
        let sessions = sm.list_sessions().await;
        assert!(!sessions.iter().any(|s| s.session_id == id));
    }

    #[tokio::test]
    async fn test_update_config() {
        let sm = SessionManager::new(make_config());
        let mut cfg = sm.get_config().await;
        cfg.model = "new-model".to_string();
        sm.update_config(cfg).await;
        assert_eq!(sm.get_config().await.model, "new-model");
        assert_eq!(sm.get_config().await.base_url, "http://localhost:11434/v1");
    }

    #[tokio::test]
    async fn test_concurrent_create_destroy() {
        let sm = Arc::new(SessionManager::new(make_config()));
        let (tx, _rx) = mpsc::channel(256);
        let mut handles = Vec::new();
        for _ in 0..10 {
            let sm = Arc::clone(&sm);
            let tx = tx.clone();
            handles.push(tokio::spawn(async move {
                let id = sm.create_session("/tmp".to_string(), tx).await;
                let _ = sm.destroy_session(&id).await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(sm.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_cancel_session() {
        let sm = SessionManager::new(make_config());
        let (tx, _rx) = mpsc::channel(256);
        let id = sm.create_session("/tmp".to_string(), tx).await;
        assert!(sm.cancel_session(&id).await);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let sm = SessionManager::new(make_config());
        assert!(!sm.cancel_session("nonexistent").await);
    }

    #[tokio::test]
    async fn test_ipc_event_handler_on_assistant_text() {
        let (tx, mut rx) = mpsc::channel(256);
        let mut handler = IpcEventHandler::new("sess_test".to_string(), tx);
        handler.on_assistant_text("hello");
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.method.as_deref(), Some(METHOD_ASSISTANT_TEXT));
        let params = msg.params.unwrap();
        assert_eq!(params["session_id"], "sess_test");
        assert_eq!(params["delta"], "hello");
    }

    #[tokio::test]
    async fn test_ipc_event_handler_on_reasoning() {
        let (tx, mut rx) = mpsc::channel(256);
        let mut handler = IpcEventHandler::new("sess_test".to_string(), tx);
        handler.on_reasoning("thinking");
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.method.as_deref(), Some(METHOD_ASSISTANT_REASONING));
        let params = msg.params.unwrap();
        assert_eq!(params["session_id"], "sess_test");
        assert_eq!(params["delta"], "thinking");
    }

    #[tokio::test]
    async fn test_ipc_event_handler_on_tool_call_start() {
        let (tx, mut rx) = mpsc::channel(256);
        let mut handler = IpcEventHandler::new("sess_test".to_string(), tx);
        handler.on_tool_call_start("call_1", "read_file", "{}");
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.method.as_deref(), Some(METHOD_TOOL_CALL_START));
        let params = msg.params.unwrap();
        assert_eq!(params["session_id"], "sess_test");
        assert_eq!(params["call_id"], "call_1");
        assert_eq!(params["name"], "read_file");
        assert_eq!(params["arguments"], "{}");
    }

    #[tokio::test]
    async fn test_ipc_event_handler_on_tool_result() {
        let (tx, mut rx) = mpsc::channel(256);
        let mut handler = IpcEventHandler::new("sess_test".to_string(), tx);
        handler.on_tool_result("call_1", "read_file", "content", true);
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.method.as_deref(), Some(METHOD_TOOL_RESULT));
        let params = msg.params.unwrap();
        assert_eq!(params["session_id"], "sess_test");
        assert_eq!(params["call_id"], "call_1");
        assert_eq!(params["name"], "read_file");
        assert_eq!(params["content"], "content");
        assert_eq!(params["is_error"], true);
    }

    #[tokio::test]
    async fn test_ipc_event_handler_on_usage() {
        let (tx, mut rx) = mpsc::channel(256);
        let mut handler = IpcEventHandler::new("sess_test".to_string(), tx);
        handler.on_usage(100, 50, 30);
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.method.as_deref(), Some(METHOD_USAGE));
        let params = msg.params.unwrap();
        assert_eq!(params["session_id"], "sess_test");
        assert_eq!(params["input_tokens"], 100);
        assert_eq!(params["output_tokens"], 50);
        assert_eq!(params["cache_read_tokens"], 30);
    }

    #[tokio::test]
    async fn test_ipc_event_handler_on_task_done() {
        let (tx, mut rx) = mpsc::channel(256);
        let mut handler = IpcEventHandler::new("sess_test".to_string(), tx);
        handler.on_task_done("done");
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.method.as_deref(), Some(METHOD_TASK_DONE));
        let params = msg.params.unwrap();
        assert_eq!(params["session_id"], "sess_test");
        assert_eq!(params["summary"], "done");
    }

    #[tokio::test]
    async fn test_ipc_event_handler_on_error() {
        let (tx, mut rx) = mpsc::channel(256);
        let mut handler = IpcEventHandler::new("sess_test".to_string(), tx);
        handler.on_error(42, "boom");
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.method.as_deref(), Some(METHOD_ERROR));
        let params = msg.params.unwrap();
        assert_eq!(params["session_id"], "sess_test");
        assert_eq!(params["code"], 42);
        assert_eq!(params["message"], "boom");
    }

    #[tokio::test]
    async fn test_generate_session_id_format() {
        let sm = SessionManager::new(make_config());
        let (tx, _rx) = mpsc::channel(256);
        let id = sm.create_session("/tmp".to_string(), tx).await;
        assert!(id.starts_with("sess_"));
        assert_eq!(id.len(), 17);
    }

    #[test]
    fn test_session_info_serialization() {
        let info = SessionInfo {
            session_id: "sess_test123".to_string(),
            cwd: "/tmp".to_string(),
            created_at: 1000,
            message_count: 5,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["session_id"], "sess_test123");
        assert_eq!(json["cwd"], "/tmp");
        assert_eq!(json["created_at"], 1000);
        assert_eq!(json["message_count"], 5);

        let deserialized: SessionInfo = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.session_id, info.session_id);
        assert_eq!(deserialized.cwd, info.cwd);
        assert_eq!(deserialized.created_at, info.created_at);
        assert_eq!(deserialized.message_count, info.message_count);
    }
}
