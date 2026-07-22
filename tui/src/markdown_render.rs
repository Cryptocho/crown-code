use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::ops::Range;
use syntect::easy::HighlightLines;
use syntect::highlighting::Color as SyntectColor;
use syntect::highlighting::FontStyle;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;
const MAX_HIGHLIGHT_LINES: usize = 10_000;

struct MarkdownStyles {
    h1: Style,
    h2: Style,
    h3: Style,
    h4: Style,
    h5: Style,
    h6: Style,
    code: Style,
    emphasis: Style,
    strong: Style,
    strikethrough: Style,
    ordered_list_marker: Style,
    unordered_list_marker: Style,
    link: Style,
    blockquote: Style,
}

impl Default for MarkdownStyles {
    fn default() -> Self {
        Self {
            h1: Style::new().bold().underlined(),
            h2: Style::new().bold(),
            h3: Style::new().bold().italic(),
            h4: Style::new().italic(),
            h5: Style::new().italic(),
            h6: Style::new().italic(),
            code: Style::new().cyan(),
            emphasis: Style::new().italic(),
            strong: Style::new().bold(),
            strikethrough: Style::new().crossed_out(),
            ordered_list_marker: Style::new().light_blue(),
            unordered_list_marker: Style::new(),
            link: Style::new().cyan().underlined(),
            blockquote: Style::new().green(),
        }
    }
}

#[derive(Clone, Debug)]
struct IndentContext {
    prefix: Vec<Span<'static>>,
    marker: Option<Vec<Span<'static>>>,
    is_list: bool,
}

impl IndentContext {
    fn new(prefix: Vec<Span<'static>>, marker: Option<Vec<Span<'static>>>, is_list: bool) -> Self {
        Self {
            prefix,
            marker,
            is_list,
        }
    }
}

struct Writer<'a, I>
where
    I: Iterator<Item = Event<'a>>,
{
    iter: I,
    text: Vec<Line<'static>>,
    styles: MarkdownStyles,
    inline_styles: Vec<Style>,
    indent_stack: Vec<IndentContext>,
    list_indices: Vec<Option<u64>>,
    link: Option<String>,
    needs_newline: bool,
    pending_marker_line: bool,
    in_paragraph: bool,
    in_code_block: bool,
    code_block_lang: Option<String>,
    code_block_buffer: String,
    wrap_width: Option<u16>,
    current_line_content: Option<Line<'static>>,
    current_initial_indent: Vec<Span<'static>>,
    current_subsequent_indent: Vec<Span<'static>>,
    current_line_style: Style,
    current_line_in_code_block: bool,
}

