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
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let y = area.y;

    let name = data.session_name.unwrap_or("crown-code");
    let (status_label, status_color) = match data.status {
        SessionStatus::Active => ("●", Color::Green),
        SessionStatus::Completed => ("●", Color::Blue),
        SessionStatus::Error => ("●", Color::Red),
    };
    let tokens = format!(
        "In:{} Out:{} Cache:{}",
        data.input_tokens, data.output_tokens, data.cache_read_tokens
    );
    let latency = data
        .avg_latency_ms
        .map(|ms| format!("avg:{ms}ms"))
        .unwrap_or_default();

    let sep = " │ ";

    // Build segments from highest priority (name) to lowest (icon)
    // Drop low-priority segments when width insufficient
    let mut segments: Vec<&str> = vec![name];

    // P2: tokens
    let tokens_w = UnicodeWidthStr::width(tokens.as_str());
    let used_after_name = UnicodeWidthStr::width(name) + sep_w(sep);
    if used_after_name + tokens_w <= width {
        segments.push(&tokens);
    }

    // P3: latency
    let lat_w = UnicodeWidthStr::width(latency.as_str());
    let used_after_tokens = used_after_name
        + if segments.len() > 1 {
            tokens_w + sep_w(sep)
        } else {
            0
        };
    if !latency.is_empty() && used_after_tokens + lat_w <= width {
        segments.push(&latency);
    }

    // P4: status icon (always)
    let used_after_latency = used_after_tokens
        + if segments.len() > 2 {
            lat_w + sep_w(sep)
        } else {
            0
        };
    let icon_w = UnicodeWidthStr::width(status_label);
    if used_after_latency + icon_w <= width {
        segments.push(status_label);
    }

    // Join all segments with separator
    let display = segments.join(sep);
    let truncated = truncate_str(&display, width);
    let truncated_w = UnicodeWidthStr::width(truncated.as_str());

    buf.set_string(area.x, y, &truncated, Style::default());

    // Overwrite the icon with its color if it's in the truncated output
    if segments.last() == Some(&status_label) {
        let icon_str = truncate_str(status_label, width);
        if let Some(pos) = truncated.rfind(icon_str.as_str()) {
            let icon_x = area.x + UnicodeWidthStr::width(&truncated[..pos]) as u16;
            buf.set_string(icon_x, y, &icon_str, Style::default().fg(status_color));
        }
    }

    let mut x = area.x + truncated_w as u16;
    while (x as usize) < area.x as usize + width {
        buf.set_string(x, y, " ", Style::default());
        x += 1;
    }
}

fn sep_w(sep: &str) -> usize {
    UnicodeWidthStr::width(sep)
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
        assert!(content.contains("Cache:890"));
        assert!(content.contains("avg:230ms"));
        assert!(content.contains("●"));
    }

    #[test]
    fn test_segments_flow_left_to_right() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        let data = StatusBarData {
            session_name: Some("proj"),
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 30,
            avg_latency_ms: Some(50),
            status: &SessionStatus::Active,
        };
        render_status_bar(area, &mut buf, &data);
        let content = buf_content(&buf, area);
        // All segments should be contiguous, no gap between name and rest
        let idx_proj = content.find("proj").unwrap();
        let idx_in = content.find("In:10").unwrap();
        let idx_avg = content.find("avg:50ms").unwrap();
        let idx_icon = content.find("●").unwrap();
        assert!(idx_proj < idx_in);
        assert!(idx_in < idx_avg);
        assert!(idx_avg < idx_icon);
    }

    #[test]
    fn test_narrow_drops_tokens_first() {
        // "my project" = 10, sep=3, latency=9, icon=1 → name+latency+icon = 23
        // tokens = 25 → total would be 51 > 30, tokens dropped
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
        assert!(!content.contains("In:1234"));
        assert!(content.contains("avg:230ms"));
        assert!(content.contains("●"));
    }

    #[test]
    fn test_narrower_drops_latency() {
        // "my project" = 10, sep=3, icon=1 → 14. latency=9 → 23 > 20
        let area = Rect::new(0, 0, 20, 1);
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
        assert!(!content.contains("In:1234"));
        assert!(!content.contains("avg:230ms"));
        assert!(content.contains("●"));
    }

    #[test]
    fn test_very_narrow_truncates() {
        let area = Rect::new(0, 0, 5, 1);
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

    #[test]
    fn test_pipe_separator_style() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        let data = StatusBarData {
            session_name: Some("proj"),
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 30,
            avg_latency_ms: Some(50),
            status: &SessionStatus::Active,
        };
        render_status_bar(area, &mut buf, &data);
        let content = buf_content(&buf, area);
        assert!(content.contains("proj"));
        assert!(content.contains(" │ "));
    }

    #[test]
    fn test_status_icon_colors() {
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
            let icon_cell = (0..80)
                .find(|&x| buf[(x, 0)].symbol() == "●")
                .expect("status icon should be present");
            assert_eq!(buf[(icon_cell, 0)].style().fg, Some(expected_color));
        }
    }
}
