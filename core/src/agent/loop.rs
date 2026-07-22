use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::agent::prompt::build_system_prompt;
use crate::agent::tools::{execute_tool, get_tool_definitions};
use crate::api::openai::{create_message_stream, new_client};
use crate::api::types::*;

pub trait AgentEventHandler: Send {
    fn on_assistant_text(&mut self, delta: &str);
    fn on_reasoning(&mut self, delta: &str);
    fn on_tool_call_start(&mut self, call_id: &str, name: &str, arguments: &str);
    fn on_tool_result(&mut self, call_id: &str, name: &str, content: &str, is_error: bool);
    fn on_usage(&mut self, input_tokens: i32, output_tokens: i32, cache_read_tokens: i32);
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
        Self {
            client,
            tools,
            history,
            cwd,
            cancelled,
        }
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

            let resp =
                create_message_stream(&mut self.client, &self.history, &self.tools, |chunk| {
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
                        ApiStreamChunk::Usage {
                            input_tokens,
                            output_tokens,
                            cache_read_tokens,
                        } => {
                            handler.on_usage(input_tokens, output_tokens, cache_read_tokens);
                        }
                        ApiStreamChunk::ToolCall(_) => {}
                        ApiStreamChunk::Done => {}
                    }
                    true
                })
                .await;

            if !resp.error.message.is_empty() {
                handler.on_error(resp.error.code, &resp.error.message);
                break;
            }

            if self.cancelled.load(Ordering::SeqCst) {
                handler.on_error(0, "task cancelled");
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

            let mut cancelled = false;
            for tc in &tool_calls {
                if self.cancelled.load(Ordering::SeqCst) {
                    handler.on_error(0, "task cancelled");
                    cancelled = true;
                    break;
                }

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

                self.history.push(Message {
                    role: MessageRole::Tool,
                    content: result,
                    tool_calls: Vec::new(),
                    tool_call_id: tc.id.clone(),
                    name: tc.function_name.clone(),
                });
            }

            if cancelled {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler {
        texts: Vec<String>,
        tool_calls: Vec<(String, String, String)>,
        tool_results: Vec<(String, String, bool)>,
        errors: Vec<(i32, String)>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                texts: Vec::new(),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                errors: Vec::new(),
            }
        }
    }

    impl AgentEventHandler for TestHandler {
        fn on_assistant_text(&mut self, delta: &str) {
            self.texts.push(delta.to_string());
        }
        fn on_reasoning(&mut self, _delta: &str) {}
        fn on_tool_call_start(&mut self, call_id: &str, name: &str, arguments: &str) {
            self.tool_calls
                .push((call_id.to_string(), name.to_string(), arguments.to_string()));
        }
        fn on_tool_result(&mut self, call_id: &str, _name: &str, content: &str, is_error: bool) {
            self.tool_results
                .push((call_id.to_string(), content.to_string(), is_error));
        }
        fn on_usage(&mut self, _input: i32, _output: i32, _cache_read: i32) {}
        fn on_error(&mut self, code: i32, message: &str) {
            self.errors.push((code, message.to_string()));
        }
    }

    fn make_test_config() -> ApiClientConfig {
        ApiClientConfig {
            base_url: "http://127.0.0.1:1/v1".to_string(),
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

    #[tokio::test]
    async fn test_handle_user_message_pushes_to_history() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut session = AgentSession::new(make_test_config(), "/tmp".to_string(), cancelled);
        let mut handler = TestHandler::new();
        session.handle_user_message("test", &mut handler).await;
        assert_eq!(session.history_len(), 2);
        assert_eq!(session.history[1].role, MessageRole::User);
        assert_eq!(session.history[1].content, "test");
    }

    #[tokio::test]
    async fn test_handle_user_message_api_error_recorded() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut session = AgentSession::new(make_test_config(), "/tmp".to_string(), cancelled);
        let mut handler = TestHandler::new();
        session.handle_user_message("hello", &mut handler).await;
        assert!(handler.errors.iter().any(|(_, msg)| !msg.is_empty()));
    }

    #[test]
    fn test_agent_session_new_with_custom_cwd() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let session = AgentSession::new(make_test_config(), "/custom/path".to_string(), cancelled);
        assert!(session.history[0].content.contains("/custom/path"));
    }

    #[test]
    fn test_agent_session_new_tools_non_empty() {
        let tools = get_tool_definitions();
        assert_eq!(tools.len(), 6);
    }
}