impl<'a, I> Writer<'a, I>
where
    I: Iterator<Item = Event<'a>>,
{
    fn new(iter: I, wrap_width: Option<u16>) -> Self {
        Self {
            iter,
            text: Vec::new(),
            styles: MarkdownStyles::default(),
            inline_styles: Vec::new(),
            indent_stack: Vec::new(),
            list_indices: Vec::new(),
            link: None,
            needs_newline: false,
            pending_marker_line: false,
            in_paragraph: false,
            in_code_block: false,
            code_block_lang: None,
            code_block_buffer: String::new(),
            wrap_width,
            current_line_content: None,
            current_initial_indent: Vec::new(),
            current_subsequent_indent: Vec::new(),
            current_line_style: Style::default(),
            current_line_in_code_block: false,
        }
    }

    fn run(&mut self) {
        while let Some(ev) = self.iter.next() {
            self.handle_event(ev);
        }
        self.flush_current_line();
    }

    fn handle_event(&mut self, event: Event<'a>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(text.as_ref()),
            Event::Code(code) => self.code(code.as_ref()),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => {
                self.flush_current_line();
                if !self.text.is_empty() {
                    self.push_blank_line();
                }
                self.push_line(Line::from("---"));
                self.needs_newline = true;
            }
            _ => {}
        }
    }

    fn start_tag(&mut self, tag: Tag<'a>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading { level, .. } => self.start_heading(level),
            Tag::BlockQuote => self.start_blockquote(),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => Some(lang.to_string()),
                    CodeBlockKind::Indented => None,
                };
                self.start_codeblock(lang);
            }
            Tag::List(start) => self.start_list(start),
            Tag::Item => self.start_item(),
            Tag::Link { dest_url, .. } => {
                self.link = Some(dest_url.to_string());
            }
            Tag::Emphasis => self.push_inline_style(self.styles.emphasis),
            Tag::Strong => self.push_inline_style(self.styles.strong),
            Tag::Strikethrough => self.push_inline_style(self.styles.strikethrough),
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.end_paragraph(),
            TagEnd::Heading(..) => self.end_heading(),
            TagEnd::BlockQuote => self.end_blockquote(),
            TagEnd::CodeBlock => self.end_codeblock(),
            TagEnd::List(_) => self.end_list(),
            TagEnd::Item => {
                self.flush_current_line();
                self.indent_stack.pop();
                self.pending_marker_line = false;
            }
            TagEnd::Link => self.pop_link(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_inline_style(),
            _ => {}
        }
    }

    fn start_paragraph(&mut self) {
        if self.needs_newline {
            self.push_blank_line();
        }
        self.push_line(Line::default());
        self.needs_newline = false;
        self.in_paragraph = true;
    }

    fn end_paragraph(&mut self) {
        self.needs_newline = true;
        self.in_paragraph = false;
        self.pending_marker_line = false;
    }

    fn start_heading(&mut self, level: HeadingLevel) {
        if self.needs_newline {
            self.push_line(Line::default());
            self.needs_newline = false;
        }
        let heading_style = match level {
            HeadingLevel::H1 => self.styles.h1,
            HeadingLevel::H2 => self.styles.h2,
            HeadingLevel::H3 => self.styles.h3,
            HeadingLevel::H4 => self.styles.h4,
            HeadingLevel::H5 => self.styles.h5,
            HeadingLevel::H6 => self.styles.h6,
        };
        let content = format!("{} ", "#".repeat(level as usize));
        self.push_line(Line::from(vec![Span::styled(content, heading_style)]));
        self.push_inline_style(heading_style);
        self.needs_newline = false;
    }

    fn end_heading(&mut self) {
        self.needs_newline = true;
        self.pop_inline_style();
    }

    fn start_blockquote(&mut self) {
        if self.needs_newline {
            self.push_blank_line();
            self.needs_newline = false;
        }
        self.indent_stack
            .push(IndentContext::new(vec![Span::from("> ")], None, false));
    }

    fn end_blockquote(&mut self) {
        self.indent_stack.pop();
        self.needs_newline = true;
    }

    fn start_codeblock(&mut self, lang: Option<String>) {
        self.flush_current_line();
        if !self.text.is_empty() {
            self.push_blank_line();
        }
        self.in_code_block = true;
        self.code_block_lang = lang;
        self.code_block_buffer.clear();
        let indent = vec![Span::from("  ")];
        self.indent_stack
            .push(IndentContext::new(indent, None, false));
    }

    fn end_codeblock(&mut self) {
        if let Some(lang) = self.code_block_lang.take() {
            let code = std::mem::take(&mut self.code_block_buffer);
            if !code.is_empty() {
                let highlighted = highlight_code_to_lines(&code, &lang);
                for hl_line in highlighted {
                    self.push_line(Line::default());
                    for span in hl_line.spans {
                        self.push_span(span);
                    }
                }
            }
        } else {
            let code = std::mem::take(&mut self.code_block_buffer);
            for line in code.lines() {
                self.push_line(Line::default());
                self.push_span(Span::styled(line.to_string(), self.styles.code));
            }
        }

        self.needs_newline = true;
        self.in_code_block = false;
        self.indent_stack.pop();
    }

    fn start_list(&mut self, index: Option<u64>) {
        if self.list_indices.is_empty() && self.needs_newline {
            self.push_line(Line::default());
        }
        self.list_indices.push(index);
    }

    fn end_list(&mut self) {
        self.list_indices.pop();
        self.needs_newline = true;
    }

    fn start_item(&mut self) {
        self.flush_current_line();
        self.pending_marker_line = true;
        let depth = self.list_indices.len();
        let is_ordered = self
            .list_indices
            .last()
            .map(Option::is_some)
            .unwrap_or(false);
        let width = depth * 4 - 3;
        let marker = if let Some(last_index) = self.list_indices.last_mut() {
            match last_index {
                None => Some(vec![Span::styled(
                    " ".repeat(width - 1) + "- ",
                    self.styles.unordered_list_marker,
                )]),
                Some(index) => {
                    *index += 1;
                    Some(vec![Span::styled(
                        format!("{:width$}. ", *index - 1),
                        self.styles.ordered_list_marker,
                    )])
                }
            }
        } else {
            None
        };
        let indent_prefix = if depth == 0 {
            Vec::new()
        } else {
            let indent_len = if is_ordered { width + 2 } else { width + 1 };
            vec![Span::from(" ".repeat(indent_len))]
        };
        self.indent_stack
            .push(IndentContext::new(indent_prefix, marker, true));
        self.needs_newline = false;
    }

    fn text(&mut self, text: &str) {
        if self.pending_marker_line {
            self.push_line(Line::default());
        }
        self.pending_marker_line = false;

        if self.in_code_block {
            self.code_block_buffer.push_str(text);
            return;
        }

        for (i, line) in text.lines().enumerate() {
            if self.needs_newline {
                self.push_line(Line::default());
                self.needs_newline = false;
            }
            if i > 0 {
                self.push_line(Line::default());
            }
            let content = line.to_string();
            let style = self.inline_styles.last().copied().unwrap_or_default();
            self.push_text_spans(&content, style);
        }
        self.needs_newline = false;
    }

    fn code(&mut self, code: &str) {
        let span = Span::styled(code.to_string(), self.styles.code);
        self.push_span(span);
    }

    fn soft_break(&mut self) {
        self.push_line(Line::default());
    }

    fn hard_break(&mut self) {
        self.push_line(Line::default());
    }

    fn flush_current_line(&mut self) {
        if let Some(line) = self.current_line_content.take() {
            let style = self.current_line_style;
            if !self.current_line_in_code_block
                && let Some(width) = self.wrap_width
            {
                let wrapped = word_wrap_line_with_indent(
                    &line,
                    width as usize,
                    &self.current_initial_indent,
                    &self.current_subsequent_indent,
                );
                for w in wrapped {
                    let owned = Line {
                        style: w.style,
                        alignment: w.alignment,
                        spans: w
                            .spans
                            .into_iter()
                            .map(|s| Span {
                                style: s.style,
                                content: std::borrow::Cow::Owned(s.content.into_owned()),
                            })
                            .collect(),
                    };
                    self.push_output_line(owned.style(style));
                }
            } else {
                let mut spans = self.current_initial_indent.clone();
                let mut line = line;
                spans.append(&mut line.spans);
                line = Line::from(spans);
                self.push_output_line(line.style(style));
            }
            self.current_initial_indent.clear();
            self.current_subsequent_indent.clear();
            self.current_line_in_code_block = false;
        }
    }

    fn push_line(&mut self, line: Line<'static>) {
        self.flush_current_line();
        let blockquote_active = self
            .indent_stack
            .iter()
            .any(|ctx| ctx.prefix.iter().any(|p| p.content.contains('>')));
        let style = if blockquote_active {
            self.styles.blockquote
        } else {
            line.style
        };
        let was_pending = self.pending_marker_line;

        self.current_initial_indent = self.prefix_spans(was_pending);
        self.current_subsequent_indent = self.prefix_spans(false);
        self.current_line_style = style;
        self.current_line_content = Some(line);
        self.current_line_in_code_block = self.in_code_block;

        self.pending_marker_line = false;
    }

    fn push_span(&mut self, span: Span<'static>) {
        if let Some(line) = self.current_line_content.as_mut() {
            line.push_span(span);
        } else {
            self.push_line(Line::from(vec![span]));
        }
    }

    fn push_text_spans(&mut self, text: &str, style: Style) {
        let span = Span::styled(text.to_string(), style);
        self.push_span(span);
    }

    fn push_blank_line(&mut self) {
        self.flush_current_line();
        if self.indent_stack.iter().all(|ctx| ctx.is_list) {
            self.push_output_line(Line::default());
        } else {
            self.push_line(Line::default());
            self.flush_current_line();
        }
    }

    fn push_output_line(&mut self, line: Line<'static>) {
        self.text.push(line);
    }

    fn prefix_spans(&self, pending_marker_line: bool) -> Vec<Span<'static>> {
        let mut prefix: Vec<Span<'static>> = Vec::new();
        let last_marker_index = if pending_marker_line {
            self.indent_stack
                .iter()
                .enumerate()
                .rev()
                .find_map(|(i, ctx)| if ctx.marker.is_some() { Some(i) } else { None })
        } else {
            None
        };
        let last_list_index = self.indent_stack.iter().rposition(|ctx| ctx.is_list);

        for (i, ctx) in self.indent_stack.iter().enumerate() {
            if pending_marker_line {
                if Some(i) == last_marker_index && let Some(marker) = &ctx.marker {
                    prefix.extend(marker.iter().cloned());
                    continue;
                }
                if ctx.is_list && last_marker_index.is_some_and(|idx| idx > i) {
                    continue;
                }
            } else if ctx.is_list && Some(i) != last_list_index {
                continue;
            }
            prefix.extend(ctx.prefix.iter().cloned());
        }
        prefix
    }

    fn push_inline_style(&mut self, style: Style) {
        let current = self.inline_styles.last().copied().unwrap_or_default();
        let merged = current.patch(style);
        self.inline_styles.push(merged);
    }

    fn pop_inline_style(&mut self) {
        self.inline_styles.pop();
    }

    fn pop_link(&mut self) {
        if let Some(dest) = self.link.take() {
            if let Some(inline_style) = self.inline_styles.last().copied() {
                let link_style = inline_style.patch(self.styles.link);
                let span = Span::styled(format!(" ({dest})"), link_style);
                self.push_span(span);
            } else {
                let span = Span::styled(format!(" ({dest})"), self.styles.link);
                self.push_span(span);
            }
        }
    }
}

