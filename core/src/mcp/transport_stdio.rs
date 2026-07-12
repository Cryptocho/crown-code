use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::AsRawFd;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::thread;

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
    stderr_thread: Option<thread::JoinHandle<()>>,
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

    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => buf_clone.push(&l),
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
        stderr_thread: Some(stderr_thread),
    })
}

pub fn read_json_line(t: &mut StdioTransport, timeout_ms: u64) -> ReadLineResult {
    let fd = t.stdout.as_raw_fd();
    let mut reader = BufReader::new(&mut t.stdout);
    let mut line = String::with_capacity(4096);

    let timeout = if timeout_ms > i32::MAX as u64 {
        i32::MAX
    } else {
        timeout_ms as i32
    };

    let mut poll_fds = [libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    }];

    let ret = unsafe { libc::poll(poll_fds.as_mut_ptr(), 1, timeout) };

    if ret < 0 {
        return ReadLineResult {
            line: String::new(),
            error: TransportError::ReadError,
        };
    }
    if ret == 0 {
        return ReadLineResult {
            line: String::new(),
            error: TransportError::Timeout,
        };
    }

    match reader.read_line(&mut line) {
        Ok(0) => ReadLineResult {
            line: String::new(),
            error: TransportError::Eof,
        },
        Ok(_) => {
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
        Err(_) => ReadLineResult {
            line: String::new(),
            error: TransportError::ReadError,
        },
    }
}

pub fn write_json_line(t: &mut StdioTransport, line: &str) -> TransportError {
    let payload = format!("{}\n", line);
    match t.stdin.write_all(payload.as_bytes()) {
        Ok(_) => match t.stdin.flush() {
            Ok(_) => TransportError::Ok,
            Err(_) => TransportError::WriteError,
        },
        Err(_) => TransportError::WriteError,
    }
}

pub fn close(t: &mut StdioTransport) {
    let _ = t.stdin.write_all(b"");
    let _ = t.child.kill();
    let _ = t.child.wait();
    if let Some(handle) = t.stderr_thread.take() {
        let _ = handle.join();
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

    #[test]
    fn test_start_true_command() {
        let mut transport = start_stdio_transport("true", &[]).expect("should spawn 'true'");
        close(&mut transport);
    }

    #[test]
    fn test_read_timeout() {
        let mut transport =
            start_stdio_transport("sleep", &["10"]).expect("should spawn 'sleep 10'");
        let result = read_json_line(&mut transport, 100);
        assert_eq!(result.error, TransportError::Timeout);
        close(&mut transport);
    }

    #[test]
    fn test_close_empty_transport() {
        let mut transport = start_stdio_transport("true", &[]).expect("should spawn 'true'");
        close(&mut transport);
        close(&mut transport);
    }

    #[test]
    fn test_resource_cleanup() {
        let mut transport =
            start_stdio_transport("echo", &["hello"]).expect("should spawn 'echo hello'");
        close(&mut transport);
    }

    #[test]
    fn test_process_termination() {
        let mut transport =
            start_stdio_transport("sleep", &["10"]).expect("should spawn 'sleep 10'");
        close(&mut transport);
        let status = transport.child.try_wait();
        assert!(status.is_ok());
        assert!(status.unwrap().is_some());
    }
}
