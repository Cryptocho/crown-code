use std::fmt::Debug;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub trait HistoryCell: Debug + Send + Sync {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn is_stream_continuation(&self) -> bool {
        false
    }
    fn append_delta(&mut self, _delta: &str) {}
    fn finish_streaming(&mut self) {}
    fn as_tool_call(&self) -> Option<&ToolCallCell> {
        None
    }
    fn as_tool_call_mut(&mut self) -> Option<&mut ToolCallCell> {
        None
    }
}

#[derive(Debug)]
pub struct UserMessageCell {
    pub content: String,
}

impl HistoryCell for UserMessageCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let prefix = "[You] ";
        let full = format!("{prefix}{}", self.content);
        wrap_text_to_lines(&full, width, Color::Cyan)
    }

    fn desired_height(&self, width: u16) -> u16 {
        let total_len = 6 + self.content.chars().count();
        (total_len as u16).div_ceil(width.max(1)).max(1)
    }
}

#[derive(Debug)]
pub struct AssistantMessageCell {
    pub content: String,
    pub is_streaming: bool,
}

impl AssistantMessageCell {
    pub fn new_streaming() -> Self {
        Self {
            content: String::new(),
            is_streaming: true,
        }
    }

    pub fn finish(&mut self) {
        self.is_streaming = false;
    }
}

impl HistoryCell for AssistantMessageCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let prefix = "[Assistant] ";
        let mut full = format!("{prefix}{}", self.content);
        if self.is_streaming {
            full.push('\u{258C}');
        }
        wrap_text_to_lines(&full, width, Color::Green)
    }

    fn desired_height(&self, width: u16) -> u16 {
        let total_len = 11 + self.content.len() + if self.is_streaming { 1 } else { 0 };
        (total_len as u16).div_ceil(width.max(1)).max(1)
    }

    fn is_stream_continuation(&self) -> bool {
        self.is_streaming
    }

    fn append_delta(&mut self, delta: &str) {
        self.content.push_str(delta);
    }

    fn finish_streaming(&mut self) {
        self.is_streaming = false;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStatus {
    Running,
    Success,
    Error,
}

#[derive(Debug)]
pub struct ToolCallCell {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    pub arguments_summary: String,
    pub status: ToolCallStatus,
    pub output: Option<String>,
    pub expanded: bool,
    pub elapsed_ms: Option<u64>,
}

impl HistoryCell for ToolCallCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let w = width as usize;
        let fold = if self.expanded { "\u{25BC}" } else { "\u{25B6}" };
        let (status_icon, status_color) = match self.status {
            ToolCallStatus::Running => ("\u{27F3}", Color::Yellow),
            ToolCallStatus::Success => ("\u{2713}", Color::Green),
            ToolCallStatus::Error => ("\u{2717}", Color::Red),
        };
        let elapsed_str = match (&self.status, self.elapsed_ms) {
            (ToolCallStatus::Success | ToolCallStatus::Error, Some(ms)) => format!("{ms}ms"),
            _ => "running".to_string(),
        };

        let header = format!("{fold} {} \"{}\"", self.name, self.arguments_summary);
        let tag = format!("[{status_icon} {elapsed_str}]");

        let mut title = header.clone();
        let title_display_len = header.chars().count() + tag.chars().count() + 1;
        if title_display_len < w {
            let padding = w - title_display_len;
            title.push_str(&" ".repeat(padding));
        } else {
            title.push(' ');
        }
        title.push_str(&tag);

        let mut lines = vec![Line::from(vec![
            Span::styled(format!("  {title}"), Style::default().fg(status_color)),
        ])];

        if self.expanded
            && let Some(ref output) = self.output
        {
            for (i, line) in output.lines().enumerate() {
                let numbered = format!("    {} | {}", i + 1, line);
                lines.push(make_line(&numbered, width, Color::Gray));
            }
        }

        lines
    }

    fn desired_height(&self, _width: u16) -> u16 {
        if self.expanded {
            let output_lines = self
                .output
                .as_ref()
                .map(|o| o.lines().count() as u16)
                .unwrap_or(0);
            1 + output_lines
        } else {
            1
        }
    }

    fn as_tool_call(&self) -> Option<&ToolCallCell> {
        Some(self)
    }

    fn as_tool_call_mut(&mut self) -> Option<&mut ToolCallCell> {
        Some(self)
    }
}

#[derive(Debug)]
pub struct SystemMessageCell {
    pub content: String,
}

impl HistoryCell for SystemMessageCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        wrap_text_to_lines(&self.content, width, Color::DarkGray)
    }

    fn desired_height(&self, width: u16) -> u16 {
        (self.content.len() as u16).div_ceil(width.max(1)).max(1)
    }
}

#[derive(Debug)]
pub struct ErrorCell {
    pub code: i32,
    pub message: String,
}

impl HistoryCell for ErrorCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let text = format!("[Error {}] {}", self.code, self.message);
        wrap_text_to_lines(&text, width, Color::Red)
    }

    fn desired_height(&self, width: u16) -> u16 {
        let text_len = 10 + self.message.len();
        (text_len as u16).div_ceil(width.max(1)).max(1)
    }
}

