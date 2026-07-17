use crate::history_cell::{AssistantMessageCell, HistoryCell};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Widget, Wrap},
};

pub struct StreamingRenderer {
    raw_source: String,
    last_width: u16,
    rendered_lines: Vec<Line<'static>>,
}

impl StreamingRenderer {
    pub fn new() -> Self {
        Self {
            raw_source: String::new(),
            last_width: 0,
            rendered_lines: Vec::new(),
        }
    }

    pub fn append_delta(&mut self, delta: &str) {
        self.raw_source.push_str(delta);
        self.rendered_lines.clear();
    }

    pub fn raw_source(&self) -> &str {
        &self.raw_source
    }

    pub fn rendered_lines(&mut self, width: u16) -> &[Line<'static>] {
        if self.rendered_lines.is_empty() || self.last_width != width {
            self.rerender(width);
        }
        &self.rendered_lines
    }

    fn rerender(&mut self, width: u16) {
        let cell = AssistantMessageCell {
            content: self.raw_source.clone(),
            is_streaming: true,
        };
        self.rendered_lines = cell.display_lines(width);
        self.last_width = width;
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let lines = self.rendered_lines(area.width).to_vec();
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(area, buf);
    }

    pub fn reset(&mut self) {
        self.raw_source.clear();
        self.rendered_lines.clear();
        self.last_width = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_renderer() {
        let mut renderer = StreamingRenderer::new();
        assert_eq!(renderer.raw_source(), "");
        let area = Rect::new(0, 0, 80, 5);
        let mut buf = Buffer::empty(area);
        renderer.render(area, &mut buf);
    }

    #[test]
    fn test_append_delta_accumulates() {
        let mut renderer = StreamingRenderer::new();
        renderer.append_delta("hello ");
        renderer.append_delta("world");
        assert_eq!(renderer.raw_source(), "hello world");
    }

    #[test]
    fn test_rerender_on_width_change() {
        let mut renderer = StreamingRenderer::new();
        renderer.append_delta("a".repeat(100).as_str());
        let lines_w80 = renderer.rendered_lines(80).to_vec();
        let lines_w40 = renderer.rendered_lines(40).to_vec();
        assert_ne!(lines_w80.len(), lines_w40.len());
    }

    #[test]
    fn test_render_to_buffer() {
        let mut renderer = StreamingRenderer::new();
        renderer.append_delta("hello world");
        let area = Rect::new(0, 0, 80, 5);
        let mut buf = Buffer::empty(area);
        renderer.render(area, &mut buf);
        let has_content = (0..80).any(|x| buf[(x, 0)].symbol() != " ");
        assert!(has_content);
    }

    #[test]
    fn test_reset_clears_state() {
        let mut renderer = StreamingRenderer::new();
        renderer.append_delta("hello");
        renderer.reset();
        assert_eq!(renderer.raw_source(), "");
        assert!(renderer.rendered_lines.is_empty());
    }
}
