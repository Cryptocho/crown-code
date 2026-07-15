use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    UserMessageSent(String),
    CancelRequested,
    Quit,

    AssistantDelta {
        delta: String,
    },
    ReasoningDelta {
        delta: String,
    },
    ToolCallStart {
        call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
    },
    Usage {
        input_tokens: i32,
        output_tokens: i32,
        cache_read_tokens: i32,
    },
    TaskDone {
        summary: String,
    },
    Error {
        code: i32,
        message: String,
    },

    RedrawRequested,
}

pub struct AppEventSender {
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl AppEventSender {
    pub fn new(tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        Self { tx }
    }

    pub fn send(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }
}

impl Clone for AppEventSender {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}