pub fn render_markdown(source: &str, width: u16) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(source, options);
    let mut w = Writer::new(parser.into_iter(), Some(width));
    w.run();
    if w.text.is_empty() {
        vec![Line::default()]
    } else {
        w.text
    }
}

fn flatten_line(line: &Line<'_>) -> (String, Vec<(Range<usize>, Style)>) {
    let mut flat = String::new();
    let mut span_bounds = Vec::new();
    let mut acc = 0usize;
    for span in &line.spans {
        let text = span.content.as_ref();
        let start = acc;
        flat.push_str(text);
        acc += text.len();
        span_bounds.push((start..acc, span.style));
    }
    (flat, span_bounds)
}

fn slice_line_spans<'a>(
    original: &'a Line<'a>,
    span_bounds: &[(Range<usize>, Style)],
    range: &Range<usize>,
) -> Line<'a> {
    let start_byte = range.start;
    let end_byte = range.end;
    let mut acc: Vec<Span<'a>> = Vec::new();
    for (i, (srange, style)) in span_bounds.iter().enumerate() {
        let s = srange.start;
        let e = srange.end;
        if e <= start_byte {
            continue;
        }
        if s >= end_byte {
            break;
        }
        let seg_start = start_byte.max(s);
        let seg_end = end_byte.min(e);
        if seg_end > seg_start {
            let local_start = seg_start - s;
            let local_end = seg_end - s;
            let content = original.spans[i].content.as_ref();
            let slice = &content[local_start..local_end];
            acc.push(Span {
                style: *style,
                content: std::borrow::Cow::Borrowed(slice),
            });
        }
        if e >= end_byte {
            break;
        }
    }
    Line {
        style: original.style,
        alignment: original.alignment,
        spans: acc,
    }
}

