#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
    pub id: String,
}

pub struct SseParser {
    event_type: String,
    data_lines: Vec<String>,
    event_id: String,
    last_event_id: String,
    retry_value: i32,
    buf: String,
    ignore_bom: bool,
}

impl SseParser {
    pub fn new() -> Self {
        SseParser {
            event_type: String::new(),
            data_lines: Vec::new(),
            event_id: String::new(),
            last_event_id: String::new(),
            retry_value: 0,
            buf: String::new(),
            ignore_bom: true,
        }
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        let mut events = Vec::new();
        if chunk.is_empty() {
            return events;
        }
        let mut s = chunk.to_string();
        if self.ignore_bom {
            if s.len() >= 3
                && s.as_bytes()[0] == 0xEF
                && s.as_bytes()[1] == 0xBB
                && s.as_bytes()[2] == 0xBF
            {
                s = s[3..].to_string();
            }
            self.ignore_bom = false;
        }
        if s.is_empty() {
            return events;
        }
        self.buf.push_str(&s);
        self.buf = self.buf.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(last_nl) = self.buf.rfind('\n') {
            let complete = self.buf[..last_nl].to_string();
            self.buf = if last_nl + 1 < self.buf.len() {
                self.buf[last_nl + 1..].to_string()
            } else {
                String::new()
            };
            for line in complete.split('\n') {
                if line.is_empty() {
                    if !self.data_lines.is_empty() {
                        events.push(self.dispatch_event());
                    }
                } else {
                    self.process_line(line);
                }
            }
        }
        events
    }

    pub fn flush(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        if !self.buf.is_empty() {
            let s = std::mem::take(&mut self.buf);
            for line in s.split('\n') {
                if line.is_empty() {
                    if !self.data_lines.is_empty() {
                        events.push(self.dispatch_event());
                    }
                } else {
                    self.process_line(line);
                }
            }
        }
        if !self.data_lines.is_empty() {
            events.push(self.dispatch_event());
        }
        events
    }

    pub fn reset(&mut self) {
        self.event_type.clear();
        self.data_lines.clear();
        self.event_id.clear();
        self.last_event_id.clear();
        self.retry_value = 0;
        self.buf.clear();
        self.ignore_bom = true;
    }

    pub fn last_event_id(&self) -> &str {
        &self.last_event_id
    }

    pub fn reconnection_time(&self) -> i32 {
        self.retry_value
    }

    fn process_line(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = if let Some(pos) = line.find(':') {
            let f = &line[..pos];
            let v = if pos + 1 < line.len() {
                let rest = &line[pos + 1..];
                if let Some(stripped) = rest.strip_prefix(' ') {
                    stripped
                } else {
                    rest
                }
            } else {
                ""
            };
            (f, v)
        } else {
            (line, "")
        };
        match field.to_lowercase().as_str() {
            "event" => self.event_type = value.to_string(),
            "data" => self.data_lines.push(value.to_string()),
            "id" => {
                if !value.contains('\0') {
                    self.event_id = value.to_string();
                    self.last_event_id = value.to_string();
                }
            }
            "retry" => {
                let trimmed = value.trim();
                if !trimmed.is_empty()
                    && let Ok(n) = trimmed.parse::<i32>()
                    && n >= 0
                {
                    self.retry_value = n;
                }
            }
            _ => {}
        }
    }

