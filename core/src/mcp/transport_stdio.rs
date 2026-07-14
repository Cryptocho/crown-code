use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

use crate::command_exec::CircularBuffer;

pub const MCP_LINE_BUF_SIZE: usize = 1_048_576;
pub const DEFAULT_LINE_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    Ok,
    Timeout,
    WriteError,
    ReadError,
    Eof,
    SpawnFailed,
}

#[derive(Debug)]
pub struct ReadLineResult {
    pub line: String,
    pub error: TransportError,
}

pub struct StdioTransport {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr_buf: Arc<CircularBuffer>,
    pub last_error: String,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for StdioTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioTransport")
            .field("child", &self.child.id())
            .field("last_error", &self.last_error)
            .finish()
    }
}

pub fn start_stdio_transport(command: &str, args: &[&str]) -> Result<StdioTransport, String> {
    if command.is_empty() {
        return Err("command is empty".to_string());
    }

    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {}", e))?;

    let stdin = child.stdin.take().ok_or("stdin not available")?;
    let stdout = child.stdout.take().ok_or("stdout not available")?;
    let stderr = child.stderr.take().ok_or("stderr not available")?;

    let stderr_buf = Arc::new(CircularBuffer::new());
    let buf_clone = Arc::clone(&stderr_buf);

    let stderr_task = tokio::spawn(async move {
        let mut reader = TokioBufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => buf_clone.push(line.trim_end()),
                Err(_) => break,
            }
        }
    });

    Ok(StdioTransport {
        child,
        stdin,
        stdout,
        stderr_buf,
        last_error: String::new(),
        stderr_task: Some(stderr_task),
    })
}

pub async fn read_json_line(t: &mut StdioTransport, timeout_ms: u64) -> ReadLineResult {
    let mut reader = TokioBufReader::new(&mut t.stdout);
    let mut line = String::with_capacity(4096);
    let dur = Duration::from_millis(timeout_ms);
    match timeout(dur, reader.read_line(&mut line)).await {
        Ok(Ok(0)) => ReadLineResult {
            line: String::new(),
            error: TransportError::Eof,
        },
        Ok(Ok(_)) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            ReadLineResult {
                line,
                error: TransportError::Ok,
            }
        }
        Ok(Err(_)) => ReadLineResult {
            line: String::new(),
            error: TransportError::ReadError,
        },
        Err(_) => ReadLineResult {
            line: String::new(),
            error: TransportError::Timeout,
        },
    }
}

pub async fn write_json_line(t: &mut StdioTransport, line: &str) -> TransportError {
    let payload = format!("{}\n", line);
    match t.stdin.write_all(payload.as_bytes()).await {
        Ok(_) => match t.stdin.flush().await {
            Ok(_) => TransportError::Ok,
            Err(_) => TransportError::WriteError,
        },
        Err(_) => TransportError::WriteError,
    }
}

pub async fn close(t: &mut StdioTransport) {
    let _ = t.child.kill().await;
    let _ = t.child.wait().await;
    if let Some(handle) = t.stderr_task.take() {
        handle.abort();
    }
}

pub fn get_stderr(t: &StdioTransport) -> String {
    t.stderr_buf.join()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_command_returns_error() {
        let result = start_stdio_transport("", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_start_true_command() {
        let mut transport = start_stdio_transport("true", &[]).expect("should spawn 'true'");
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_read_timeout() {
        let mut transport =
            start_stdio_transport("sleep", &["10"]).expect("should spawn 'sleep 10'");
        let result = read_json_line(&mut transport, 100).await;
        assert_eq!(result.error, TransportError::Timeout);
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_close_empty_transport() {
        let mut transport = start_stdio_transport("true", &[]).expect("should spawn 'true'");
        close(&mut transport).await;
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_resource_cleanup() {
        let mut transport =
            start_stdio_transport("echo", &["hello"]).expect("should spawn 'echo hello'");
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_process_termination() {
        let mut transport =
            start_stdio_transport("sleep", &["10"]).expect("should spawn 'sleep 10'");
        close(&mut transport).await;
        let status = transport.child.try_wait();
        assert!(status.is_ok());
        assert!(status.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_read_json_line_normal() {
        let mut transport =
            start_stdio_transport("echo", &["{\"jsonrpc\":\"2.0\"}"]).expect("should spawn");
        let result = read_json_line(&mut transport, 5000).await;
        assert_eq!(result.error, TransportError::Ok);
        assert_eq!(result.line, "{\"jsonrpc\":\"2.0\"}");
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_read_json_line_strips_crlf() {
        let mut transport =
            start_stdio_transport("printf", &["line\r\n"]).expect("should spawn printf");
        let result = read_json_line(&mut transport, 5000).await;
        assert_eq!(result.error, TransportError::Ok);
        assert_eq!(result.line, "line");
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_write_json_line_success() {
        let mut transport = start_stdio_transport("cat", &[]).expect("should spawn cat");
        let err = write_json_line(&mut transport, "{\"test\":1}").await;
        assert_eq!(err, TransportError::Ok);
        let result = read_json_line(&mut transport, 5000).await;
        assert_eq!(result.error, TransportError::Ok);
        assert_eq!(result.line, "{\"test\":1}");
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_write_json_line_error() {
        let mut transport = start_stdio_transport("true", &[]).expect("should spawn true");
        transport.child.wait().await.unwrap();
        let err = write_json_line(&mut transport, "test").await;
        assert_eq!(err, TransportError::WriteError);
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_read_json_line_eof() {
        let mut transport = start_stdio_transport("true", &[]).expect("should spawn true");
        transport.child.wait().await.unwrap();
        let result = read_json_line(&mut transport, 5000).await;
        assert_eq!(result.error, TransportError::Eof);
        close(&mut transport).await;
    }

    #[tokio::test]
    async fn test_get_stderr_captures_output() {
        let mut transport =
            start_stdio_transport("bash", &["-c", "echo err >&2"]).expect("should spawn bash");
        transport.child.wait().await.unwrap();
        if let Some(handle) = transport.stderr_task.take() {
            let _ = handle.await;
        }
        close(&mut transport).await;
        let stderr = get_stderr(&transport);
        assert!(stderr.contains("err"));
    }

    #[tokio::test]
    async fn test_stderr_concurrent_with_stdout() {
        let mut transport = start_stdio_transport("bash", &["-c", "echo out; echo err >&2"])
            .expect("should spawn bash");
        let result = read_json_line(&mut transport, 5000).await;
        assert_eq!(result.error, TransportError::Ok);
        assert_eq!(result.line.trim(), "out");
        transport.child.wait().await.unwrap();
        if let Some(handle) = transport.stderr_task.take() {
            let _ = handle.await;
        }
        close(&mut transport).await;
        let stderr = get_stderr(&transport);
        assert!(stderr.contains("err"));
    }
}