fn span_indent_width(spans: &[Span<'static>]) -> usize {
    use unicode_width::UnicodeWidthStr;
    spans
        .iter()
        .map(|s| s.content.as_ref().width())
        .sum()
}

fn word_wrap_line_with_indent<'a>(
    line: &'a Line<'a>,
    width: usize,
    initial_indent: &[Span<'static>],
    subsequent_indent: &[Span<'static>],
) -> Vec<Line<'a>> {
    if width == 0 {
        return vec![line.clone()];
    }

    let (flat, span_bounds) = flatten_line(line);
    if flat.is_empty() {
        return vec![line.clone()];
    }

    let initial_indent_width = span_indent_width(initial_indent);
    let subsequent_indent_width = span_indent_width(subsequent_indent);

    let initial_available = width.saturating_sub(initial_indent_width).max(1);
    let subsequent_available = width.saturating_sub(subsequent_indent_width).max(1);

    let initial_wrapped = textwrap::wrap(&flat, initial_available);
    let Some(first_line_str) = initial_wrapped.first() else {
        return vec![];
    };

    let mut out: Vec<Line<'a>> = Vec::new();

    let first_end = first_line_str.len();
    let sliced = slice_line_spans(line, &span_bounds, &(0..first_end));
    let mut first_spans: Vec<Span<'a>> = initial_indent
        .iter()
        .map(|s| Span {
            style: s.style,
            content: std::borrow::Cow::Owned(s.content.to_string()),
        })
        .collect();
    first_spans.extend(sliced.spans);
    out.push(Line::from(first_spans));

    let mut offset = first_end;
    if offset < flat.len() && flat.as_bytes()[offset] == b' ' {
        offset += 1;
    }

    if offset < flat.len() {
        let remaining = &flat[offset..];
        let subsequent_wrapped = textwrap::wrap(remaining, subsequent_available);
        let mut sub_offset = 0usize;
        for wrapped_line in &subsequent_wrapped {
            if wrapped_line.is_empty() {
                continue;
            }
            let range_end = sub_offset + wrapped_line.len();
            let global_range = (offset + sub_offset)..(offset + range_end);
            let sliced = slice_line_spans(line, &span_bounds, &global_range);
            let mut line_spans: Vec<Span<'a>> = subsequent_indent
                .iter()
                .map(|s| Span {
                    style: s.style,
                    content: std::borrow::Cow::Owned(s.content.to_string()),
                })
                .collect();
            line_spans.extend(sliced.spans);
            out.push(Line::from(line_spans));
            sub_offset = range_end;
            if sub_offset < remaining.len() && remaining.as_bytes()[sub_offset] == b' ' {
                sub_offset += 1;
            }
        }
    }

    if out.is_empty() {
        out.push(line.clone());
    }
    out
}

const ANSI_ALPHA_INDEX: u8 = 0x00;
const ANSI_ALPHA_DEFAULT: u8 = 0x01;
const OPAQUE_ALPHA: u8 = 0xFF;

fn ansi_palette_color(index: u8) -> Color {
    match index {
        0x00 => Color::Black,
        0x01 => Color::Red,
        0x02 => Color::Green,
        0x03 => Color::Yellow,
        0x04 => Color::Blue,
        0x05 => Color::Magenta,
        0x06 => Color::Cyan,
        0x07 => Color::Gray,
        n => Color::Indexed(n),
    }
}

fn convert_syntect_color(color: SyntectColor) -> Option<Color> {
    match color.a {
        ANSI_ALPHA_INDEX => Some(ansi_palette_color(color.r)),
        ANSI_ALPHA_DEFAULT => None,
        OPAQUE_ALPHA => Some(Color::Rgb(color.r, color.g, color.b)),
        _ => Some(Color::Rgb(color.r, color.g, color.b)),
    }
}

fn convert_style(syn_style: syntect::highlighting::Style) -> Style {
    let mut rt_style = Style::default();

    if let Some(fg) = convert_syntect_color(syn_style.foreground) {
        rt_style = rt_style.fg(fg);
    }

    if syn_style.font_style.contains(FontStyle::BOLD) {
        rt_style.add_modifier |= Modifier::BOLD;
    }

    rt_style
}

fn find_syntax(lang: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    static SYNTAX_SET: std::sync::OnceLock<SyntaxSet> = std::sync::OnceLock::new();
    let ss = SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines);

    let normalized = lang.to_ascii_lowercase();
    let patched = match normalized.as_str() {
        "csharp" | "c-sharp" => "c#",
        "cppm" | "cxxm" | "ixx" => "cpp",
        "golang" => "go",
        "python3" => "python",
        "shell" => "bash",
        _ => lang,
    };

    if let Some(s) = ss.find_syntax_by_token(patched) {
        return Some(s);
    }
    if let Some(s) = ss.find_syntax_by_name(patched) {
        return Some(s);
    }
    let lower = patched.to_ascii_lowercase();
    if let Some(s) = ss
        .syntaxes()
        .iter()
        .find(|s| s.name.to_ascii_lowercase() == lower)
    {
        return Some(s);
    }
    if let Some(s) = ss.find_syntax_by_extension(lang) {
        return Some(s);
    }
    None
}

