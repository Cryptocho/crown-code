use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{Mutex, mpsc, watch};
use tokio::task::JoinHandle;

use crate::api::types::ApiClientConfig;
use crate::ipc::message::{
    JsonRpcMessage, METHOD_CANCEL, METHOD_CREATE_SESSION, METHOD_DESTROY_SESSION,
    METHOD_LIST_SESSIONS, METHOD_NOT_FOUND, METHOD_SET_AGENT_MODE, METHOD_SET_CONFIG,
    METHOD_USER_MESSAGE, SESSION_NOT_FOUND, make_error_response, make_response,
};
use crate::ipc::session_manager::{IpcEventHandler, SessionManager};
use crate::ipc::transport::{IpcConnection, IpcTransport, IpcTransportError};

pub struct IpcServer {
    transport: IpcTransport,
    session_manager: Arc<SessionManager>,
    shutdown_tx: watch::Sender<bool>,
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl IpcServer {
    pub fn new(socket_path: &str, config: ApiClientConfig) -> Result<Self, IpcTransportError> {
        let transport = IpcTransport::bind(socket_path)?;
        let (shutdown_tx, _) = watch::channel(false);
        Ok(Self {
            transport,
            session_manager: Arc::new(SessionManager::new(config)),
            shutdown_tx,
            handles: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn socket_path(&self) -> &str {
        self.transport.socket_path()
    }

    pub async fn run(&self) -> Result<(), IpcTransportError> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        loop {
            tokio::select! {
                result = self.transport.accept() => {
                    match result {
                        Ok(conn) => {
                            let sm = Arc::clone(&self.session_manager);
                            let rx = self.shutdown_tx.subscribe();
                            let handle = tokio::spawn(handle_connection(conn, sm, rx));
                            self.handles.lock().await.push(handle);
                        }
                        Err(e) => {
                            eprintln!("accept error: {e}");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() { break; }
                }
            }
        }
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), IpcTransportError> {
        let mut handles = self.handles.lock().await;
        let _ = self.shutdown_tx.send(true);
        for h in handles.drain(..) {
            let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
        }
        Ok(())
    }
}

struct OwnedSessions {
    inner: HashSet<String>,
}

impl OwnedSessions {
    fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }

    fn insert(&mut self, id: String) {
        self.inner.insert(id);
    }

    fn remove(&mut self, id: &str) {
        self.inner.remove(id);
    }

    fn contains(&self, id: &str) -> bool {
        self.inner.contains(id)
    }
}

async fn handle_connection(
    mut conn: IpcConnection,
    session_manager: Arc<SessionManager>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let (event_tx, mut event_rx) = mpsc::channel::<JsonRpcMessage>(256);
    let mut owned_sessions = OwnedSessions::new();

    loop {
        tokio::select! {
            biased;

            msg = conn.read_message() => {
                match msg {
                    Ok(Some(rpc_msg)) => {
                        if rpc_msg.is_request() {
                            let resp = dispatch_request(
                                &rpc_msg, &session_manager,
                                &event_tx, &mut owned_sessions,
                            ).await;
                            let _ = conn.write_message(&resp).await;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => { eprintln!("read error: {e}"); break; }
                }
            }
            event = event_rx.recv() => {
                match event {
                    Some(evt) => { let _ = conn.write_message(&evt).await; }
                    None => break,
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() { break; }
            }
        }
    }
}

async fn dispatch_request(
    msg: &JsonRpcMessage,
    sm: &SessionManager,
    event_tx: &mpsc::Sender<JsonRpcMessage>,
    owned_sessions: &mut OwnedSessions,
) -> JsonRpcMessage {
    let id = msg.id.unwrap_or(0);
    let method = msg.method.as_deref().unwrap_or("");
    let params = msg.params.as_ref().unwrap_or(&Value::Null);

    match method {
        METHOD_CREATE_SESSION => {
            let cwd = params["cwd"].as_str().unwrap_or(".").to_string();
            let sid = sm.create_session(cwd, event_tx.clone()).await;
            owned_sessions.insert(sid.clone());
            make_response(id, serde_json::json!({"session_id": sid}))
        }
        METHOD_LIST_SESSIONS => {
            let sessions = sm.list_sessions().await;
            make_response(id, serde_json::json!({"sessions": sessions}))
        }
        METHOD_DESTROY_SESSION => {
            let sid = params["session_id"].as_str().unwrap_or("");
            match sm.destroy_session(sid).await {
                Ok(()) => {
                    owned_sessions.remove(sid);
                    make_response(id, serde_json::json!({"ok": true}))
                }
                Err(()) => make_error_response(id, SESSION_NOT_FOUND, "session not found", None),
            }
        }
        METHOD_USER_MESSAGE => {
            let sid = params["session_id"].as_str().unwrap_or("");
            let content = params["content"].as_str().unwrap_or("");
            if !owned_sessions.contains(sid) {
                return make_error_response(
                    id,
                    SESSION_NOT_FOUND,
                    "session not found or not owned",
                    None,
                );
            }
            let session = match sm.get_session(sid).await {
                Some(s) => s,
                None => {
                    return make_error_response(id, SESSION_NOT_FOUND, "session not found", None);
                }
            };

            let sid_owned = sid.to_string();
            let content_owned = content.to_string();

            tokio::spawn(async move {
                let mut state = session.lock().await;
                let mut handler = IpcEventHandler::new(sid_owned.clone(), state.event_tx.clone());
                state
                    .agent
                    .handle_user_message(&content_owned, &mut handler)
                    .await;
                state.info.message_count += 1;
            });

            make_response(id, serde_json::json!({"ok": true}))
        }
        METHOD_CANCEL => {
            let sid = params["session_id"].as_str().unwrap_or("");
            if sm.cancel_session(sid).await {
                make_response(id, serde_json::json!({"ok": true}))
            } else {
                make_error_response(id, SESSION_NOT_FOUND, "session not found", None)
            }
        }
        METHOD_SET_CONFIG => {
            let mut config = sm.get_config().await;
            if let Some(v) = params.get("base_url").and_then(|v| v.as_str()) {
                config.base_url = v.to_string();
            }
            if let Some(v) = params.get("api_key").and_then(|v| v.as_str()) {
                config.api_key = v.to_string();
            }
            if let Some(v) = params.get("model").and_then(|v| v.as_str()) {
                config.model = v.to_string();
            }
            if let Some(v) = params.get("temperature").and_then(|v| v.as_f64()) {
                config.temperature = v;
            }
            if let Some(v) = params.get("max_tokens").and_then(|v| v.as_i64()) {
                config.max_tokens = v as i32;
            }
            sm.update_config(config).await;
            make_response(id, serde_json::json!({"ok": true}))
        }
        METHOD_SET_AGENT_MODE => {
            make_response(id, serde_json::json!({"ok": true}))
        }
        _ => make_error_response(
            id,
            METHOD_NOT_FOUND,
            &format!("unknown method: {method}"),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::message::{METHOD_ASSISTANT_TEXT, METHOD_TASK_DONE, make_request};
    use interprocess::local_socket::ToFsName;
    use interprocess::local_socket::traits::tokio::Stream as StreamTrait;

    struct TestClient {
        conn: IpcConnection,
    }

    impl TestClient {
        async fn connect(path: &str) -> Self {
            let name = path
                .to_fs_name::<interprocess::local_socket::GenericFilePath>()
                .expect("fs name");
            let stream = interprocess::local_socket::tokio::Stream::connect(name)
                .await
                .expect("connect");
            Self {
                conn: IpcConnection::from_stream(stream),
            }
        }

        async fn send_request(&mut self, method: &str, params: Value) -> JsonRpcMessage {
            let msg = make_request(1, method, params);
            self.conn.write_message(&msg).await.unwrap();
            self.conn.read_message().await.unwrap().unwrap()
        }

        async fn send_request_with_id(
            &mut self,
            id: u64,
            method: &str,
            params: Value,
        ) -> JsonRpcMessage {
            let msg = make_request(id, method, params);
            self.conn.write_message(&msg).await.unwrap();
            self.conn.read_message().await.unwrap().unwrap()
        }

        #[allow(dead_code)]
        async fn read_event(&mut self) -> JsonRpcMessage {
            self.conn.read_message().await.unwrap().unwrap()
        }

        #[allow(dead_code)]
        async fn read_event_timeout(&mut self, timeout: Duration) -> Option<JsonRpcMessage> {
            tokio::time::timeout(timeout, self.conn.read_message())
                .await
                .ok()
                .and_then(|r| r.unwrap())
        }
    }

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

    fn unique_socket_path() -> String {
        format!("/tmp/crown-code-test-{}.sock", nanoid::nanoid!(12))
    }

    #[tokio::test]
    async fn test_bind_connect() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_create_session_response() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await;
        let result = resp.result.unwrap();
        assert!(result["session_id"].as_str().unwrap().starts_with("sess_"));
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        let binding = resp.result.unwrap();
        let sessions = binding["sessions"].as_array().unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_list_after_create() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let _create_resp = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await;
        let resp = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        let binding = resp.result.unwrap();
        let sessions = binding["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_destroy_session() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let create_resp = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await;
        let binding = create_resp.result.unwrap();
        let sid = binding["session_id"].as_str().unwrap().to_string();

        let resp = client
            .send_request("destroy_session", serde_json::json!({"session_id": sid}))
            .await;
        assert_eq!(resp.result.unwrap()["ok"], true);

        let list_resp = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        let binding = list_resp.result.unwrap();
        let sessions = binding["sessions"].as_array().unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request("nonexistent_method", serde_json::json!({}))
            .await;
        let err = resp.error.unwrap();
        assert_eq!(err.code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_malformed_json() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let name = path
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .expect("fs name");
        let stream = interprocess::local_socket::tokio::Stream::connect(name)
            .await
            .expect("connect");
        let mut conn = IpcConnection::from_stream(stream);
        conn.write_raw("not valid json").await.unwrap();

        let result = conn.read_message().await;
        assert!(result.is_err() || result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cancel() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let create_resp = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await;
        let binding = create_resp.result.unwrap();
        let sid = binding["session_id"].as_str().unwrap().to_string();

        let resp = client
            .send_request("cancel", serde_json::json!({"session_id": sid}))
            .await;
        assert_eq!(resp.result.unwrap()["ok"], true);

        let resp = client
            .send_request("cancel", serde_json::json!({"session_id": "nonexistent"}))
            .await;
        assert!(resp.error.is_some());
    }

    #[tokio::test]
    async fn test_user_message_unowned_session() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request(
                "user_message",
                serde_json::json!({"session_id": "sess_nonexistent", "content": "hi"}),
            )
            .await;
        let err = resp.error.unwrap();
        assert_eq!(err.code, SESSION_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_set_config_partial() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;

        let resp = client
            .send_request(
                "set_config",
                serde_json::json!({"model": "new-model", "temperature": 0.9}),
            )
            .await;
        assert_eq!(resp.result.unwrap()["ok"], true);
    }

    #[tokio::test]
    async fn test_multi_session_single_connection() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let r1 = client
            .send_request("create_session", serde_json::json!({"cwd": "/a"}))
            .await;
        let r2 = client
            .send_request("create_session", serde_json::json!({"cwd": "/b"}))
            .await;
        let binding1 = r1.result.unwrap();
        let binding2 = r2.result.unwrap();
        let sid1 = binding1["session_id"].as_str().unwrap().to_string();
        let sid2 = binding2["session_id"].as_str().unwrap().to_string();
        assert_ne!(sid1, sid2);

        let list = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        let binding = list.result.unwrap();
        let sessions = binding["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_clients() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut handles = Vec::new();
        for i in 0..3 {
            let p = path.clone();
            handles.push(tokio::spawn(async move {
                let mut client = TestClient::connect(&p).await;
                let resp = client
                    .send_request_with_id(
                        i,
                        "create_session",
                        serde_json::json!({"cwd": format!("/tmp/{}", i)}),
                    )
                    .await;
                let result = resp.result.unwrap();
                assert!(result["session_id"].as_str().unwrap().starts_with("sess_"));
            }));
        }

        for h in handles {
            h.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_connection_drop_sessions_survive() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let sm = Arc::clone(&server.session_manager);
        let (tx, _rx) = mpsc::channel(256);
        let sid = sm.create_session("/tmp".to_string(), tx).await;

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let list = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        let binding = list.result.unwrap();
        let sessions = binding["sessions"].as_array().unwrap();
        assert!(!sessions.is_empty());
        assert!(sessions.iter().any(|s| s["session_id"] == sid));
    }

    #[tokio::test]
    async fn test_destroy_nonexistent_session() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request(
                "destroy_session",
                serde_json::json!({"session_id": "sess_fake"}),
            )
            .await;
        let err = resp.error.unwrap();
        assert_eq!(err.code, SESSION_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_set_config_all_fields() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request(
                "set_config",
                serde_json::json!({
                    "base_url": "http://x",
                    "api_key": "k",
                    "model": "m",
                    "temperature": 0.5,
                    "max_tokens": 1024,
                }),
            )
            .await;
        assert_eq!(resp.result.unwrap()["ok"], true);

        let cfg = server.session_manager.get_config().await;
        assert_eq!(cfg.base_url, "http://x");
        assert_eq!(cfg.api_key, "k");
        assert_eq!(cfg.model, "m");
        assert_eq!(cfg.temperature, 0.5);
        assert_eq!(cfg.max_tokens, 1024);
    }

    #[tokio::test]
    async fn test_set_config_no_fields() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request("set_config", serde_json::json!({}))
            .await;
        assert_eq!(resp.result.unwrap()["ok"], true);
    }

    #[tokio::test]
    async fn test_create_session_default_cwd() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request("create_session", serde_json::json!({}))
            .await;
        assert!(
            resp.result.unwrap()["session_id"]
                .as_str()
                .unwrap()
                .starts_with("sess_")
        );

        let list = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        let binding = list.result.unwrap();
        let sessions = binding["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["cwd"], ".");
    }

    #[tokio::test]
    async fn test_server_shutdown() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        let handle = tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        assert!(resp.result.is_some());

        server.shutdown().await.unwrap();

        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("server should shut down")
            .unwrap();
    }

    #[tokio::test]
    async fn test_set_agent_mode() {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = TestClient::connect(&path).await;
        let resp = client
            .send_request(
                "set_agent_mode",
                serde_json::json!({"session_id": "sess_test", "mode": "code"}),
            )
            .await;
        assert_eq!(resp.result.unwrap()["ok"], true);
    }
}
