use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::shell_detect::detect_shells;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    Ok,
    ApprovalDenied,
    ExecutionFailed,
    Timeout,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time: f64,
    pub abnormal_exit: bool,
    pub error: CommandError,
}

pub const MAX_FULL_OUTPUT_SIZE: usize = 1024 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 300_000;
pub const CIRCULAR_BUFFER_SIZE: usize = 2000;

pub struct CircularBuffer {
    inner: Mutex<CircularBufferInner>,
}

struct CircularBufferInner {
    buffer: Vec<String>,
    head: usize,
    count: usize,
}

impl Default for CircularBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl CircularBuffer {
    pub fn new() -> Self {
        CircularBuffer {
            inner: Mutex::new(CircularBufferInner {
                buffer: vec![String::new(); CIRCULAR_BUFFER_SIZE],
                head: 0,
                count: 0,
            }),
        }
    }

    pub fn push(&self, line: &str) {
        let mut lock = self.inner.lock().unwrap();
        let head = lock.head;
        lock.buffer[head] = line.to_string();
        lock.head = (lock.head + 1) % CIRCULAR_BUFFER_SIZE;
        if lock.count < CIRCULAR_BUFFER_SIZE {
            lock.count += 1;
        }
    }

    pub fn join(&self) -> String {
        let lock = self.inner.lock().unwrap();
        let mut total = 0usize;
        for i in 0..lock.count {
            let idx = (lock.head + CIRCULAR_BUFFER_SIZE - lock.count + i) % CIRCULAR_BUFFER_SIZE;
            total += lock.buffer[idx].len();
        }
        let mut result = String::with_capacity(total);
        for i in 0..lock.count {
            let idx = (lock.head + CIRCULAR_BUFFER_SIZE - lock.count + i) % CIRCULAR_BUFFER_SIZE;
            result.push_str(&lock.buffer[idx]);
        }
        result
    }
}

pub fn trim_whitespace(s: &str) -> String {
    s.trim_matches(|c: char| c == ' ' || c == '\t').to_string()
}

pub fn split_commands(command: &str) -> Vec<String> {
    if command.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let cmd = command.as_bytes();
    let len = cmd.len();
    let mut start = 0;
    let mut i = 0;

    while i < len {
        let mut sep_len = 0;

        if i + 1 < len {
            let two_char = &cmd[i..i + 2];
            if two_char == b"&&" || two_char == b"&|" || two_char == b"||" {
                sep_len = 2;
            }
        }
        if sep_len == 0 && (cmd[i] == b'&' || cmd[i] == b'|' || cmd[i] == b';') {
            sep_len = 1;
        }

        if sep_len > 0 {
            let token_len = i - start;
            if token_len > 0 {
                result.push(command[start..i].to_string());
            }
            i += sep_len;
            start = i;
        } else {
            i += 1;
        }
    }

    let token_len = i - start;
    if token_len > 0 {
        result.push(command[start..i].to_string());
    }

    result
}

pub fn requires_approval(_command: &str) -> bool {
    true
}

