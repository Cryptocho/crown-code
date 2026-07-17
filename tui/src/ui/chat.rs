use crate::chatwidget::ChatWidget;
use ratatui::{buffer::Buffer, layout::Rect};

pub fn render_chat_panel(area: Rect, buf: &mut Buffer, chat: &ChatWidget) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width;
    let visible_height = area.height as usize;

    let mut all_lines = Vec::new();
    for cell in &chat.cells {
        all_lines.extend(cell.display_lines(width));
    }
    if let Some(ref active) = chat.active_cell {
        all_lines.extend(active.display_lines(width));
    }

    let total_lines = all_lines.len();
    let end = total_lines.saturating_sub(chat.scroll_offset);
    let start = end.saturating_sub(visible_height);
    let visible = &all_lines[start..end];

    for (i, line) in visible.iter().enumerate() {
        let y = area.y + i as u16;
        clear_line(buf, area.x, y, area.width);
        buf.set_line(area.x, y, line, area.width);
    }

    for i in visible.len()..visible_height {
        let y = area.y + i as u16;
        clear_line(buf, area.x, y, area.width);
    }
}

fn clear_line(buf: &mut Buffer, x: u16, y: u16, width: u16) {
    for col in x..x + width {
        buf[(col, y)].set_symbol(" ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chatwidget::ChatWidget;
    use crate::history_cell::{
        AssistantMessageCell, ToolCallCell, ToolCallStatus, UserMessageCell,
    };

    fn line_content(buf: &Buffer, area: Rect, line: u16) -> String {
        let mut s = String::new();
        for x in area.x..area.x + area.width {
            s.push_str(buf[(x, area.y + line)].symbol());
        }
        s
    }

    fn all_content(buf: &Buffer, area: Rect) -> String {
        let mut s = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn test_empty_chat_renders_blank() {
        let chat = ChatWidget::new();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        for y in 0..24 {
            for x in 0..80 {
                assert_eq!(buf[(x, y)].symbol(), " ");
            }
        }
    }

    #[test]
    fn test_single_user_message() {
        let mut chat = ChatWidget::new();
        chat.push_cell(Box::new(UserMessageCell {
            content: "hello".into(),
        }));
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        let first_line = line_content(&buf, area, 0);
        assert!(first_line.contains("[You]"));
        assert!(first_line.contains("hello"));
    }

    #[test]
    fn test_scroll_offset_hides_bottom_content() {
        let mut chat = ChatWidget::new();
        for i in 0..20 {
            chat.push_cell(Box::new(UserMessageCell {
                content: format!("msg {i}"),
            }));
        }
        chat.scroll_up(5);
        let area = Rect::new(0, 0, 80, 10);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        let last_visible = line_content(&buf, area, 9);
        assert!(!last_visible.contains("msg 19"));
    }

    #[test]
    fn test_active_streaming_cell_included() {
        let mut chat = ChatWidget::new();
        chat.push_cell(Box::new(UserMessageCell {
            content: "hi".into(),
        }));
        chat.start_streaming();
        chat.append_streaming("streaming...");
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        let content = all_content(&buf, area);
        assert!(content.contains("[Assistant]"));
        assert!(content.contains("streaming..."));
    }

    #[test]
    fn test_tool_call_in_chat() {
        let mut chat = ChatWidget::new();
        chat.push_cell(Box::new(UserMessageCell {
            content: "read file".into(),
        }));
        chat.start_tool_call("c1", "read_file", "/tmp/test.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        chat.finish_tool_call("c1", "read_file", "content here", false);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        let content = all_content(&buf, area);
        assert!(content.contains("read_file"));
    }
}