    fn dispatch_event(&mut self) -> SseEvent {
        let event = SseEvent {
            event: if self.event_type.is_empty() {
                "message".to_string()
            } else {
                std::mem::take(&mut self.event_type)
            },
            data: self.data_lines.join("\n"),
            id: std::mem::take(&mut self.event_id),
        };
        self.data_lines.clear();
        event
    }
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_event_with_data() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello world\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "message");
        assert_eq!(events[0].data, "hello world");
        assert_eq!(events[0].id, "");
    }

    #[test]
    fn test_event_with_type_and_id() {
        let mut parser = SseParser::new();
        let events = parser.feed("event: update\ndata: some data\nid: 123\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "update");
        assert_eq!(events[0].data, "some data");
        assert_eq!(events[0].id, "123");
    }

    #[test]
    fn test_multiple_events() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: first\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn test_multi_line_data() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: line1\ndata: line2\ndata: line3\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2\nline3");
    }

    #[test]
    fn test_comment_lines_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed(": this is a comment\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_bom_stripped() {
        let mut parser = SseParser::new();
        let bom = "\u{FEFF}";
        let events = parser.feed(&format!("{}data: hello\n\n", bom));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_no_data_no_event() {
        let mut parser = SseParser::new();
        let events = parser.feed("event: noop\n\n");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_null_id_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed("id: nul\0l\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "");
        assert_eq!(parser.last_event_id(), "");
    }

    #[test]
    fn test_invalid_retry_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed("retry: not-a-number\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(parser.reconnection_time(), 0);
    }

    #[test]
    fn test_valid_retry() {
        let mut parser = SseParser::new();
        let events = parser.feed("retry: 5000\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(parser.reconnection_time(), 5000);
    }

    #[test]
    fn test_unknown_field_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed("x-unknown: foobar\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_crlf_line_endings() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello\r\n\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_cr_line_endings() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello\r\r");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_retry_with_trailing_space() {
        let mut parser = SseParser::new();
        let events = parser.feed("retry: 3000 \ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(parser.reconnection_time(), 3000);
    }

    #[test]
    fn test_colon_in_event_name() {
        let mut parser = SseParser::new();
        let events = parser.feed("event: custom:event\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "custom:event");
    }

    #[test]
    fn test_leading_space_in_data() {
        let mut parser = SseParser::new();
        let events = parser.feed("data:  hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, " hello");
    }

    #[test]
    fn test_streaming_across_chunks() {
        let mut parser = SseParser::new();
        let events1 = parser.feed("data: hel");
        assert_eq!(events1.len(), 0);
        let events2 = parser.feed("lo\n\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "hello");
    }

    #[test]
    fn test_streaming_mid_line_split() {
        let mut parser = SseParser::new();
        let events1 = parser.feed("data: hello\n");
        assert_eq!(events1.len(), 0);
        let events2 = parser.feed("\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "hello");
    }

    #[test]
    fn test_flush_residual_data() {
        let mut parser = SseParser::new();
        parser.feed("data: hello\n");
        let events = parser.flush();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_reset_clears_state() {
        let mut parser = SseParser::new();
        parser.feed("data: hello\n\nevent: update\n");
        parser.reset();
        assert_eq!(parser.last_event_id(), "");
        assert_eq!(parser.reconnection_time(), 0);
        let events = parser.feed("data: after reset\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "after reset");
    }

    #[test]
    fn test_last_event_id_persists() {
        let mut parser = SseParser::new();
        parser.feed("id: abc123\ndata: first\n\n");
        assert_eq!(parser.last_event_id(), "abc123");
        parser.feed("data: second\n\n");
        assert_eq!(parser.last_event_id(), "abc123");
    }

    #[test]
    fn test_negative_retry_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed("retry: -1\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(parser.reconnection_time(), 0);
    }

    #[test]
    fn test_flush_no_pending() {
        let mut parser = SseParser::new();
        let events = parser.flush();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_data_field_with_trailing_whitespace() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: value with trailing space   \n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "value with trailing space   ");
    }

    #[test]
    fn test_event_field_value_empty_string() {
        let mut parser = SseParser::new();
        let events = parser.feed("event:\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "message");
    }

    #[test]
    fn test_empty_data_line_in_multiline_data() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: line1\ndata: \ndata: line3\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\n\nline3");
    }

    #[test]
    fn test_empty_chunk_returns_no_events() {
        let mut parser = SseParser::new();
        let events = parser.feed("");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_multi_chunk_streaming_with_leftover_and_flush() {
        let mut parser = SseParser::new();
        let e1 = parser.feed("data: a\n\n");
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].data, "a");

        let e2 = parser.feed("data: b\n\n");
        assert_eq!(e2.len(), 1);
        assert_eq!(e2[0].data, "b");

        let e3 = parser.feed("data: c\n");
        assert_eq!(e3.len(), 0);

        let e4 = parser.flush();
        assert_eq!(e4.len(), 1);
        assert_eq!(e4[0].data, "c");
    }

    #[test]
    fn test_flush_without_data_lines_no_event() {
        let mut parser = SseParser::new();
        let events = parser.feed("event: ping\n");
        assert_eq!(events.len(), 0);
        let flushed = parser.flush();
        assert_eq!(flushed.len(), 0);
    }
}
