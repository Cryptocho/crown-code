use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::agent::prompt::build_system_prompt;
use crate::agent::tools::{execute_tool, get_tool_definitions};
use crate::api::openai::{create_message_stream, new_client};
use crate::api::types::*;

pub trait AgentEventHandler: Send {
    fn on_assistant_text(&mut self, delta: &str);
    fn on_reasoning(&mut self, delta: &str);
    fn on_tool_call_start(&mut self, call_id: &str, name: &str, arguments: &str);
    fn on_tool_result(&mut self, call_id: &str, name: &str, content: &str, is_error: bool);
    fn on_usage(&mut self, input_tokens: i32, output_tokens: i32);
    fn on_task_done(&mut self, summary: &str);
    fn on_error(&mut self, code: i32, message: &str);
}

pub struct AgentSession {
    client: ApiClient,
    tools: Vec<Tool>,
    pub(crate) history: Vec<Message>,
    cwd: String,
    pub(crate) cancelled: Arc<AtomicBool>,
}

impl AgentSession {
    pub fn new(config: ApiClientConfig, cwd: String, cancelled: Arc<AtomicBool>) -> Self {
        let client = new_client(config);
        let tools = get_tool_definitions();
        let system_prompt = build_system_prompt(&cwd);
        let history = vec![Message {
            role: MessageRole::System,
            content: system_prompt,
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            name: String::new(),
        }];
        Self { client, tools, history, cwd, cancelled }
    }

    pub async fn handle_user_message(
        &mut self,
        content: &str,
        handler: &mut dyn AgentEventHandler,
    ) {
        self.cancelled.store(false, Ordering::SeqCst);

        self.history.push(Message {
            role: MessageRole::User,
            content: content.to_string(),
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            name: String::new(),
        });

        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                handler.on_error(0, "task cancelled");
                break;
            }

            let mut assistant_text = String::new();

            let resp = create_message_stream(
                &mut self.client,
                &self.history,
                &self.tools,
                |chunk| {
                    if self.cancelled.load(Ordering::SeqCst) {
                        return false;
                    }
                    match chunk {
                        ApiStreamChunk::Text(ref t) => {
                            assistant_text.push_str(t);
                            handler.on_assistant_text(t);
                        }
                        ApiStreamChunk::Reasoning(ref r) => {
                            handler.on_reasoning(r);
                        }
                        ApiStreamChunk::Usage { input_tokens, output_tokens } => {
                            handler.on_usage(input_tokens, output_tokens);
                        }
                        ApiStreamChunk::ToolCall(_) => {}
                        ApiStreamChunk::Done => {}
                    }
                    true
                },
            )
            .await;

            if !resp.error.message.is_empty() {
                handler.on_error(resp.error.code, &resp.error.message);
                break;
            }

            let tool_calls = resp.tool_calls;

            self.history.push(Message {
                role: MessageRole::Assistant,
                content: assistant_text,
                tool_calls: tool_calls.clone(),
                tool_call_id: String::new(),
                name: String::new(),
            });

            if tool_calls.is_empty() {
                break;
            }

            let mut has_completion = false;
            for tc in &tool_calls {
                handler.on_tool_call_start(&tc.id, &tc.function_name, &tc.arguments);

                let args: serde_json::Value = match serde_json::from_str(&tc.arguments) {
                    Ok(v) => v,
                    Err(_) => {
                        let error_msg = format!(
                            "Error: tool call arguments are invalid JSON (truncated response?): {}",
                            tc.arguments
                        );
                        handler.on_tool_result(&tc.id, &tc.function_name, &error_msg, true);
                        self.history.push(Message {
                            role: MessageRole::Tool,
                            content: error_msg,
                            tool_calls: Vec::new(),
                            tool_call_id: tc.id.clone(),
                            name: tc.function_name.clone(),
                        });
                        continue;
                    }
                };

                let result = execute_tool(&tc.function_name, &args).await;
                let is_error = result.starts_with("Error:");

                handler.on_tool_result(&tc.id, &tc.function_name, &result, is_error);

                if tc.function_name == "attempt_completion" {
                    let summary = result
                        .strip_prefix("[COMPLETION]")
                        .unwrap_or(&result);
                    handler.on_task_done(summary);
                    has_completion = true;
                }

                self.history.push(Message {
                    role: MessageRole::Tool,
                    content: result,
                    tool_calls: Vec::new(),
                    tool_call_id: tc.id.clone(),
                    name: tc.function_name.clone(),
                });
            }