fn highlight_code_to_lines(code: &str, lang: &str) -> Vec<Line<'static>> {
    static THEME_SET: std::sync::OnceLock<ThemeSet> = std::sync::OnceLock::new();
    let ts = THEME_SET.get_or_init(ThemeSet::load_defaults);

    if code.is_empty() {
        return vec![Line::default()];
    }

    if code.len() > MAX_HIGHLIGHT_BYTES || code.lines().count() > MAX_HIGHLIGHT_LINES {
        return code.lines().map(|l| Line::from(l.to_string())).collect();
    }

    let syntax = match find_syntax(lang) {
        Some(s) => s,
        None => {
            return code.lines().map(|l| Line::from(l.to_string())).collect();
        }
    };

    let theme = &ts.themes["base16-ocean.dark"];
    let mut h = HighlightLines::new(syntax, theme);
    let mut lines: Vec<Line<'static>> = Vec::new();

    for line in LinesWithEndings::from(code) {
        if let Ok(ranges) = h.highlight_line(line, ss_ref()) {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (style, text) in ranges {
                let text = text.trim_end_matches(['\n', '\r']);
                if text.is_empty() {
                    continue;
                }
                spans.push(Span::styled(text.to_string(), convert_style(style)));
            }
            if spans.is_empty() {
                spans.push(Span::raw(String::new()));
            }
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(line.trim_end_matches(['\n', '\r']).to_string()));
        }
    }

    if lines.is_empty() {
        lines.push(Line::default());
    }
    lines
}

