use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, SessionStatus};

pub struct StatusBarData<'a> {
    pub session_name: Option<&'a str>,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cache_read_tokens: i32,
    pub avg_latency_ms: Option<u64>,
    pub status: &'a SessionStatus,
}

pub fn status_bar_data_from_app(app: &App) -> StatusBarData<'_> {
    StatusBarData {
        session_name: app.session_name.as_deref(),
        input_tokens: app.input_tokens,
        output_tokens: app.output_tokens,
        cache_read_tokens: app.cache_read_tokens,
        avg_latency_ms: app.avg_latency(),
        status: &app.status,
    }
}

pub fn render_status_bar(area: Rect, buf: &mut Buffer, data: &StatusBarData) {
    if area.height == 0 {
        return;
    }
    let width = area.width as usize;
    let y = area.y;
    let bg_style = Style::default().bg(Color::DarkGray);
    let sep = " │ "; // 3 columns

    let name_seg = data.session_name.unwrap_or("crown-code");
    let token_seg = format!(
        "In:{} Out:{} Cache R:{}",
        data.input_tokens, data.output_tokens, data.cache_read_tokens
    );
    let latency_seg = data.avg_latency_ms.map(|ms| format!("avg:{ms}ms"));
    let status_icon = "●";
    let status_color = match data.status {
        SessionStatus::Active => Color::Green,
        SessionStatus::Completed => Color::Blue,
        SessionStatus::Error => Color::Red,
    };

    // Build right side segments from lowest priority (P4) upward
    let mut right: Vec<(&str, Color)> = Vec::new();

    // P4: icon (always)
    right.push((status_icon, status_color));

    // P3: latency
    if let Some(ref lat) = latency_seg {
        let candidate =
            segments_width(&right, sep) + sep.len() + UnicodeWidthStr::width(lat.as_str());
        if candidate <= width {
            right.push((lat.as_str(), Color::Yellow));
        }
    }

    // P2: token
    {
        let candidate =
            segments_width(&right, sep) + sep.len() + UnicodeWidthStr::width(token_seg.as_str());
        if candidate <= width {
            right.push((token_seg.as_str(), Color::White));
        }
    }

    // Reverse so order is: token | latency | icon (leftmost in right side)
    right.reverse();

    let right_width = segments_width(&right, sep);
    let has_right = !right.is_empty();

    // Calculate name available width: total minus right side
    // (right side includes separators between right segments)
    // plus the separator " │ " between name and right
    let name_avail = if has_right {
        width.saturating_sub(right_width + sep.len())
        // name_avail can be 0, that's fine
        // but we need to ensure total doesn't exceed width
    } else {
        width
    };
    let name_display = truncate_str(name_seg, name_avail);
    let name_w = UnicodeWidthStr::width(name_display.as_str());

    let mut x: usize = area.x as usize;

    // Render name
    buf.set_string(x as u16, y, &name_display, bg_style);
    x += name_w;

    if has_right {
        let right_space = right_width + sep.len();
        let fill_needed = width - right_space - name_w;
        for _ in 0..fill_needed {
            buf.set_string(x as u16, y, " ", bg_style);
            x += 1;
        }

        // Separator between name and right side
        buf.set_string(
            x as u16,
            y,
            sep,
            Style::default().fg(Color::DarkGray).bg(Color::DarkGray),
        );
        x += sep.len();

        // Right segments with inter-separators
        for (i, (text, color)) in right.iter().enumerate() {
            if i > 0 {
                buf.set_string(
                    x as u16,
                    y,
                    sep,
                    Style::default().fg(Color::DarkGray).bg(Color::DarkGray),
                );
                x += sep.len();
            }
            buf.set_string(
                x as u16,
                y,
                text,
                Style::default().fg(*color).bg(Color::DarkGray),
            );
            x += UnicodeWidthStr::width(*text);
        }
    }

    // Fill trailing spaces
    while x < width {
        buf.set_string(x as u16, y, " ", bg_style);
        x += 1;
    }
}

fn segments_width(segments: &[(&str, Color)], sep: &str) -> usize {
    if segments.is_empty() {
        return 0;
    }
    let text_total: usize = segments
        .iter()
        .map(|(text, _)| UnicodeWidthStr::width(*text))
        .sum();
    text_total + sep.len() * (segments.len() - 1)
}

fn truncate_str(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }
    let mut result = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_width {
            break;
        }
        result.push(ch);
        w += cw;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf_content(buf: &Buffer, area: Rect) -> String {
        let mut s = String::new();
        for x in area.x..area.x + area.width {
            s.push_str(buf[(x, area.y)].symbol());
        }
        s
    }

    #[test]
    fn test_full_width_shows_all_segments() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        let data = StatusBarData {
            session_name: Some("my project"),
            input_tokens: 1234,
            output_tokens: 567,
            cache_read_tokens: 890,
            avg_latency_ms: Some(230),
            status: &SessionStatus::Active,
        };
        render_status_bar(area, &mut buf, &data);
        let content = buf_content(&buf, area);
        assert!(content.contains("my project"));
        assert!(content.contains("In:1234"));
        assert!(content.contains("Out:567"));
        assert!(content.contains("Cache R:890"));
        assert!(content.contains("avg:230ms"));
        assert!(content.contains("●"));
    }

    #[test]
    fn test_narrow_width_drops_low_priority() {
        let area = Rect::new(0, 0, 30, 1);
        let mut buf = Buffer::empty(area);
        let data = StatusBarData {
            session_name: Some("my project"),
            input_tokens: 1234,
            output_tokens: 567,
            cache_read_tokens: 890,
            avg_latency_ms: Some(230),
            status: &SessionStatus::Active,
        };
        render_status_bar(area, &mut buf, &data);
        let content = buf_content(&buf, area);
        assert!(content.contains("my project"));
        // In width 30, token (29 chars) doesn't fit with icon + latency + name + seps
        assert!(!content.contains("In:1234"));
    }

    #[test]
    fn test_very_narrow_truncates_name() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        let data = StatusBarData {
            session_name: Some("a very long session name"),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            avg_latency_ms: None,
            status: &SessionStatus::Active,
        };
        render_status_bar(area, &mut buf, &data);
        // No panic
    }

    #[test]
    fn test_status_colors() {
        for (status, expected_color) in [
            (SessionStatus::Active, Color::Green),
            (SessionStatus::Completed, Color::Blue),
            (SessionStatus::Error, Color::Red),
        ] {
            let area = Rect::new(0, 0, 80, 1);
            let mut buf = Buffer::empty(area);
            let data = StatusBarData {
                session_name: None,
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                avg_latency_ms: None,
                status: &status,
            };
            render_status_bar(area, &mut buf, &data);
            let status_cell = (0..80)
                .find(|&x| buf[(x, 0)].symbol() == "●")
                .expect("status indicator should be present");
            assert_eq!(buf[(status_cell, 0)].style().fg, Some(expected_color));
        }
    }

    #[test]
    fn test_zero_height_no_panic() {
        let area = Rect::new(0, 0, 80, 0);
        let mut buf = Buffer::empty(area);
        let data = StatusBarData {
            session_name: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            avg_latency_ms: None,
            status: &SessionStatus::Active,
        };
        render_status_bar(area, &mut buf, &data);
    }

    #[test]
    fn test_no_session_name_shows_default() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        let data = StatusBarData {
            session_name: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            avg_latency_ms: None,
            status: &SessionStatus::Active,
        };
        render_status_bar(area, &mut buf, &data);
        let content = buf_content(&buf, area);
        assert!(content.contains("crown-code"));
    }
}
