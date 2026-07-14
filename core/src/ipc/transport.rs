use std::io;

use interprocess::local_socket::tokio::{Listener as TokioListener, Stream as TokioStream};
use interprocess::local_socket::traits::tokio::{Listener as ListenerTrait, Stream as StreamTrait};
use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::ipc::message::JsonRpcMessage;

const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

fn make_socket_name(path: &str) -> io::Result<interprocess::local_socket::Name<'_>> {
    path.to_fs_name::<GenericFilePath>()
}

fn get_user_id() -> String {
    #[cfg(unix)]
    {
        unsafe { libc::getuid().to_string() }
    }
    #[cfg(windows)]
    {
        std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string())
    }
}

pub fn default_socket_path() -> String {
    let uid = get_user_id();
    #[cfg(unix)]
    {
        format!("/tmp/crown-code-{}.sock", uid)
    }
    #[cfg(windows)]
    {
        format!("crown-code-{}", uid)
    }
}

pub fn resolve_socket_path(cli_override: Option<&str>) -> String {
    if let Some(p) = cli_override {
        return p.to_string();
    }
    if let Ok(p) = std::env::var("CROWN_SOCKET_PATH")
        && !p.is_empty()
    {
        return p;
    }
    default_socket_path()
}

pub struct IpcTransport {
    listener: TokioListener,
    socket_path: String,
}

impl IpcTransport {
    pub fn bind(socket_path: &str) -> Result<Self, IpcTransportError> {
        let name = make_socket_name(socket_path)
            .map_err(|e| IpcTransportError::BindFailed(e.to_string()))?;
        let listener = ListenerOptions::new()
            .name(name)
            .reclaim_name(true)
            .try_overwrite(true)
            .create_tokio()
            .map_err(|e| IpcTransportError::BindFailed(e.to_string()))?;
        Ok(Self {
            listener,
            socket_path: socket_path.to_string(),
        })
    }

    pub async fn accept(&self) -> Result<IpcConnection, IpcTransportError> {
        let stream = self
            .listener
            .accept()
            .await
            .map_err(|e| IpcTransportError::AcceptFailed(e.to_string()))?;
        Ok(IpcConnection::from_stream(stream))
    }

    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }
}

impl Drop for IpcTransport {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

pub struct IpcReadHalf {
    reader: BufReader<interprocess::local_socket::tokio::RecvHalf>,
}

pub struct IpcWriteHalf {
    writer: interprocess::local_socket::tokio::SendHalf,
}

impl IpcReadHalf {
    pub async fn read_message(&mut self) -> Result<Option<JsonRpcMessage>, IpcTransportError> {
        loop {
            let mut line = String::with_capacity(4096);
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| IpcTransportError::ReadError(e.to_string()))?;
            if n == 0 {
                return Ok(None);
            }
            if line.len() > MAX_MESSAGE_SIZE {
                return Err(IpcTransportError::ParseError(format!(
                    "message too large: {} bytes",
                    line.len()
                )));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let msg = JsonRpcMessage::from_line(trimmed)
                .map_err(|e| IpcTransportError::ParseError(e.to_string()))?;
            return Ok(Some(msg));
        }
    }
}

impl IpcWriteHalf {
    pub async fn write_message(&mut self, msg: &JsonRpcMessage) -> Result<(), IpcTransportError> {
        let line = msg.to_line();
        self.write_raw(&line).await
    }

    pub async fn write_raw(&mut self, data: &str) -> Result<(), IpcTransportError> {
        let payload = format!("{}\n", data);
        self.writer
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| IpcTransportError::WriteError(e.to_string()))?;
        self.writer
            .flush()
            .await
            .map_err(|e| IpcTransportError::WriteError(e.to_string()))?;
        Ok(())
    }
}

pub struct IpcConnection {
    read_half: IpcReadHalf,
    write_half: IpcWriteHalf,
}

impl IpcConnection {
    pub(crate) fn from_stream(stream: TokioStream) -> Self {
        let (recv_half, send_half) = stream.split();
        Self {
            read_half: IpcReadHalf {
                reader: BufReader::new(recv_half),
            },
            write_half: IpcWriteHalf { writer: send_half },
        }
    }

    pub fn split(self) -> (IpcReadHalf, IpcWriteHalf) {
        (self.read_half, self.write_half)
    }

