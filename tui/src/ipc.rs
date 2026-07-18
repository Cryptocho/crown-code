use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};

use crown_core::ipc::message::{JsonRpcMessage, make_notification, make_request};
use crown_core::ipc::transport::{IpcConnection, IpcReadHalf, IpcTransportError, IpcWriteHalf};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum IpcError {
    Transport(IpcTransportError),
    Disconnected,
    RequestTimeout,
    RpcError { code: i32, message: String },
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::Transport(e) => write!(f, "transport error: {e}"),
            IpcError::Disconnected => write!(f, "disconnected"),
            IpcError::RequestTimeout => write!(f, "request timeout"),
            IpcError::RpcError { code, message } => write!(f, "rpc error {code}: {message}"),
        }
    }
}

impl std::error::Error for IpcError {}

impl From<IpcTransportError> for IpcError {
    fn from(e: IpcTransportError) -> Self {
        IpcError::Transport(e)
    }
}

pub struct IpcClient {
    write_tx: mpsc::UnboundedSender<JsonRpcMessage>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcMessage>>>>,
    next_id: Arc<AtomicU64>,
    connected: Arc<AtomicBool>,
}

impl std::fmt::Debug for IpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcClient")
            .field("next_id", &self.next_id.load(Ordering::SeqCst))
            .field("connected", &self.connected.load(Ordering::SeqCst))
            .finish()
    }
}

pub struct IpcEventReader {
    incoming_rx: mpsc::UnboundedReceiver<JsonRpcMessage>,
    connected: Arc<AtomicBool>,
}

impl std::fmt::Debug for IpcEventReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcEventReader")
            .field("connected", &self.connected.load(Ordering::SeqCst))
            .finish()
    }
}

