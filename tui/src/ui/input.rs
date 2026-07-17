use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use tui_textarea::TextArea;
use unicode_width::UnicodeWidthStr;

use crate::app::AgentMode;

pub struct InputBarData<'a> {
    pub model: &'a str,
    pub agent_mode: &'a AgentMode,
    pub textarea: &'a TextArea<'static>,
    pub focus: bool,
}

pub fn render_input_bar(area: Rect, buf: &mut Buffer, data: &InputBarData) {
    if area.height == 0 {
        return;
    }

    let width = area.width as usize;
    let input_line_y = area.y;

    // Line 0: input field
    let prefix = if data.model.is_empty() {
        format!("{} │ > ", data.agent_mode.label())
    } else {
        format!("{} │ {} │ > ", data.model, data.agent_mode.label())
    };
    let prefix_len = UnicodeWidthStr::width(prefix.as_str());
    let prefix_style = Style::default().fg(Color::DarkGray);

    // Render prefix
    buf.set_string(area.x, input_line_y, &prefix, prefix_style);

    // Render textarea content
    let textarea_x = area.x + prefix_len as u16;
    let textarea_width = width.saturating_sub(prefix_len).max(1) as u16;
    let lines = data.textarea.lines();
    let text = lines.first().map(|s| s.as_str()).unwrap_or("");

    // Clear textarea area
    for x in textarea_x..textarea_x + textarea_width {
        buf[(x, input_line_y)].set_symbol(" ");
    }

    // Display text (truncate if needed)
    let display_text: String = text.chars().take(textarea_width as usize).collect();
    buf.set_string(
        textarea_x,
        input_line_y,
        &display_text,
        Style::default().fg(Color::White),
    );

    // Cursor (reverse video) when focused
    if data.focus {
        let cursor_col = data.textarea.cursor().1;
        // cursor_col is char index, but display width may differ (CJK = 2 cols)
        let text_before: String = text.chars().take(cursor_col).collect();
        let cursor_display_offset = UnicodeWidthStr::width(text_before.as_str());
        let cursor_x = textarea_x + cursor_display_offset as u16;
        if cursor_x >= textarea_x && cursor_x < textarea_x + textarea_width {
            buf[(cursor_x, input_line_y)]
                .set_style(Style::default().add_modifier(Modifier::REVERSED));
        }
    }

    // Fill remaining space
    let used_width = prefix_len + UnicodeWidthStr::width(display_text.as_str());
    for x in (area.x + used_width as u16)..(area.x + area.width) {
        buf[(x, input_line_y)].set_symbol(" ");
    }

    // Line 1: hint (only if height >= 2)
    if area.height >= 2 {
        let hint_line_y = area.y + 1;
        let hint = "Enter 发送 · Ctrl+C 退出 · Tab 切换焦点 · Ctrl+P 切换模式";
        let hint_style = Style::default().fg(Color::DarkGray);
        buf.set_string(area.x, hint_line_y, hint, hint_style);
        let hint_width = UnicodeWidthStr::width(hint).min(width);
        for x in (area.x + hint_width as u16)..(area.x + area.width) {
            buf[(x, hint_line_y)].set_symbol(" ");
        }
    }

    // Clear extra lines (height > 2)
    for extra_y in (area.y + 2)..(area.y + area.height) {
        for x in area.x..area.x + area.width {
            buf[(x, extra_y)].set_symbol(" ");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tui_textarea::TextArea;

    fn line_content(buf: &Buffer, area: Rect, line: u16) -> String {
        let mut s = String::new();
        for x in area.x..area.x + area.width {
            s.push_str(buf[(x, area.y + line)].symbol());
        }
        s
    }

    #[test]
    fn test_input_bar_normal_rendering() {
        let mut textarea = TextArea::default();
        textarea.insert_str("hello world");
        let data = InputBarData {
            model: "gpt-4o",
            agent_mode: &AgentMode::Code,
            textarea: &textarea,
            focus: true,
        };
        let area = Rect::new(0, 0, 80, 2);
        let mut buf = Buffer::empty(area);
        render_input_bar(area, &mut buf, &data);
        let line0 = line_content(&buf, area, 0);
        assert!(line0.contains("gpt-4o"));
        assert!(line0.contains("[Code]"));
        assert!(line0.contains("hello world"));
        let line1 = line_content(&buf, area, 1);
        // CJK chars are double-width in buffer, so check for English prefix only
        assert!(line1.contains("Enter"));
        assert!(line1.contains("Ctrl+C"));
    }

    #[test]
    fn test_input_bar_empty_input_shows_prefix() {
        let textarea = TextArea::default();
        let data = InputBarData {
            model: "claude-3",
            agent_mode: &AgentMode::Plan,
            textarea: &textarea,
            focus: false,
        };
        let area = Rect::new(0, 0, 80, 2);
        let mut buf = Buffer::empty(area);
        render_input_bar(area, &mut buf, &data);
        let line0 = line_content(&buf, area, 0);
        assert!(line0.contains("claude-3"));
        assert!(line0.contains("[Plan]"));
    }

    #[test]
    fn test_input_bar_empty_model() {
        let textarea = TextArea::default();
        let data = InputBarData {
            model: "",
            agent_mode: &AgentMode::Ask,
            textarea: &textarea,
            focus: false,
        };
        let area = Rect::new(0, 0, 80, 2);
        let mut buf = Buffer::empty(area);
        render_input_bar(area, &mut buf, &data);
        let line0 = line_content(&buf, area, 0);
        assert!(line0.contains("[Ask]"));
    }

    #[test]
    fn test_input_bar_minimum_height() {
        let textarea = TextArea::default();
        let data = InputBarData {
            model: "",
            agent_mode: &AgentMode::Code,
            textarea: &textarea,
            focus: false,
        };
        // Height 1: only render input line, no hint line
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        render_input_bar(area, &mut buf, &data);
        // Height 0: no panic
        let area0 = Rect::new(0, 0, 80, 0);
        let mut buf0 = Buffer::empty(area0);
        render_input_bar(area0, &mut buf0, &data);
    }
}
