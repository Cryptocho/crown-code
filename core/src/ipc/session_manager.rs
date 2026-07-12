use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::api::types::{ApiClientConfig, Message};
use crate::ipc::message::JsonRpcMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub cwd: String,
    pub created_at: u64,
    pub message_count: usize,
}

pub struct SessionState {
    pub info: SessionInfo,
    pub history: Vec<Message>,
    pub cancelled: Arc<AtomicBool>,
    pub event_tx: mpsc::Sender<JsonRpcMessage>,
}

fn generate_session_id() -> String {
    format!("sess_{}", nanoid::nanoid!(12))
}

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Mutex<SessionState>>>>,
    config: Mutex<ApiClientConfig>,
}

impl SessionManager {
    pub fn new(config: ApiClientConfig) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            config: Mutex::new(config),
        }
    }

    pub async fn create_session(
        &self,
        cwd: String,
        event_tx: mpsc::Sender<JsonRpcMessage>,
    ) -> String {
        let session_id = generate_session_id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let state = SessionState {
            info: SessionInfo {
                session_id: session_id.clone(),
                cwd,
                created_at: now,
                message_count: 0,
            },
            history: Vec::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            event_tx,
        };
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), Arc::new(Mutex::new(state)));
        session_id
    }

    pub async fn destroy_session(&self, session_id: &str) -> Result<(), ()> {
        self.sessions
            .write()
            .await
            .remove(session_id)
            .map(|_| ())
            .ok_or(())
    }

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Option<Arc<Mutex<SessionState>>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions: Vec<Arc<Mutex<SessionState>>> = self
            .sessions
            .read()
            .await
            .values()
            .cloned()
            .collect();
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
}