    pub async fn read_message(&mut self) -> Result<Option<JsonRpcMessage>, IpcTransportError> {
        self.read_half.read_message().await
    }

    pub async fn write_message(&mut self, msg: &JsonRpcMessage) -> Result<(), IpcTransportError> {
        self.write_half.write_message(msg).await
    }

    pub async fn write_raw(&mut self, data: &str) -> Result<(), IpcTransportError> {
        self.write_half.write_raw(data).await
    }

    pub async fn connect(socket_path: &str) -> Result<Self, IpcTransportError> {
        let name = make_socket_name(socket_path)
            .map_err(|e| IpcTransportError::ConnectFailed(format!("invalid socket path: {e}")))?;
        let stream = TokioStream::connect(name)
            .await
            .map_err(|e| IpcTransportError::ConnectFailed(e.to_string()))?;
        Ok(Self::from_stream(stream))
    }
}

#[derive(Debug)]
pub enum IpcTransportError {
    BindFailed(String),
    AcceptFailed(String),
    ConnectFailed(String),
    ReadError(String),
    WriteError(String),
    ParseError(String),
}

impl std::fmt::Display for IpcTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcTransportError::BindFailed(msg) => write!(f, "bind failed: {msg}"),
            IpcTransportError::AcceptFailed(msg) => write!(f, "accept failed: {msg}"),
            IpcTransportError::ConnectFailed(msg) => write!(f, "connect failed: {msg}"),
            IpcTransportError::ReadError(msg) => write!(f, "read error: {msg}"),
            IpcTransportError::WriteError(msg) => write!(f, "write error: {msg}"),
            IpcTransportError::ParseError(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for IpcTransportError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::message::make_request;
    use interprocess::local_socket::GenericFilePath;
    use interprocess::local_socket::ToFsName;

    use serial_test::serial;

    fn unique_socket_path() -> String {
        format!("/tmp/crown-code-test-{}.sock", nanoid::nanoid!(12))
    }

    #[test]
    fn test_default_socket_path_contains_crown_code() {
        let path = default_socket_path();
        assert!(path.contains("crown-code"));
    }

    #[test]
    fn test_resolve_cli_override() {
        assert_eq!(
            resolve_socket_path(Some("/cli/path.sock")),
            "/cli/path.sock"
        );
    }

    #[serial]
    #[test]
    fn test_resolve_env_var() {
        let prev = std::env::var("CROWN_SOCKET_PATH").ok();
        unsafe {
            std::env::set_var("CROWN_SOCKET_PATH", "/env/path.sock");
        }
        let result = resolve_socket_path(None);
        assert_eq!(result, "/env/path.sock");
        if let Some(p) = prev {
            unsafe {
                std::env::set_var("CROWN_SOCKET_PATH", p);
            }
        } else {
            unsafe {
                std::env::remove_var("CROWN_SOCKET_PATH");
            }
        }
    }

    #[serial]
    #[test]
    fn test_resolve_default() {
        let prev = std::env::var("CROWN_SOCKET_PATH").ok();
        unsafe {
            std::env::remove_var("CROWN_SOCKET_PATH");
        }
        let result = resolve_socket_path(None);
        assert!(result.contains("crown-code"));
        if let Some(p) = prev {
            unsafe {
                std::env::set_var("CROWN_SOCKET_PATH", p);
            }
        }
    }

    #[tokio::test]
    async fn test_bind_and_accept() {
        let path = unique_socket_path();
        let transport = IpcTransport::bind(&path).unwrap();

        let client = tokio::spawn(async move {
            let name = path.to_fs_name::<GenericFilePath>().expect("fs name");
            TokioStream::connect(name).await.expect("connect");
        });

        let conn = transport.accept().await;
        assert!(conn.is_ok());
        client.await.unwrap();
    }

    #[tokio::test]
    async fn test_read_write_roundtrip() {
        let path = unique_socket_path();
        let transport = IpcTransport::bind(&path).unwrap();

        let client = tokio::spawn(async move {
            let name = path.to_fs_name::<GenericFilePath>().expect("fs name");
            let stream = TokioStream::connect(name).await.expect("connect");
            let mut conn = IpcConnection::from_stream(stream);
            let msg = JsonRpcMessage::from_line(
                r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
            )
            .unwrap();
            conn.write_message(&msg).await.unwrap();
        });

        let mut conn = transport.accept().await.unwrap();
        let msg = conn.read_message().await.unwrap().unwrap();
        assert_eq!(msg.id, Some(1));
        assert_eq!(msg.method.as_deref(), Some("ping"));

        client.await.unwrap();
    }

    #[tokio::test]
    async fn test_read_eof() {
        let path = unique_socket_path();
        let transport = IpcTransport::bind(&path).unwrap();

        let client = tokio::spawn(async move {
            let name = path.to_fs_name::<GenericFilePath>().expect("fs name");
            let _stream = TokioStream::connect(name).await.expect("connect");
        });

        let mut conn = transport.accept().await.unwrap();
        let msg = conn.read_message().await.unwrap();
        assert!(msg.is_none());

        client.await.unwrap();
    }

    #[tokio::test]
    async fn test_read_malformed_json() {
        let path = unique_socket_path();
        let transport = IpcTransport::bind(&path).unwrap();

        let client = tokio::spawn(async move {
            let name = path.to_fs_name::<GenericFilePath>().expect("fs name");
            let stream = TokioStream::connect(name).await.expect("connect");
            let mut conn = IpcConnection::from_stream(stream);
            conn.write_raw("not valid json").await.unwrap();
        });

        let mut conn = transport.accept().await.unwrap();
        let result = conn.read_message().await;
        assert!(matches!(result, Err(IpcTransportError::ParseError(_))));

        client.await.unwrap();
    }

    #[tokio::test]
    async fn test_connect_to_listener() {
        let path = unique_socket_path();
        let transport = IpcTransport::bind(&path).unwrap();

        let handle = tokio::spawn(async move {
            let mut conn = IpcConnection::connect(&path).await.unwrap();
            let msg = make_request(42, "ping", serde_json::json!({}));
            conn.write_message(&msg).await.unwrap();
        });

        let mut server_conn = transport.accept().await.unwrap();
        let msg = server_conn.read_message().await.unwrap().unwrap();
        assert_eq!(msg.id, Some(42));
        assert_eq!(msg.method.as_deref(), Some("ping"));

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_connect_nonexistent() {
        let result = IpcConnection::connect("/tmp/nonexistent.sock").await;
        assert!(matches!(result, Err(IpcTransportError::ConnectFailed(_))));
    }

    #[tokio::test]
    async fn test_split_read_write() {
        let path = unique_socket_path();
        let transport = IpcTransport::bind(&path).unwrap();

        let handle = tokio::spawn(async move {
            let conn = IpcConnection::connect(&path).await.unwrap();
            let (mut read_half, mut write_half) = conn.split();
            let msg = make_request(7, "echo", serde_json::json!({"data": "hello"}));
            write_half.write_message(&msg).await.unwrap();
            let response = read_half.read_message().await.unwrap().unwrap();
            assert_eq!(response.id, Some(7));
        });

        let mut server_conn = transport.accept().await.unwrap();
        let msg = server_conn.read_message().await.unwrap().unwrap();
        assert_eq!(msg.method.as_deref(), Some("echo"));
        let response = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: Some(7),
            method: None,
            params: None,
            result: Some(serde_json::json!({"echo": "hello"})),
            error: None,
        };
        server_conn.write_message(&response).await.unwrap();

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_concurrent_clients() {
        let path = unique_socket_path();
        let transport = IpcTransport::bind(&path).unwrap();

        let mut client_handles = Vec::new();
        for i in 0..3 {
            let p = path.clone();
            client_handles.push(tokio::spawn(async move {
                let name = p.to_fs_name::<GenericFilePath>().expect("fs name");
                let stream = TokioStream::connect(name).await.expect("connect");
                let mut conn = IpcConnection::from_stream(stream);
                let msg = make_ping(i);
                conn.write_message(&msg).await.unwrap();
            }));
        }

        for _ in 0..3 {
            let mut conn = transport.accept().await.unwrap();
            let msg = conn.read_message().await.unwrap().unwrap();
            assert_eq!(msg.method.as_deref(), Some("ping"));
        }

        for h in client_handles {
            h.await.unwrap();
        }
    }

    fn make_ping(id: u64) -> JsonRpcMessage {
        make_request(id, "ping", serde_json::json!({}))
    }
}