impl IpcClient {
    pub async fn connect(socket_path: &str) -> Result<(Self, IpcEventReader), IpcError> {
        let conn = IpcConnection::connect(socket_path).await?;
        let (read_half, write_half) = conn.split();

        let (write_tx, write_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<JsonRpcMessage>();
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcMessage>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let connected = Arc::new(AtomicBool::new(true));

        spawn_read_task(
            read_half,
            incoming_tx,
            Arc::clone(&pending),
            Arc::clone(&connected),
        );
        spawn_write_task(write_half, write_rx, Arc::clone(&connected));

        let client = IpcClient {
            write_tx,
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
            connected: Arc::clone(&connected),
        };
        let reader = IpcEventReader {
            incoming_rx,
            connected,
        };
        Ok((client, reader))
    }

    pub async fn send_request(&self, method: &str, params: Value) -> Result<Value, IpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = make_request(id, method, params);

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        if self.write_tx.send(msg).is_err() {
            self.pending.lock().await.remove(&id);
            return Err(IpcError::Disconnected);
        }

        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(resp)) => {
                if let Some(err) = resp.error {
                    Err(IpcError::RpcError {
                        code: err.code,
                        message: err.message,
                    })
                } else {
                    Ok(resp.result.unwrap_or(Value::Null))
                }
            }
            Ok(Err(_)) => Err(IpcError::Disconnected),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(IpcError::RequestTimeout)
            }
        }
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), IpcError> {
        let msg = make_notification(method, params);
        self.write_tx
            .send(msg)
            .map_err(|_| IpcError::Disconnected)?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

impl IpcEventReader {
    pub async fn read_message(&mut self) -> Option<JsonRpcMessage> {
        self.incoming_rx.recv().await
    }
}

fn spawn_read_task(
    mut read_half: IpcReadHalf,
    incoming_tx: mpsc::UnboundedSender<JsonRpcMessage>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcMessage>>>>,
    connected: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        loop {
            match read_half.read_message().await {
                Ok(Some(msg)) => {
                    if msg.is_response() {
                        let id = msg.id.unwrap();
                        let mut map = pending.lock().await;
                        if let Some(tx) = map.remove(&id) {
                            let _ = tx.send(msg);
                        }
                    } else {
                        if incoming_tx.send(msg).is_err() {
                            break;
                        }
                    }
                }
                Ok(None) => {
                    connected.store(false, Ordering::SeqCst);
                    break;
                }
                Err(_) => {
                    connected.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
    });
}

fn spawn_write_task(
    mut write_half: IpcWriteHalf,
    mut write_rx: mpsc::UnboundedReceiver<JsonRpcMessage>,
    connected: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if write_half.write_message(&msg).await.is_err() {
                connected.store(false, Ordering::SeqCst);
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crown_core::api::types::ApiClientConfig;
    use crown_core::ipc::server::IpcServer;

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

    async fn start_server() -> (Arc<IpcServer>, String) {
        let path = unique_socket_path();
        let server = Arc::new(IpcServer::new(&path, make_config()).unwrap());
        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = srv.run().await;
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        (server, path)
    }

    #[tokio::test]
    async fn test_connect_and_create_session() {
        let (_server, path) = start_server().await;

        let (client, _reader) = IpcClient::connect(&path).await.unwrap();
        let result = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        let session_id = value.get("session_id").unwrap().as_str().unwrap();
        assert!(session_id.starts_with("sess_"));
        assert!(client.is_connected());
    }

    #[tokio::test]
    async fn test_send_notification() {
        let (_server, path) = start_server().await;

        let (client, _reader) = IpcClient::connect(&path).await.unwrap();
        let notif_result = client
            .send_notification("set_config", serde_json::json!({"model": "new_model"}))
            .await;
        assert!(notif_result.is_ok());

        let result = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_request_error_response() {
        let (_server, path) = start_server().await;

        let (client, _reader) = IpcClient::connect(&path).await.unwrap();
        let result = client
            .send_request("nonexistent_method", serde_json::json!({}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            IpcError::RpcError { message, .. } => {
                assert!(message.contains("unknown method"), "got: {message}");
            }
            other => panic!("expected RpcError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_send_request_after_disconnect() {
        let (_server, path) = start_server().await;

        let (client, _reader) = IpcClient::connect(&path).await.unwrap();
        assert!(client.is_connected());

        let result = client
            .send_request("list_sessions", serde_json::json!({}))
            .await;
        assert!(result.is_ok());

        let _ = tokio::time::timeout(Duration::from_secs(6), _server.shutdown()).await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_read_message_returns_none_on_disconnect() {
        let (_server, path) = start_server().await;

        let (client, mut reader) = IpcClient::connect(&path).await.unwrap();

        drop(client);

        let msg = tokio::time::timeout(Duration::from_secs(5), reader.read_message()).await;
        assert!(msg.is_ok());
        assert!(msg.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_connect_to_nonexistent_socket() {
        let result = IpcClient::connect("/tmp/nonexistent.sock").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            IpcError::Transport(IpcTransportError::ConnectFailed(_)) => {}
            other => panic!("expected ConnectFailed, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_multiple_concurrent_requests() {
        let (_server, path) = start_server().await;

        let (client, _reader) = IpcClient::connect(&path).await.unwrap();
        let client = Arc::new(client);

        let (r1, r2) = tokio::join!(
            client.send_request("create_session", serde_json::json!({"cwd": "/tmp"})),
            client.send_request("create_session", serde_json::json!({"cwd": "/tmp"})),
        );
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn test_full_roundtrip_user_message_and_events() {
        let (_server, path) = start_server().await;

        let (client, mut reader) = IpcClient::connect(&path).await.unwrap();

        let result = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap().to_string();

        let resp = client
            .send_request(
                "user_message",
                serde_json::json!({
                    "session_id": session_id,
                    "content": "hello",
                }),
            )
            .await
            .unwrap();
        assert_eq!(resp["ok"], true);

        let mut got_event = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_secs(2), reader.read_message()).await {
                Ok(Some(msg)) => {
                    if msg.is_notification() {
                        got_event = true;
                        let method = msg.method.as_deref().unwrap_or("");
                        assert!(
                            [
                                "assistant_text",
                                "error",
                                "tool_call_start",
                                "usage",
                                "task_done"
                            ]
                            .contains(&method),
                            "unexpected event method: {method}"
                        );
                        if let Some(params) = &msg.params {
                            assert_eq!(params["session_id"].as_str(), Some(session_id.as_str()));
                        }
                        if method == "error" || method == "task_done" {
                            break;
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => {} // timeout, keep waiting until deadline
            }
        }
        assert!(
            got_event,
            "should have received at least one event from core"
        );
    }

    #[tokio::test]
    async fn test_cancel_during_agent_loop() {
        let (_server, path) = start_server().await;

        let (client, mut reader) = IpcClient::connect(&path).await.unwrap();
        let result = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap().to_string();

        let _ = client
            .send_request(
                "user_message",
                serde_json::json!({
                    "session_id": session_id,
                    "content": "write a very long essay",
                }),
            )
            .await;

        tokio::time::sleep(Duration::from_millis(50)).await;
        let resp = client
            .send_request(
                "cancel",
                serde_json::json!({
                    "session_id": session_id,
                }),
            )
            .await
            .unwrap();
        assert_eq!(resp["ok"], true);

        let mut got_cancel = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_secs(2), reader.read_message()).await {
                Ok(Some(msg)) => {
                    if msg.method.as_deref() == Some("error") {
                        if let Some(params) = &msg.params {
                            if params["message"].as_str() == Some("task cancelled") {
                                got_cancel = true;
                                break;
                            }
                        }
                    }
                    if msg.method.as_deref() == Some("task_done") {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => {} // timeout, keep waiting until deadline
            }
        }
        assert!(
            got_cancel,
            "should have received task cancelled error event"
        );
    }

    #[tokio::test]
    async fn test_create_and_destroy_session_roundtrip() {
        let (_server, path) = start_server().await;

        let (client, _reader) = IpcClient::connect(&path).await.unwrap();

        let create_result = client
            .send_request("create_session", serde_json::json!({"cwd": "/tmp"}))
            .await
            .unwrap();
        let session_id = create_result["session_id"].as_str().unwrap().to_string();
        assert!(session_id.starts_with("sess_"));

        let list_result = client
            .send_request("list_sessions", serde_json::json!({}))
            .await
            .unwrap();
        let sessions = list_result["sessions"].as_array().unwrap();
        assert!(!sessions.is_empty());

        let destroy_result = client
            .send_request(
                "destroy_session",
                serde_json::json!({"session_id": session_id}),
            )
            .await
            .unwrap();
        assert_eq!(destroy_result["ok"], true);
    }

    #[tokio::test]
    async fn test_cancel_unknown_session_returns_error() {
        let (_server, path) = start_server().await;

        let (client, _reader) = IpcClient::connect(&path).await.unwrap();
        let result = client
            .send_request(
                "cancel",
                serde_json::json!({"session_id": "sess_nonexistent"}),
            )
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            IpcError::RpcError { code, .. } => {
                assert_eq!(code, crown_core::ipc::message::SESSION_NOT_FOUND);
            }
            other => panic!("expected RpcError, got: {other:?}"),
        }
    }
}
