use crate::chatwidget::ChatWidget;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget},
};

pub fn render_chat_panel(area: Rect, buf: &mut Buffer, chat: &ChatWidget) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let (content_width, scrollbar_area) = if area.width >= 2 {
        (area.width - 1, Rect::new(area.x + area.width - 1, area.y, 1, area.height))
    } else {
        (area.width, Rect::default())
    };

    let visible_height = area.height as usize;

    let mut all_lines = Vec::new();
    for cell in &chat.cells {
        all_lines.extend(cell.display_lines(content_width));
    }
    if let Some(ref active) = chat.active_cell {
        all_lines.extend(active.display_lines(content_width));
    }

    let total_lines = all_lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll_offset = chat.scroll_offset.min(max_scroll);
    let end = total_lines.saturating_sub(scroll_offset);
    let start = end.saturating_sub(visible_height);
    let visible = &all_lines[start..end];

    for (i, line) in visible.iter().enumerate() {
        let y = area.y + i as u16;
        clear_line(buf, area.x, y, content_width);
        buf.set_line(area.x, y, line, content_width);
    }

    for i in visible.len()..visible_height {
        let y = area.y + i as u16;
        clear_line(buf, area.x, y, content_width);
    }

    if total_lines > visible_height {
        let position = max_scroll.saturating_sub(scroll_offset);
        let mut state = ScrollbarState::new(max_scroll + 1)
            .position(position)
            .viewport_content_length(visible_height);
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .render(scrollbar_area, buf, &mut state);
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

    fn content_width(area: Rect) -> u16 {
        if area.width >= 2 {
            area.width - 1
        } else {
            area.width
        }
    }

    fn line_content(buf: &Buffer, area: Rect, line: u16) -> String {
        let w = content_width(area);
        let mut s = String::new();
        for x in area.x..area.x + w {
            s.push_str(buf[(x, area.y + line)].symbol());
        }
        s
    }

    fn all_content(buf: &Buffer, area: Rect) -> String {
        let w = content_width(area);
        let mut s = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + w {
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
        let w = content_width(area);
        for y in 0..24 {
            for x in 0..w {
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
        assert!(content.contains('\u{258C}'));
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

    #[test]
    fn test_scrollbar_rendered_when_overflow() {
        let mut chat = ChatWidget::new();
        for i in 0..50 {
            chat.push_cell(Box::new(UserMessageCell {
                content: format!("msg {i}"),
            }));
        }
        let area = Rect::new(0, 0, 80, 10);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        let scrollbar_x = area.x + area.width - 1;
        let mut has_scrollbar_char = false;
        for y in area.y..area.y + area.height {
            let sym = buf[(scrollbar_x, y)].symbol();
            if sym != " " {
                has_scrollbar_char = true;
                break;
            }
        }
        assert!(has_scrollbar_char, "scrollbar should be rendered when content overflows");
    }

    #[test]
    fn test_no_scrollbar_when_content_fits() {
        let mut chat = ChatWidget::new();
        chat.push_cell(Box::new(UserMessageCell {
            content: "short".into(),
        }));
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        let scrollbar_x = area.x + area.width - 1;
        for y in area.y..area.y + area.height {
            let sym = buf[(scrollbar_x, y)].symbol();
            assert_eq!(sym, " ", "scrollbar area should be blank when content fits");
        }
    }

    #[test]
    fn test_narrow_width_no_scrollbar() {
        let chat = ChatWidget::new();
        let area = Rect::new(0, 0, 1, 10);
        let mut buf = Buffer::empty(area);
        render_chat_panel(area, &mut buf, &chat);
        for y in 0..10 {
            assert_eq!(buf[(0, y)].symbol(), " ");
        }
    }
}
