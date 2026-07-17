use std::collections::VecDeque;

use crate::app_event::AppEventSender;
use crate::chatwidget::ChatWidget;

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
}
