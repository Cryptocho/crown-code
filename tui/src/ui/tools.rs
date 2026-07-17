use crate::history_cell::{HistoryCell, ToolCallCell};
use ratatui::text::Line;

pub fn tool_call_lines(cell: &ToolCallCell, width: u16) -> Vec<Line<'static>> {
    cell.display_lines(width)
}

pub fn tool_call_height(cell: &ToolCallCell, width: u16) -> u16 {
    cell.desired_height(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_cell::ToolCallStatus;

    fn make_tool_call(
        expanded: bool,
        status: ToolCallStatus,
        output: Option<&str>,
    ) -> ToolCallCell {
        ToolCallCell {
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
            arguments_summary: "{}".into(),
            status,
            output: output.map(String::from),
            expanded,
            elapsed_ms: Some(100),
        }
    }

    #[test]
    fn test_collapsed_tool_call_single_line() {
        let cell = make_tool_call(false, ToolCallStatus::Success, Some("output"));
        let lines = tool_call_lines(&cell, 80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_expanded_tool_call_multiple_lines() {
        let cell = make_tool_call(true, ToolCallStatus::Success, Some("line1\nline2\nline3"));
        let lines = tool_call_lines(&cell, 80);
        assert_eq!(lines.len(), 4); // 1 header + 3 output lines
    }

    #[test]
    fn test_running_tool_call_height() {
        let cell = make_tool_call(false, ToolCallStatus::Running, None);
        assert_eq!(tool_call_height(&cell, 80), 1);
    }
}