            if has_completion {
                break;
            }
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn reset(&mut self) {
        self.history.clear();
        let system_prompt = build_system_prompt(&self.cwd);
        self.history.push(Message {
            role: MessageRole::System,
            content: system_prompt,
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            name: String::new(),
        });
        self.cancelled.store(false, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
pub async fn run_agent_loop(config: ApiClientConfig) {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut session = AgentSession::new(config, cwd, cancelled);

    println!("crown-code — A vibe coding tool");
    println!("Type your task. /quit to exit.");

    struct CliHandler;
    impl AgentEventHandler for CliHandler {
        fn on_assistant_text(&mut self, delta: &str) {
            print!("{}", delta);
            io::stdout().flush().unwrap();
        }
        fn on_reasoning(&mut self, _delta: &str) {}
        fn on_tool_call_start(&mut self, _call_id: &str, name: &str, arguments: &str) {
            eprintln!("[TOOL_CALL] {}({})", name, arguments);
        }
        fn on_tool_result(&mut self, _call_id: &str, name: &str, content: &str, _is_error: bool) {
            eprintln!("[TOOL_RESULT] {}:\n{}\n---", name, content);
        }
        fn on_usage(&mut self, _input: i32, _output: i32) {}
        fn on_task_done(&mut self, summary: &str) {
            println!("\n[COMPLETION] {}", summary);
            println!("--- Task finished. Enter new task or /quit ---");
        }
        fn on_error(&mut self, _code: i32, message: &str) {
            eprintln!("\n[API Error] {}", message);
        }
    }

    let mut handler = CliHandler;
    loop {
        print!("\nYou: ");
        io::stdout().flush().unwrap();
        let mut user_input = String::new();
        match io::stdin().read_line(&mut user_input) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let user_input = user_input.trim().to_string();
        if user_input.is_empty() { continue; }
        if user_input == "/quit" || user_input == "/exit" { break; }
        session.handle_user_message(&user_input, &mut handler).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler {
        texts: Vec<String>,
        tool_calls: Vec<(String, String, String)>,
        tool_results: Vec<(String, String, bool)>,
        errors: Vec<(i32, String)>,
        summaries: Vec<String>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                texts: Vec::new(),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                errors: Vec::new(),
                summaries: Vec::new(),
            }
        }
    }

    impl AgentEventHandler for TestHandler {
        fn on_assistant_text(&mut self, delta: &str) { self.texts.push(delta.to_string()); }
        fn on_reasoning(&mut self, _delta: &str) {}
        fn on_tool_call_start(&mut self, call_id: &str, name: &str, arguments: &str) {
            self.tool_calls.push((call_id.to_string(), name.to_string(), arguments.to_string()));
        }
        fn on_tool_result(&mut self, call_id: &str, _name: &str, content: &str, is_error: bool) {
            self.tool_results.push((call_id.to_string(), content.to_string(), is_error));
        }
        fn on_usage(&mut self, _input: i32, _output: i32) {}
        fn on_task_done(&mut self, summary: &str) { self.summaries.push(summary.to_string()); }
        fn on_error(&mut self, code: i32, message: &str) {
            self.errors.push((code, message.to_string()));
        }
    }

    fn make_test_config() -> ApiClientConfig {
        ApiClientConfig {
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: String::new(),
            model: "test".to_string(),
            temperature: 0.0,
            max_tokens: 4096,
            stream_options: None,
        }
    }

    #[test]
    fn test_agent_session_new_has_system_prompt() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let session = AgentSession::new(make_test_config(), "/tmp".to_string(), cancelled);
        assert_eq!(session.history_len(), 1);
        assert_eq!(session.history[0].role, MessageRole::System);
    }

    #[test]
    fn test_agent_session_cancel() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let session = AgentSession::new(make_test_config(), "/tmp".to_string(), cancelled);
        session.cancel();
        assert!(session.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn test_agent_session_reset() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut session = AgentSession::new(make_test_config(), "/tmp".to_string(), cancelled);
        session.history.push(Message {
            role: MessageRole::User,
            content: "hello".to_string(),
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            name: String::new(),
        });
        assert_eq!(session.history_len(), 2);
        session.reset();
        assert_eq!(session.history_len(), 1);
        assert_eq!(session.history[0].role, MessageRole::System);
        assert!(!session.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn test_agent_session_cancelled_flag_shared() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let session = AgentSession::new(make_test_config(), "/tmp".to_string(), cancelled.clone());
        assert!(!cancelled.load(Ordering::SeqCst));
        session.cancel();
        assert!(cancelled.load(Ordering::SeqCst));
    }
}