fn ss_ref() -> &'static SyntaxSet {
    static SYNTAX_SET: std::sync::OnceLock<SyntaxSet> = std::sync::OnceLock::new();
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_empty() {
        let lines = render_markdown("", 80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_render_plain_text() {
        let lines = render_markdown("hello world", 80);
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "hello world");
    }

    #[test]
    fn test_render_heading_h1() {
        let lines = render_markdown("# Hello", 80);
        assert!(!lines.is_empty());
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("#"));
        assert!(text.contains("Hello"));
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn test_render_heading_h2() {
        let lines = render_markdown("## World", 80);
        assert!(!lines.is_empty());
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("##"));
        assert!(text.contains("World"));
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn test_render_bold() {
        let lines = render_markdown("**bold**", 80);
        assert!(!lines.is_empty());
        let found_bold = lines[0]
            .spans
            .iter()
            .any(|s| s.content.contains("bold") && s.style.add_modifier.contains(Modifier::BOLD));
        assert!(found_bold);
    }

    #[test]
    fn test_render_italic() {
        let lines = render_markdown("*italic*", 80);
        assert!(!lines.is_empty());
        let found_italic = lines[0].spans.iter().any(|s| {
            s.content.contains("italic") && s.style.add_modifier.contains(Modifier::ITALIC)
        });
        assert!(found_italic);
    }

    #[test]
    fn test_render_inline_code() {
        let lines = render_markdown("`code`", 80);
        assert!(!lines.is_empty());
        let found_code = lines[0]
            .spans
            .iter()
            .any(|s| s.content.contains("code") && s.style.fg == Some(Color::Cyan));
        assert!(found_code);
    }

    #[test]
    fn test_render_code_block_no_lang() {
        let source = "```\nlet x = 1;\n```";
        let lines = render_markdown(source, 80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("let x = 1;"));
    }

    #[test]
    fn test_render_code_block_with_lang() {
        let source = "```rust\nlet x = 1;\n```";
        let lines = render_markdown(source, 80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("let x = 1;"));
    }

    #[test]
    fn test_render_unordered_list() {
        let source = "- item 1\n- item 2\n- item 3";
        let lines = render_markdown(source, 80);
        assert!(lines.len() >= 3);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("item 1"));
        assert!(all_text.contains("item 2"));
        assert!(all_text.contains("item 3"));
    }

    #[test]
    fn test_render_ordered_list() {
        let source = "1. first\n2. second\n3. third";
        let lines = render_markdown(source, 80);
        assert!(lines.len() >= 3);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("first"));
        assert!(all_text.contains("second"));
        assert!(all_text.contains("third"));
    }

    #[test]
    fn test_render_blockquote() {
        let source = "> quote text";
        let lines = render_markdown(source, 80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains(">"));
        assert!(all_text.contains("quote text"));
    }

    #[test]
    fn test_render_link() {
        let source = "[text](http://example.com)";
        let lines = render_markdown(source, 80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("text"));
        assert!(all_text.contains("http://example.com"));
    }

    #[test]
    fn test_render_rule() {
        let source = "---";
        let lines = render_markdown(source, 80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("---"));
    }

    #[test]
    fn test_render_strikethrough() {
        let lines = render_markdown("~~deleted~~", 80);
        assert!(!lines.is_empty());
        let found_strike = lines[0].spans.iter().any(|s| {
            s.content.contains("deleted") && s.style.add_modifier.contains(Modifier::CROSSED_OUT)
        });
        assert!(found_strike);
    }

    #[test]
    fn test_render_empty_input() {
        let lines = render_markdown("", 80);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans.is_empty());
    }

    #[test]
    fn test_word_wrap_line() {
        let line = Line::from("hello world this is a test");
        let wrapped = word_wrap_line_with_indent(&line, 10, &[], &[]);
        assert!(wrapped.len() > 1);
    }

    #[test]
    fn test_word_wrap_preserves_styles() {
        let line = Line::from(vec![
            Span::styled("hello ", Style::default().fg(Color::Red)),
            Span::styled("world this is a test", Style::default().fg(Color::Blue)),
        ]);
        let wrapped = word_wrap_line_with_indent(&line, 10, &[], &[]);
        assert!(wrapped.len() > 1);
        let first_line_text: String = wrapped[0]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(first_line_text.starts_with("hello"));
    }

    #[test]
    fn test_flatten_and_slice() {
        let line = Line::from(vec![
            Span::styled("hello ", Style::default().fg(Color::Red)),
            Span::styled("world", Style::default().fg(Color::Blue)),
        ]);
        let (flat, bounds) = flatten_line(&line);
        assert_eq!(flat, "hello world");
        assert_eq!(bounds.len(), 2);
        assert_eq!(bounds[0].0, 0..6);
        assert_eq!(bounds[1].0, 6..11);

        let sliced = slice_line_spans(&line, &bounds, &(0..5));
        assert_eq!(sliced.spans.len(), 1);
        assert_eq!(sliced.spans[0].content, "hello");
    }

    #[test]
    fn test_nested_styles() {
        let source = "**bold *and italic***";
        let lines = render_markdown(source, 80);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_mixed_content() {
        let source =
            "# Title\n\nSome **bold** and *italic* text.\n\n```\ncode block\n```\n\n- list item";
        let lines = render_markdown(source, 80);
        assert!(lines.len() > 5);
    }

    #[test]
    fn test_long_line_wraps() {
        let source = "This is a very long line that should definitely wrap when the width is limited to something reasonable like 40 columns.";
        let lines = render_markdown(source, 40);
        assert!(lines.len() > 1);
    }

    #[test]
    fn test_cjk_text() {
        let source = "这是一段中文文本，用于测试CJK字符在Markdown渲染中的宽度处理。";
        let lines = render_markdown(source, 30);
        assert!(lines.len() >= 1);
    }

    #[test]
    fn test_streaming_cursor() {
        let mut source = "hello".to_string();
        source.push('\u{258C}');
        let lines = render_markdown(&source, 80);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains('\u{258C}'));
    }

    #[test]
    fn test_list_items_no_indent_accumulation() {
        let source = "- item 1\n- item 2\n- item 3";
        let lines = render_markdown(source, 80);
        assert!(lines.len() >= 3);
        let first: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let second: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let first_marker = first.find("- ").unwrap_or(0);
        let second_marker = second.find("- ").unwrap_or(0);
        assert_eq!(first_marker, second_marker, "list markers should align");
    }

    #[test]
    fn test_ordered_list_no_indent_accumulation() {
        let source = "1. first\n2. second\n3. third";
        let lines = render_markdown(source, 80);
        assert!(lines.len() >= 3);
        let first: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let second: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let first_marker = first.find(". ").unwrap_or(0);
        let second_marker = second.find(". ").unwrap_or(0);
        assert_eq!(first_marker, second_marker, "ordered list markers should align");
    }

    #[test]
    fn test_heading_after_list_no_indent() {
        let source = "- item 1\n- item 2\n\n## Heading";
        let lines = render_markdown(source, 80);
        let heading_line = lines.iter().find(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains("Heading"))
        });
        assert!(heading_line.is_some(), "heading should be present");
        let heading: String = heading_line
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            !heading.starts_with("    "),
            "heading should not have list indent, got: {:?}",
            heading
        );
    }

    #[test]
    fn test_continuation_indent() {
        let source = "- This is a long list item text that should wrap to multiple lines at width 30";
        let lines = render_markdown(source, 30);
        assert!(
            lines.len() > 1,
            "long list item should wrap, got {} lines",
            lines.len()
        );
        let second: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            second.starts_with("  "),
            "continuation line should be indented, got: {:?}",
            second
        );
    }

    #[test]
    fn test_word_wrap_with_indent() {
        let line = Line::from("hello world foo bar baz qux");
        let initial = vec![Span::from("  - ")];
        let subsequent = vec![Span::from("    ")];
        let wrapped = word_wrap_line_with_indent(&line, 15, &initial, &subsequent);
        assert!(wrapped.len() > 1);
        let first: String = wrapped[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first.starts_with("  - "), "first line should start with marker: {:?}", first);
        let second: String = wrapped[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(second.starts_with("    "), "continuation should have indent: {:?}", second);
    }
}
