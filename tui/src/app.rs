use std::collections::VecDeque;

use crate::app_event::{AppEvent, AppEventSender};
use crate::chatwidget::ChatWidget;
use crate::keymap::{self, KeyAction};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crown_core::ipc::message::JsonRpcMessage;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Active,
    Completed,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentMode {
    Plan,
    Code,
    Ask,
}

impl AgentMode {
    pub fn label(&self) -> &'static str {
        match self {
            AgentMode::Plan => "[Plan]",
            AgentMode::Code => "[Code]",
            AgentMode::Ask => "[Ask]",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            AgentMode::Plan => AgentMode::Code,
            AgentMode::Code => AgentMode::Ask,
            AgentMode::Ask => AgentMode::Plan,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FocusTarget {
    ChatPanel,
    Input,
}

pub struct App {
    pub chat_widget: ChatWidget,
    pub status: SessionStatus,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub agent_mode: AgentMode,
    pub should_quit: bool,
    pub needs_redraw: bool,
    pub app_event_tx: AppEventSender,
    pub focus: FocusTarget,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cache_read_tokens: i32,
    pub model: String,
    pub api_latencies: VecDeque<u64>,
}

impl App {
    pub fn new(session_id: String, app_event_tx: AppEventSender) -> Self {
        Self {
            chat_widget: ChatWidget::new(),
            status: SessionStatus::Active,
            session_id: Some(session_id),
            session_name: None,
            agent_mode: AgentMode::Code,
            should_quit: false,
            needs_redraw: true,
            app_event_tx,
            focus: FocusTarget::Input,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            model: String::new(),
            api_latencies: VecDeque::with_capacity(5),
        }
    }

    pub fn avg_latency(&self) -> Option<u64> {
        if self.api_latencies.is_empty() {
            None
        } else {
            Some(self.api_latencies.iter().sum::<u64>() / self.api_latencies.len() as u64)
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let action = if self.focus == FocusTarget::Input {
            keymap::map_input_key(key)
        } else {
            keymap::map_chat_key(key)
        };
        match action {
            KeyAction::Quit => {
                self.app_event_tx.send(AppEvent::Quit);
            }
            KeyAction::Cancel => {
                self.app_event_tx.send(AppEvent::CancelRequested);
            }
            KeyAction::CycleAgentMode => {
                self.agent_mode = self.agent_mode.cycle();
                self.needs_redraw = true;
            }
            KeyAction::SubmitMessage => {
                let msg = self.chat_widget.take_input();
                if !msg.trim().is_empty() {
                    self.app_event_tx.send(AppEvent::UserMessageSent(msg));
                }
                self.needs_redraw = true;
            }
            KeyAction::FocusNext => {
                self.focus = if self.focus == FocusTarget::Input {
                    FocusTarget::ChatPanel
                } else {
                    FocusTarget::Input
                };
                self.needs_redraw = true;
            }
            KeyAction::ScrollUp(n) => {
                self.chat_widget.scroll_up(n);
                self.needs_redraw = true;
            }
            KeyAction::ScrollDown(n) => {
                self.chat_widget.scroll_down(n);
                self.needs_redraw = true;
            }
            KeyAction::ScrollToBottom => {
                self.chat_widget.scroll_to_bottom();
                self.needs_redraw = true;
            }
            KeyAction::ToggleToolExpand => {
                if let Some(idx) = self.chat_widget.current_tool_index() {
                    self.chat_widget.toggle_tool_expanded(idx);
                    self.needs_redraw = true;
                }
            }
            KeyAction::None => {
                if self.focus == FocusTarget::Input {
                    self.chat_widget.input_key(key);
                    self.needs_redraw = true;
                }
            }
        }
    }

    pub fn handle_paste(&mut self, text: &str) {
        if self.focus == FocusTarget::Input {
            for ch in text.chars() {
                let key = if ch == '\n' || ch == '\r' {
                    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
                } else {
                    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)
                };
                self.chat_widget.input_key(key);
            }
            self.needs_redraw = true;
        }
    }

    pub fn handle_ipc_message(&mut self, msg: JsonRpcMessage) {
        if let Some(event) = self.parse_ipc_event(&msg) {
            self.handle_app_event(event);
        }
    }

    pub fn parse_ipc_event(&self, msg: &JsonRpcMessage) -> Option<AppEvent> {
        if !msg.is_notification() {
            return None;
        }
        let method = msg.method.as_deref()?;
        let params = msg.params.as_ref()?;

        match method {
            "assistant_text" => {
                let delta = params["delta"].as_str().unwrap_or("").to_string();
                Some(AppEvent::AssistantDelta { delta })
            }
            "assistant_reasoning" => {
                let delta = params["delta"].as_str().unwrap_or("").to_string();
                Some(AppEvent::ReasoningDelta { delta })
            }
            "tool_call_start" => Some(AppEvent::ToolCallStart {
                call_id: params["call_id"].as_str().unwrap_or("").to_string(),
                name: params["name"].as_str().unwrap_or("").to_string(),
                arguments: params["arguments"].as_str().unwrap_or("").to_string(),
            }),
            "tool_result" => Some(AppEvent::ToolResult {
                call_id: params["call_id"].as_str().unwrap_or("").to_string(),
                name: params["name"].as_str().unwrap_or("").to_string(),
                content: params["content"].as_str().unwrap_or("").to_string(),
                is_error: params["is_error"].as_bool().unwrap_or(false),
            }),
            "usage" => Some(AppEvent::Usage {
                input_tokens: params["input_tokens"].as_i64().unwrap_or(0) as i32,
                output_tokens: params["output_tokens"].as_i64().unwrap_or(0) as i32,
                cache_read_tokens: params["cache_read_tokens"].as_i64().unwrap_or(0) as i32,
            }),
            "task_done" => {
                let summary = params["summary"].as_str().unwrap_or("").to_string();
                Some(AppEvent::TaskDone { summary })
            }
            "error" => Some(AppEvent::Error {
                code: params["code"].as_i64().unwrap_or(0) as i32,
                message: params["message"].as_str().unwrap_or("").to_string(),
            }),
            "session_created" => {
                let session_id = params["session_id"].as_str().unwrap_or("").to_string();
                Some(AppEvent::SessionCreated { session_id })
            }
            "session_name_update" => {
                let name = params["name"].as_str().unwrap_or("").to_string();
                Some(AppEvent::SessionNameUpdate { name })
            }
            _ => None,
        }
    }

    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::UserMessageSent(text) => {
                use crate::history_cell::UserMessageCell;
                self.chat_widget
                    .push_cell(Box::new(UserMessageCell { content: text }));
                self.needs_redraw = true;
            }
            AppEvent::AssistantDelta { delta } => {
                if self.chat_widget.active_cell.is_none() {
                    self.chat_widget.start_streaming();
                }
                self.chat_widget.append_streaming(&delta);
                self.status = SessionStatus::Active;
                self.needs_redraw = true;
            }
            AppEvent::ReasoningDelta { .. } => {}
            AppEvent::ToolCallStart {
                call_id,
                name,
                arguments,
            } => {
                self.chat_widget
                    .start_tool_call(&call_id, &name, &arguments);
                self.status = SessionStatus::Active;
                self.needs_redraw = true;
            }
            AppEvent::ToolResult {
                call_id,
                name,
                content,
                is_error,
            } => {
                self.chat_widget
                    .finish_tool_call(&call_id, &name, &content, is_error);
                self.needs_redraw = true;
            }
            AppEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
            } => {
                self.input_tokens += input_tokens;
                self.output_tokens += output_tokens;
                self.cache_read_tokens += cache_read_tokens;
                self.needs_redraw = true;
            }
            AppEvent::TaskDone { .. } => {
                self.chat_widget.finish_streaming();
                self.status = SessionStatus::Completed;
                self.needs_redraw = true;
            }
            AppEvent::Error { code, message } => {
                self.chat_widget.finish_streaming();
                use crate::history_cell::ErrorCell;
                self.chat_widget
                    .push_cell(Box::new(ErrorCell { code, message }));
                self.status = SessionStatus::Error;
                self.needs_redraw = true;
            }
            AppEvent::CancelRequested => {
                self.status = SessionStatus::Completed;
                self.needs_redraw = true;
            }
            AppEvent::Quit => {
                self.should_quit = true;
            }
            AppEvent::RedrawRequested => {
                self.needs_redraw = true;
            }
            AppEvent::SessionCreated { session_id } => {
                self.session_id = Some(session_id);
            }
            AppEvent::SessionNameUpdate { name } => {
                self.session_name = Some(name);
                self.needs_redraw = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn make_test_app() -> App {
        let (tx, _rx) = mpsc::unbounded_channel();
        App::new("test_session".into(), AppEventSender::new(tx))
    }

    #[test]
    fn test_app_new_defaults() {
        let app = make_test_app();
        assert_eq!(app.session_id, Some("test_session".into()));
        assert_eq!(app.status, SessionStatus::Active);
        assert_eq!(app.agent_mode, AgentMode::Code);
        assert!(!app.should_quit);
        assert!(app.needs_redraw);
        assert_eq!(app.focus, FocusTarget::Input);
        assert_eq!(app.model, "");
    }

    #[test]
    fn test_agent_mode_label() {
        assert_eq!(AgentMode::Plan.label(), "[Plan]");
        assert_eq!(AgentMode::Code.label(), "[Code]");
        assert_eq!(AgentMode::Ask.label(), "[Ask]");
    }

    #[test]
    fn test_agent_mode_cycle() {
        assert_eq!(AgentMode::Plan.cycle(), AgentMode::Code);
        assert_eq!(AgentMode::Code.cycle(), AgentMode::Ask);
        assert_eq!(AgentMode::Ask.cycle(), AgentMode::Plan);
    }

    #[test]
    fn test_avg_latency_empty() {
        let app = make_test_app();
        assert_eq!(app.avg_latency(), None);
    }

    #[test]
    fn test_avg_latency_nonempty() {
        let mut app = make_test_app();
        app.api_latencies = VecDeque::from([100, 200, 300]);
        assert_eq!(app.avg_latency(), Some(200));
    }

    // === parse_ipc_event tests ===

    #[test]
    fn test_parse_ipc_assistant_text() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "assistant_text",
            serde_json::json!({"session_id": "s1", "delta": "hello"}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::AssistantDelta { delta } => assert_eq!(delta, "hello"),
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_assistant_reasoning() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "assistant_reasoning",
            serde_json::json!({"delta": "thinking..."}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::ReasoningDelta { delta } => assert_eq!(delta, "thinking..."),
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_tool_call_start() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "tool_call_start",
            serde_json::json!({"call_id": "c1", "name": "read_file", "arguments": "/tmp/x"}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::ToolCallStart {
                call_id,
                name,
                arguments,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(name, "read_file");
                assert_eq!(arguments, "/tmp/x");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_tool_result() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "tool_result",
            serde_json::json!({"call_id": "c1", "name": "read_file", "content": "file data", "is_error": false}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::ToolResult {
                call_id,
                name,
                content,
                is_error,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(name, "read_file");
                assert_eq!(content, "file data");
                assert!(!is_error);
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_usage() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "usage",
            serde_json::json!({"input_tokens": 100, "output_tokens": 50, "cache_read_tokens": 30}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
            } => {
                assert_eq!(input_tokens, 100);
                assert_eq!(output_tokens, 50);
                assert_eq!(cache_read_tokens, 30);
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_task_done() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "task_done",
            serde_json::json!({"summary": "finished"}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::TaskDone { summary } => assert_eq!(summary, "finished"),
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_error() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "error",
            serde_json::json!({"code": 500, "message": "crash"}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::Error { code, message } => {
                assert_eq!(code, 500);
                assert_eq!(message, "crash");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_session_created() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "session_created",
            serde_json::json!({"session_id": "sess_123"}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::SessionCreated { session_id } => assert_eq!(session_id, "sess_123"),
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_session_name_update() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_notification(
            "session_name_update",
            serde_json::json!({"name": "my project"}),
        );
        let event = app.parse_ipc_event(&msg).unwrap();
        match event {
            AppEvent::SessionNameUpdate { name } => assert_eq!(name, "my project"),
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_parse_ipc_unknown_method_returns_none() {
        let app = make_test_app();
        let msg =
            crown_core::ipc::message::make_notification("future_event", serde_json::json!({}));
        assert!(app.parse_ipc_event(&msg).is_none());
    }

    #[test]
    fn test_parse_ipc_response_returns_none() {
        let app = make_test_app();
        let msg = crown_core::ipc::message::make_response(1, serde_json::json!({"ok": true}));
        assert!(app.parse_ipc_event(&msg).is_none());
    }

    // === handle_app_event tests ===

    #[test]
    fn test_handle_assistant_delta_starts_streaming() {
        let app = make_test_app();
        let mut app = app;
        app.handle_app_event(AppEvent::AssistantDelta {
            delta: "hello".into(),
        });
        assert!(app.chat_widget.active_cell.is_some());
        assert_eq!(app.status, SessionStatus::Active);
        assert!(app.needs_redraw);
    }

    #[test]
    fn test_handle_tool_call_start_interrupts_streaming() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::AssistantDelta {
            delta: "text".into(),
        });
        app.handle_app_event(AppEvent::ToolCallStart {
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
        });
        assert!(app.chat_widget.active_cell.is_none());
        assert_eq!(app.chat_widget.cells.len(), 2);
    }

    #[test]
    fn test_handle_usage_accumulates() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 30,
        });
        app.handle_app_event(AppEvent::Usage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read_tokens: 60,
        });
        assert_eq!(app.input_tokens, 300);
        assert_eq!(app.output_tokens, 150);
        assert_eq!(app.cache_read_tokens, 90);
    }

    #[test]
    fn test_handle_task_done_sets_completed() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::AssistantDelta {
            delta: "done".into(),
        });
        app.handle_app_event(AppEvent::TaskDone {
            summary: "finished".into(),
        });
        assert_eq!(app.status, SessionStatus::Completed);
        assert!(app.chat_widget.active_cell.is_none());
    }

    #[test]
    fn test_handle_error_sets_error_status() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::Error {
            code: 500,
            message: "crash".into(),
        });
        assert_eq!(app.status, SessionStatus::Error);
        assert!(app.chat_widget.cells.iter().any(|c| {
            c.display_lines(80).iter().any(|l| {
                let text = format!("{l:?}");
                text.contains("Error") || text.contains("crash")
            })
        }));
    }

    #[test]
    fn test_handle_quit() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_full_flow_assistant_text_then_tool_then_done() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));

        app.handle_app_event(AppEvent::UserMessageSent("hello".into()));
        assert_eq!(app.chat_widget.cells.len(), 1);

        app.handle_app_event(AppEvent::AssistantDelta {
            delta: "I'll ".into(),
        });
        app.handle_app_event(AppEvent::AssistantDelta {
            delta: "read a file".into(),
        });
        assert!(app.chat_widget.active_cell.is_some());

        app.handle_app_event(AppEvent::ToolCallStart {
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
        });
        assert!(app.chat_widget.active_cell.is_none());
        assert_eq!(app.chat_widget.cells.len(), 3);

        app.handle_app_event(AppEvent::ToolResult {
            call_id: "c1".into(),
            name: "read_file".into(),
            content: "file content".into(),
            is_error: false,
        });

        app.handle_app_event(AppEvent::AssistantDelta {
            delta: "Here's the file.".into(),
        });
        assert!(app.chat_widget.active_cell.is_some());

        app.handle_app_event(AppEvent::TaskDone {
            summary: "done".into(),
        });
        assert!(app.chat_widget.active_cell.is_none());
        assert_eq!(app.status, SessionStatus::Completed);
    }

    #[test]
    fn test_handle_session_name_update() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::SessionNameUpdate {
            name: "我的项目".into(),
        });
        assert_eq!(app.session_name, Some("我的项目".into()));
    }

    #[test]
    fn test_handle_session_created_updates_id() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("old_id".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::SessionCreated {
            session_id: "new_id".into(),
        });
        assert_eq!(app.session_id, Some("new_id".into()));
    }

    #[test]
    fn test_handle_user_message_sent_pushes_cell() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::UserMessageSent("hello world".into()));
        assert_eq!(app.chat_widget.cells.len(), 1);
        let lines = app.chat_widget.cells[0].display_lines(80);
        let text = format!("{lines:?}");
        assert!(text.contains("hello world"));
    }

    #[test]
    fn test_handle_assistant_delta_auto_starts_streaming() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        assert!(app.chat_widget.active_cell.is_none());
        app.handle_app_event(AppEvent::AssistantDelta { delta: "hi".into() });
        assert!(app.chat_widget.active_cell.is_some());
    }

    #[test]
    fn test_handle_error_finishes_active_streaming() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::AssistantDelta {
            delta: "text".into(),
        });
        assert!(app.chat_widget.active_cell.is_some());
        app.handle_app_event(AppEvent::Error {
            code: 500,
            message: "fail".into(),
        });
        assert!(app.chat_widget.active_cell.is_none());
    }

    #[test]
    fn test_handle_cancel_requested() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_app_event(AppEvent::CancelRequested);
        assert_eq!(app.status, SessionStatus::Completed);
        assert!(app.needs_redraw);
    }

    #[test]
    fn test_handle_redraw_requested() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.needs_redraw = false;
        app.handle_app_event(AppEvent::RedrawRequested);
        assert!(app.needs_redraw);
    }

    #[test]
    fn test_handle_key_quit() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, AppEvent::Quit));
    }

    #[test]
    fn test_handle_key_cycle_mode() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        assert_eq!(app.agent_mode, AgentMode::Code);
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('p'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        assert_eq!(app.agent_mode, AgentMode::Ask);
        assert!(app.needs_redraw);
    }

    #[test]
    fn test_handle_key_submit_message() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.chat_widget.textarea.insert_str("hello");
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let event = rx.try_recv().unwrap();
        match event {
            AppEvent::UserMessageSent(msg) => assert_eq!(msg, "hello"),
            _ => panic!("expected UserMessageSent"),
        }
        assert!(app.needs_redraw);
    }

    #[test]
    fn test_handle_key_scroll() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.focus = FocusTarget::ChatPanel;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(app.chat_widget.scroll_offset > 0);
        assert!(app.needs_redraw);
    }

    #[test]
    fn test_handle_key_toggle_tool() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.chat_widget.start_tool_call("c1", "read_file", "{}");
        app.focus = FocusTarget::ChatPanel;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let tc = app.chat_widget.cells[0].as_tool_call().unwrap();
        assert!(tc.expanded);
        assert!(app.needs_redraw);
    }

    #[test]
    fn test_handle_key_input_passthrough() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.focus = FocusTarget::Input;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(app.needs_redraw);
        let lines = app.chat_widget.textarea.lines();
        assert!(lines.iter().any(|l| l.contains('a')));
    }

    #[test]
    fn test_handle_paste_in_input() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.focus = FocusTarget::Input;
        app.handle_paste("hello\nworld");
        assert!(app.needs_redraw);
        let lines = app.chat_widget.textarea.lines();
        let content = lines.join("\n");
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
    }

    #[test]
    fn test_handle_paste_in_chat_panel() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        app.focus = FocusTarget::ChatPanel;
        app.needs_redraw = false;
        app.handle_paste("hello");
        assert!(!app.needs_redraw);
    }

    #[test]
    fn test_handle_ipc_message() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new("test".into(), AppEventSender::new(tx));
        let msg = crown_core::ipc::message::make_notification(
            "assistant_text",
            serde_json::json!({"delta": "hello"}),
        );
        app.handle_ipc_message(msg);
        assert!(app.chat_widget.active_cell.is_some());
        assert!(app.needs_redraw);
    }
}