fn wrap_text_to_lines(text: &str, width: u16, color: Color) -> Vec<Line<'static>> {
    let width = width as usize;
    if width == 0 {
        return vec![];
    }

    let mut lines = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        let mut remaining = line;
        while !remaining.is_empty() {
            let byte_end = remaining
                .char_indices()
                .nth(width)
                .map(|(i, _)| i)
                .unwrap_or(remaining.len());
            let split_at = if remaining.len() > byte_end {
                remaining[..byte_end]
                    .rfind(' ')
                    .map(|i| i + 1)
                    .unwrap_or(byte_end)
            } else {
                byte_end
            };
            let (chunk, rest) = remaining.split_at(split_at);
            lines.push(Line::from(Span::styled(
                chunk.to_string(),
                Style::default().fg(color),
            )));
            remaining = rest.trim_start();
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

fn make_line(text: &str, width: u16, color: Color) -> Line<'static> {
    let truncated: String = text.chars().take(width as usize).collect();
    Line::from(Span::styled(truncated, Style::default().fg(color)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_message_cell_height() {
        let cell = UserMessageCell {
            content: "hello".to_string(),
        };
        assert_eq!(cell.desired_height(80), 1);
    }

    #[test]
    fn test_user_message_cell_wrapping() {
        let cell = UserMessageCell {
            content: "a".repeat(100),
        };
        let lines = cell.display_lines(40);
        assert!(lines.len() > 1);
    }

    #[test]
    fn test_assistant_streaming_delta() {
        let mut cell = AssistantMessageCell::new_streaming();
        assert!(cell.is_stream_continuation());
        cell.append_delta("hello ");
        cell.append_delta("world");
        let lines = cell.display_lines(80);
        assert!(lines.len() >= 1);
        let text = format!("{lines:?}");
        assert!(text.contains('\u{258C}'));
    }

    #[test]
    fn test_assistant_finish() {
        let mut cell = AssistantMessageCell::new_streaming();
        cell.append_delta("done");
        cell.finish();
        assert!(!cell.is_stream_continuation());
    }

    #[test]
    fn test_tool_call_cell_status_icons() {
        let cell = ToolCallCell {
            call_id: "c1".to_string(),
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            arguments_summary: "{}".to_string(),
            status: ToolCallStatus::Running,
            output: None,
            expanded: false,
            elapsed_ms: None,
        };
        let lines = cell.display_lines(80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_tool_call_collapsed_hides_output() {
        let cell = ToolCallCell {
            call_id: "c1".to_string(),
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            arguments_summary: "{}".to_string(),
            status: ToolCallStatus::Success,
            output: Some("line1\nline2\nline3".to_string()),
            expanded: false,
            elapsed_ms: Some(100),
        };
        let lines = cell.display_lines(80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_tool_call_expanded_shows_output_with_line_numbers() {
        let cell = ToolCallCell {
            call_id: "c1".to_string(),
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            arguments_summary: "{}".to_string(),
            status: ToolCallStatus::Success,
            output: Some("line1\nline2\nline3".to_string()),
            expanded: true,
            elapsed_ms: Some(100),
        };
        let lines = cell.display_lines(80);
        assert_eq!(lines.len(), 4);
        let line_text = format!("{:?}", lines[1]);
        assert!(line_text.contains("1 |"));
        let line_text2 = format!("{:?}", lines[2]);
        assert!(line_text2.contains("2 |"));
    }

    #[test]
    fn test_tool_call_elapsed_display() {
        let cell = ToolCallCell {
            call_id: "c1".to_string(),
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            arguments_summary: "{}".to_string(),
            status: ToolCallStatus::Success,
            output: None,
            expanded: false,
            elapsed_ms: Some(100),
        };
        let lines = cell.display_lines(80);
        let line_text = format!("{:?}", lines[0]);
        assert!(line_text.contains("100ms"));
    }

    #[test]
    fn test_tool_call_running_no_elapsed() {
        let cell = ToolCallCell {
            call_id: "c1".to_string(),
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            arguments_summary: "{}".to_string(),
            status: ToolCallStatus::Running,
            output: None,
            expanded: false,
            elapsed_ms: None,
        };
        let lines = cell.display_lines(80);
        let line_text = format!("{:?}", lines[0]);
        assert!(line_text.contains("running"));
    }

    #[test]
    fn test_finish_streaming_trait() {
        let mut cell = AssistantMessageCell::new_streaming();
        assert!(cell.is_stream_continuation());
        HistoryCell::finish_streaming(&mut cell);
        assert!(!cell.is_stream_continuation());
    }

    #[test]
    fn test_tool_call_as_tool_call() {
        let mut cell = ToolCallCell {
            call_id: "c1".to_string(),
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            arguments_summary: "{}".to_string(),
            status: ToolCallStatus::Running,
            output: None,
            expanded: false,
            elapsed_ms: None,
        };
        assert!(cell.as_tool_call().is_some());
        assert!(cell.as_tool_call_mut().is_some());
    }

    #[test]
    fn test_user_cell_as_tool_call_none() {
        let mut cell = UserMessageCell {
            content: "hi".to_string(),
        };
        assert!(cell.as_tool_call().is_none());
        assert!(cell.as_tool_call_mut().is_none());
    }

    #[test]
    fn test_error_cell_format() {
        let cell = ErrorCell {
            code: 401,
            message: "unauthorized".to_string(),
        };
        let lines = cell.display_lines(80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_system_message_cell() {
        let cell = SystemMessageCell {
            content: "session started".to_string(),
        };
        assert_eq!(cell.desired_height(80), 1);
    }

    #[test]
    fn test_wrap_empty_text() {
        let lines = wrap_text_to_lines("", 80, Color::White);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_wrap_long_line() {
        let text = "word ".repeat(20);
        let lines = wrap_text_to_lines(&text, 40, Color::White);
        assert!(lines.len() >= 3);
    }
}