pub fn exec_command(command: &str, blacklist: &[&str]) -> CommandResult {
    let trimmed = trim_whitespace(command);
    if trimmed.is_empty() {
        return CommandResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: -1,
            execution_time: 0.0,
            abnormal_exit: false,
            error: CommandError::ExecutionFailed,
        };
    }

    let sub_commands = split_commands(&trimmed);
    for sub in &sub_commands {
        let sub_trimmed = trim_whitespace(sub);
        if sub_trimmed.is_empty() {
            continue;
        }
        for blocked in blacklist {
            if sub_trimmed == *blocked {
                if !requires_approval(&sub_trimmed) {
                    return CommandResult {
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: -1,
                        execution_time: 0.0,
                        abnormal_exit: false,
                        error: CommandError::ApprovalDenied,
                    };
                }
                break;
            }
        }
    }

    let shells = detect_shells();
    if shells.is_empty() || !shells[0].found {
        return CommandResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: -1,
            execution_time: 0.0,
            abnormal_exit: false,
            error: CommandError::ExecutionFailed,
        };
    }

    let shell_path = &shells[0].path;
    let start_time = Instant::now();

    #[cfg(not(target_os = "windows"))]
    let child_result = Command::new(shell_path)
        .args(["-l", "-c", command])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    #[cfg(target_os = "windows")]
    let child_result = Command::new("cmd.exe")
        .args(["/c", command])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child_result {
        Ok(c) => c,
        Err(_) => {
            return CommandResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                execution_time: start_time.elapsed().as_secs_f64(),
                abnormal_exit: false,
                error: CommandError::ExecutionFailed,
            };
        }
    };

    let stdout = child.stdout.take().expect("failed to take stdout");
    let stderr = child.stderr.take().expect("failed to take stderr");

    let out_buf = Arc::new(CircularBuffer::new());
    let err_buf = Arc::new(CircularBuffer::new());
    let total_out_size = Arc::new(AtomicUsize::new(0));
    let total_err_size = Arc::new(AtomicUsize::new(0));

    let out_buf_clone = out_buf.clone();
    let total_out_clone = total_out_size.clone();
    let out_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if total_out_clone.load(Ordering::Relaxed) < MAX_FULL_OUTPUT_SIZE {
                        out_buf_clone.push(&l);
                        total_out_clone.fetch_add(l.len() + 1, Ordering::Relaxed);
                    }
                }
                Err(_) => break,
            }
        }
    });

    let err_buf_clone = err_buf.clone();
    let total_err_clone = total_err_size.clone();
    let err_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if total_err_clone.load(Ordering::Relaxed) < MAX_FULL_OUTPUT_SIZE {
                        err_buf_clone.push(&l);
                        total_err_clone.fetch_add(l.len() + 1, Ordering::Relaxed);
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut exit_code = -1;
    let mut timed_out = false;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code().unwrap_or(-1);
                break;
            }
            Ok(None) => {
                if start_time.elapsed().as_millis() as u64 >= DEFAULT_TIMEOUT_MS {
                    timed_out = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => {
                exit_code = -1;
                break;
            }
        }
    }

    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
    }

    out_thread.join().ok();
    err_thread.join().ok();

    let execution_time = start_time.elapsed().as_secs_f64();

    let mut result = CommandResult {
        stdout: out_buf.join(),
        stderr: err_buf.join(),
        exit_code,
        execution_time,
        abnormal_exit: timed_out,
        error: CommandError::Ok,
    };

    if timed_out {
        result.error = CommandError::Timeout;
    } else if result.exit_code != 0 {
        result.error = CommandError::ExecutionFailed;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_whitespace_empty() {
        assert_eq!(trim_whitespace(""), "");
    }

    #[test]
    fn test_trim_whitespace_spaces() {
        assert_eq!(trim_whitespace("  hello  "), "hello");
    }

    #[test]
    fn test_trim_whitespace_tabs() {
        assert_eq!(trim_whitespace("\t\thello\t"), "hello");
    }

    #[test]
    fn test_trim_whitespace_mixed() {
        assert_eq!(trim_whitespace(" \t hello \t "), "hello");
    }

    #[test]
    fn test_trim_whitespace_no_change() {
        assert_eq!(trim_whitespace("hello"), "hello");
    }

    #[test]
    fn test_split_commands_and_and() {
        let result = split_commands("echo a && echo b");
        assert_eq!(result, vec!["echo a ", " echo b"]);
    }

    #[test]
    fn test_split_commands_or_or() {
        let result = split_commands("echo a || echo b");
        assert_eq!(result, vec!["echo a ", " echo b"]);
    }

    #[test]
    fn test_split_commands_and_or() {
        let result = split_commands("echo a &| echo b");
        assert_eq!(result, vec!["echo a ", " echo b"]);
    }

    #[test]
    fn test_split_commands_pipe() {
        let result = split_commands("echo a | grep a");
        assert_eq!(result, vec!["echo a ", " grep a"]);
    }

    #[test]
    fn test_split_commands_background() {
        let result = split_commands("cmd1 & cmd2");
        assert_eq!(result, vec!["cmd1 ", " cmd2"]);
    }

    #[test]
    fn test_split_commands_semicolon() {
        let result = split_commands("echo a; echo b");
        assert_eq!(result, vec!["echo a", " echo b"]);
    }

    #[test]
    fn test_split_commands_combined() {
        let result = split_commands("a && b || c; d & e | f");
        assert_eq!(result, vec!["a ", " b ", " c", " d ", " e ", " f"]);
    }

    #[test]
    fn test_split_commands_empty() {
        let result: Vec<String> = split_commands("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_split_commands_no_separator() {
        let result = split_commands("echo hello world");
        assert_eq!(result, vec!["echo hello world"]);
    }

    #[test]
    fn test_requires_approval_always_true() {
        assert!(requires_approval("echo hello"));
        assert!(requires_approval(""));
        assert!(requires_approval("rm -rf /"));
    }

    #[test]
    fn test_circular_buffer_push_join() {
        let cb = CircularBuffer::new();
        cb.push("hello");
        cb.push("world");
        assert_eq!(cb.join(), "helloworld");
    }

    #[test]
    fn test_circular_buffer_overflow() {
        let cb = CircularBuffer::new();
        for i in 0..CIRCULAR_BUFFER_SIZE + 100 {
            cb.push(&format!("line{}", i));
        }
        let result = cb.join();
        assert!(result.starts_with("line100"));
        assert!(result.ends_with("line2099"));
    }

    #[test]
    fn test_circular_buffer_single_line() {
        let cb = CircularBuffer::new();
        cb.push("only");
        assert_eq!(cb.join(), "only");
    }

    #[test]
    fn test_circular_buffer_empty() {
        let cb = CircularBuffer::new();
        assert_eq!(cb.join(), "");
    }

    #[test]
    fn test_exec_command_echo() {
        let result = exec_command("echo hello", &[]);
        assert_eq!(result.error, CommandError::Ok);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[test]
    fn test_exec_command_exit_code() {
        let result = exec_command("bash -c 'exit 42'", &[]);
        assert_eq!(result.exit_code, 42);
        assert_eq!(result.error, CommandError::ExecutionFailed);
    }

    #[test]
    fn test_exec_command_stderr() {
        let result = exec_command("bash -c 'echo error >&2'", &[]);
        assert_eq!(result.error, CommandError::Ok);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stderr.trim(), "error");
    }

    #[test]
    fn test_exec_command_empty() {
        let result = exec_command("", &[]);
        assert_eq!(result.error, CommandError::ExecutionFailed);
    }

    #[test]
    fn test_exec_command_whitespace() {
        let result = exec_command("   \t   ", &[]);
        assert_eq!(result.error, CommandError::ExecutionFailed);
    }

    #[test]
    fn test_exec_command_execution_time() {
        let result = exec_command("echo hello", &[]);
        assert!(result.execution_time > 0.0);
    }

    #[test]
    fn test_exec_command_not_found() {
        let result = exec_command("nonexistent_command_xyz123", &[]);
        assert_eq!(result.error, CommandError::ExecutionFailed);
    }
}
