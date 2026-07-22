use std::fmt::Debug;

use unicode_width::UnicodeWidthChar;

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
        let total_len: usize = "[You] "
            .chars()
            .map(|c| c.width().unwrap_or(0))
            .sum::<usize>()
            + self
                .content
                .chars()
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>();
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
        let total_len: usize = "[Assistant] "
            .chars()
            .map(|c| c.width().unwrap_or(0))
            .sum::<usize>()
            + self
                .content
                .chars()
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>()
            + if self.is_streaming { 1 } else { 0 };
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
        let fold = if self.expanded {
            "\u{25BC}"
        } else {
            "\u{25B6}"
        };
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
        // Account for the "  " prefix (2 chars) in width calculation
        let prefix_width = 2;
        let title_display_len: usize = prefix_width
            + header
                .chars()
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>()
            + tag.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>()
            + 1;
        if title_display_len < w {
            let padding = w - title_display_len;
            title.push_str(&" ".repeat(padding));
        } else {
            title.push(' ');
        }
        title.push_str(&tag);

        let title_line = format!("  {title}");
        let mut lines = wrap_text_to_lines(&title_line, width, status_color);

        if self.expanded
            && let Some(ref output) = self.output
        {
            for (i, line) in output.lines().enumerate() {
                let numbered = format!("{} | {}", i + 1, line);
                lines.extend(wrap_text_to_lines(&numbered, width, Color::Gray));
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
        let display_len: usize = self.content.chars().map(|c| c.width().unwrap_or(0)).sum();
        (display_len as u16).div_ceil(width.max(1)).max(1)
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
        let text_len: usize = format!("[Error {}] ", self.code)
            .chars()
            .map(|c| c.width().unwrap_or(0))
            .sum::<usize>()
            + self
                .message
                .chars()
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>();
        (text_len as u16).div_ceil(width.max(1)).max(1)
    }
}

#[derive(Debug)]
pub struct AgentMarkdownCell {
    pub markdown_source: String,
    pub is_streaming: bool,
}

impl AgentMarkdownCell {
    pub fn new_streaming() -> Self {
        Self {
            markdown_source: String::new(),
            is_streaming: true,
        }
    }

    pub fn finish(&mut self) {
        self.is_streaming = false;
    }
}

impl HistoryCell for AgentMarkdownCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let prefix = "[Assistant] ";
        let mut lines = vec![Line::from(Span::styled(
            prefix.to_string(),
            Style::default().fg(Color::Green),
        ))];
        let mut source = self.markdown_source.clone();
        if self.is_streaming {
            source.push('\u{258C}');
        }
        let mut content_lines =
            render_markdown(&source, width.saturating_sub(prefix_width(prefix)));
        lines.append(&mut content_lines);
        lines
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.display_lines(width).len() as u16
    }

    fn is_stream_continuation(&self) -> bool {
        self.is_streaming
    }

    fn append_delta(&mut self, delta: &str) {
        self.markdown_source.push_str(delta);
    }

    fn finish_streaming(&mut self) {
        self.is_streaming = false;
    }
}

use crate::markdown_render::render_markdown;

fn prefix_width(prefix: &str) -> u16 {
    prefix
        .chars()
        .map(|c| c.width().unwrap_or(0))
        .sum::<usize>() as u16
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
            // Find the split point using display width (not char count)
            let mut display_width = 0usize;
            let mut byte_end = remaining.len();
            for (i, ch) in remaining.char_indices() {
                let w = ch.width().unwrap_or(0);
                if display_width + w > width {
                    byte_end = i;
                    break;
                }
                display_width += w;
            }
            // Try to break at a space for cleaner output
            let split_at = if remaining.len() > byte_end {
                remaining[..byte_end]
                    .rfind(' ')
                    .map(|i| i + 1)
                    .unwrap_or(byte_end)
            } else {
                byte_end
            };
            // Avoid splitting at zero width (happens with wide char at boundary)
            let split_at = if split_at == 0 {
                byte_end.max(1)
            } else {
                split_at
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

// Kept for backward compatibility; prefer wrap_text_to_lines for new code.
#[allow(dead_code)]
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

    #[test]
    fn test_agent_markdown_cell_basic() {
        let cell = AgentMarkdownCell {
            markdown_source: "# Hello".to_string(),
            is_streaming: false,
        };
        let lines = cell.display_lines(80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("Hello"));
    }

    #[test]
    fn test_agent_markdown_cell_streaming_lifecycle() {
        let mut cell = AgentMarkdownCell::new_streaming();
        assert!(cell.is_stream_continuation());

        cell.append_delta("hello ");
        cell.append_delta("world");
        assert!(cell.is_stream_continuation());

        let lines = cell.display_lines(80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("hello world"));
        assert!(all_text.contains('\u{258C}'));

        cell.finish_streaming();
        assert!(!cell.is_stream_continuation());

        let lines = cell.display_lines(80);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(!all_text.contains('\u{258C}'));
    }

    #[test]
    fn test_agent_markdown_cell_resize_rerender() {
        let cell = AgentMarkdownCell {
            markdown_source: "This is a long line that should wrap at different widths".to_string(),
            is_streaming: false,
        };
        let lines_w80 = cell.display_lines(80);
        let lines_w40 = cell.display_lines(40);
        assert!(lines_w40.len() >= lines_w80.len());
    }

    #[test]
    fn test_agent_markdown_cell_desired_height_matches_display() {
        let cell = AgentMarkdownCell {
            markdown_source: "# Title\n\nSome content here".to_string(),
            is_streaming: false,
        };
        let display_lines = cell.display_lines(80);
        let desired = cell.desired_height(80);
        assert_eq!(display_lines.len() as u16, desired);
    }

    #[test]
    fn test_agent_markdown_cell_with_markdown() {
        let cell = AgentMarkdownCell {
            markdown_source: "**bold** and *italic*".to_string(),
            is_streaming: false,
        };
        let lines = cell.display_lines(80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("bold"));
        assert!(all_text.contains("italic"));
    }
}
