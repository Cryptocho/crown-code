use std::collections::HashMap;
use std::time::Instant;

use tui_textarea::TextArea;

use crate::history_cell::{
    AssistantMessageCell, HistoryCell, ToolCallCell, ToolCallStatus, UserMessageCell,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallTracker {
    pub cell_index: usize,
    pub started_at: Instant,
}

pub struct ChatWidget {
    pub cells: Vec<Box<dyn HistoryCell>>,
    pub active_cell: Option<Box<dyn HistoryCell>>,
    pub textarea: TextArea<'static>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    tool_trackers: HashMap<String, ToolCallTracker>,
}

impl ChatWidget {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("请输入你的任务...");
        Self {
            cells: Vec::new(),
            active_cell: None,
            textarea,
            scroll_offset: 0,
            auto_scroll: true,
            tool_trackers: HashMap::new(),
        }
    }

    pub fn push_cell(&mut self, cell: Box<dyn HistoryCell>) {
        self.cells.push(cell);
        if self.auto_scroll {
            self.scroll_offset = 0;
        }
    }

    pub fn start_streaming(&mut self) {
        self.active_cell = Some(Box::new(AssistantMessageCell::new_streaming()));
    }

    pub fn append_streaming(&mut self, delta: &str) {
        if let Some(ref mut cell) = self.active_cell {
            cell.append_delta(delta);
        }
    }

    pub fn finish_streaming(&mut self) {
        if let Some(mut cell) = self.active_cell.take() {
            cell.finish_streaming();
            self.cells.push(cell);
            if self.auto_scroll {
                self.scroll_offset = 0;
            }
        }
    }

    pub fn start_tool_call(&mut self, call_id: &str, name: &str, arguments: &str) {
        self.finish_streaming();

        let args_summary = if arguments.chars().count() > 80 {
            let truncated: String = arguments.chars().take(77).collect();
            format!("{truncated}...")
        } else {
            arguments.to_string()
        };

        let index = self.cells.len();
        self.cells.push(Box::new(ToolCallCell {
            call_id: call_id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
            arguments_summary: args_summary,
            status: ToolCallStatus::Running,
            output: None,
            expanded: false,
            elapsed_ms: None,
        }));
        self.tool_trackers.insert(
            call_id.to_string(),
            ToolCallTracker {
                cell_index: index,
                started_at: Instant::now(),
            },
        );
        if self.auto_scroll {
            self.scroll_offset = 0;
        }
    }

    pub fn finish_tool_call(&mut self, call_id: &str, _name: &str, content: &str, is_error: bool) {
        if let Some(tracker) = self.tool_trackers.remove(call_id) {
            let elapsed = tracker.started_at.elapsed().as_millis() as u64;
            if let Some(cell) = self.cells.get_mut(tracker.cell_index)
                && let Some(tc) = cell.as_tool_call_mut()
            {
                tc.status = if is_error {
                    ToolCallStatus::Error
                } else {
                    ToolCallStatus::Success
                };
                tc.output = Some(content.to_string());
                tc.elapsed_ms = Some(elapsed);
            }
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        if self.scroll_offset == 0 {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = true;
    }

    pub fn input_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        self.textarea.input(key)
    }

    pub fn take_input(&mut self) -> String {
        let content: String = self.textarea.lines().join("\n");
        self.textarea = TextArea::default();
        self.textarea.set_placeholder_text("请输入你的任务...");
        content
    }

    pub fn input_is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    pub fn toggle_tool_expanded(&mut self, index: usize) {
        if let Some(cell) = self.cells.get_mut(index)
            && let Some(tc) = cell.as_tool_call_mut()
        {
            tc.expanded = !tc.expanded;
        }
    }

    pub fn is_tool_call(&self, index: usize) -> bool {
        self.cells
            .get(index)
            .and_then(|c| c.as_tool_call())
            .is_some()
    }

    pub fn total_rendered_lines(&self, width: u16) -> usize {
        let mut total = 0usize;
        for cell in &self.cells {
            total += cell.desired_height(width) as usize;
        }
        if let Some(ref active) = self.active_cell {
            total += active.desired_height(width) as usize;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_chatwidget() {
        let w = ChatWidget::new();
        assert!(w.cells.is_empty());
        assert!(w.active_cell.is_none());
        assert_eq!(w.scroll_offset, 0);
        assert!(w.auto_scroll);
        assert!(w.tool_trackers.is_empty());
    }

    #[test]
    fn test_push_cell_and_auto_scroll() {
        let mut w = ChatWidget::new();
        w.scroll_offset = 5;
        w.push_cell(Box::new(UserMessageCell {
            content: "hi".into(),
        }));
        assert_eq!(w.cells.len(), 1);
        assert_eq!(w.scroll_offset, 0);
    }

    #[test]
    fn test_streaming_lifecycle() {
        let mut w = ChatWidget::new();
        w.start_streaming();
        assert!(w.active_cell.is_some());
        assert!(w.active_cell.as_ref().unwrap().is_stream_continuation());

        w.append_streaming("hello ");
        w.append_streaming("world");
        w.finish_streaming();

        assert!(w.active_cell.is_none());
        assert_eq!(w.cells.len(), 1);
        assert!(!w.cells[0].is_stream_continuation());
    }

    #[test]
    fn test_finish_streaming_noop_when_empty() {
        let mut w = ChatWidget::new();
        w.finish_streaming();
        assert!(w.cells.is_empty());
    }

    #[test]
    fn test_tool_call_lifecycle() {
        let mut w = ChatWidget::new();
        w.start_tool_call("c1", "read_file", "/tmp/test.rs");
        assert_eq!(w.cells.len(), 1);
        assert!(w.is_tool_call(0));
        assert!(w.tool_trackers.contains_key("c1"));

        std::thread::sleep(std::time::Duration::from_millis(10));
        w.finish_tool_call("c1", "read_file", "file content", false);

        let tc = w.cells[0].as_tool_call().unwrap();
        assert_eq!(tc.status, ToolCallStatus::Success);
        assert_eq!(tc.output.as_deref(), Some("file content"));
        assert!(tc.elapsed_ms.is_some());
        assert!(tc.elapsed_ms.unwrap() >= 10);
        assert!(!w.tool_trackers.contains_key("c1"));
    }

    #[test]
    fn test_tool_call_error() {
        let mut w = ChatWidget::new();
        w.start_tool_call("c2", "write_file", "/tmp/x");
        w.finish_tool_call("c2", "write_file", "Error: permission denied", true);
        let tc = w.cells[0].as_tool_call().unwrap();
        assert_eq!(tc.status, ToolCallStatus::Error);
    }

    #[test]
    fn test_tool_call_finish_nonexistent() {
        let mut w = ChatWidget::new();
        w.finish_tool_call("nonexistent", "x", "y", false);
        assert!(w.cells.is_empty());
    }

    #[test]
    fn test_start_tool_call_interrupts_streaming() {
        let mut w = ChatWidget::new();
        w.start_streaming();
        w.append_streaming("text before tool");
        w.start_tool_call("c1", "read_file", "{}");
        assert_eq!(w.cells.len(), 2);
        assert!(!w.cells[0].is_stream_continuation());
    }

    #[test]
    fn test_scroll_operations() {
        let mut w = ChatWidget::new();
        assert!(w.auto_scroll);

        w.scroll_up(5);
        assert_eq!(w.scroll_offset, 5);
        assert!(!w.auto_scroll);

        w.scroll_up(3);
        assert_eq!(w.scroll_offset, 8);

        w.scroll_down(3);
        assert_eq!(w.scroll_offset, 5);
        assert!(!w.auto_scroll);

        w.scroll_down(5);
        assert_eq!(w.scroll_offset, 0);
        assert!(w.auto_scroll);

        w.scroll_up(10);
        w.scroll_to_bottom();
        assert_eq!(w.scroll_offset, 0);
        assert!(w.auto_scroll);
    }

    #[test]
    fn test_toggle_tool_expanded() {
        let mut w = ChatWidget::new();
        w.start_tool_call("c1", "read_file", "{}");
        let tc = w.cells[0].as_tool_call().unwrap();
        assert!(!tc.expanded);

        w.toggle_tool_expanded(0);
        let tc = w.cells[0].as_tool_call().unwrap();
        assert!(tc.expanded);

        w.toggle_tool_expanded(0);
        let tc = w.cells[0].as_tool_call().unwrap();
        assert!(!tc.expanded);
    }

    #[test]
    fn test_toggle_nonexistent_index() {
        let mut w = ChatWidget::new();
        w.toggle_tool_expanded(99);
    }

    #[test]
    fn test_is_tool_call() {
        let mut w = ChatWidget::new();
        w.push_cell(Box::new(UserMessageCell {
            content: "hi".into(),
        }));
        w.start_tool_call("c1", "read_file", "{}");
        assert!(!w.is_tool_call(0));
        assert!(w.is_tool_call(1));
        assert!(!w.is_tool_call(2));
    }

    #[test]
    fn test_take_input() {
        let mut w = ChatWidget::new();
        assert!(w.input_is_empty());
        let content = w.take_input();
        assert!(content.is_empty());
    }

    #[test]
    fn test_total_rendered_lines() {
        let mut w = ChatWidget::new();
        assert_eq!(w.total_rendered_lines(80), 0);

        w.push_cell(Box::new(UserMessageCell {
            content: "hello".into(),
        }));
        assert_eq!(w.total_rendered_lines(80), 1);

        w.start_streaming();
        assert_eq!(w.total_rendered_lines(80), 2);
    }
}
