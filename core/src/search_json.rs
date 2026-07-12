use crate::context::Context;
use crate::search::Match;

pub fn json_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            '\r' => result.push_str("\\r"),
            _ => result.push(c),
        }
    }
    result.push('"');
    result
}

pub fn format_start_json(path: &str) -> String {
    format!("{{\"type\":\"start\",\"path\":{}}}\n", json_escape(path))
}

pub fn format_end_json() -> String {
    "{\"type\":\"end\"}\n".to_string()
}

pub fn format_match_json(match_data: &Match, ctx: Option<&Context>) -> String {
    let mut result = String::from("{\"type\":\"match\",");
    result.push_str("\"path\":");
    result.push_str(&json_escape(&match_data.path));
    result.push_str(",\"line_number\":");
    result.push_str(&match_data.line_number.to_string());
    result.push_str(",\"columns\":{\"start\":");
    result.push_str(&match_data.column_start.to_string());
    result.push_str(",\"end\":");
    result.push_str(&match_data.column_end.to_string());
    result.push_str("},\"line\":");
    result.push_str(&json_escape(&match_data.line));

    if let Some(ctx) = ctx {
        let before = ctx.lines_before();
        let after = ctx.lines_after();
        if !before.is_empty() {
            result.push_str(",\"context_before\":[");
            for (i, line) in before.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                result.push_str(&json_escape(line));
            }
            result.push(']');
        }
        if !after.is_empty() {
            result.push_str(",\"context_after\":[");
            for (i, line) in after.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                result.push_str(&json_escape(line));
            }
            result.push(']');
        }
    }

    result.push_str("}\n");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    #[test]
    fn test_json_escape_double_quote() {
        assert_eq!(json_escape("\""), "\"\\\"\"");
    }

    #[test]
    fn test_json_escape_backslash() {
        assert_eq!(json_escape("\\"), "\"\\\\\"");
    }

    #[test]
    fn test_json_escape_newline() {
        assert_eq!(json_escape("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn test_json_escape_tab() {
        assert_eq!(json_escape("a\tb"), "\"a\\tb\"");
    }

    #[test]
    fn test_json_escape_carriage_return() {
        assert_eq!(json_escape("a\rb"), "\"a\\rb\"");
    }

    #[test]
    fn test_json_escape_empty() {
        assert_eq!(json_escape(""), "\"\"");
    }

    #[test]
    fn test_json_escape_no_special() {
        assert_eq!(json_escape("hello"), "\"hello\"");
    }

    #[test]
    fn test_json_escape_mixed_special_chars() {
        assert_eq!(
            json_escape("say \"hello\"\n\tbye"),
            "\"say \\\"hello\\\"\\n\\tbye\""
        );
    }

    #[test]
    fn test_json_escape_unicode() {
        assert_eq!(json_escape("中文"), "\"中文\"");
    }

    #[test]
    fn test_format_start_json_normal() {
        let result = format_start_json("/path/to/file.txt");
        assert_eq!(
            result,
            "{\"type\":\"start\",\"path\":\"/path/to/file.txt\"}\n"
        );
    }

    #[test]
    fn test_format_start_json_special_chars() {
        let result = format_start_json("path\"with\"quotes");
        assert_eq!(
            result,
            "{\"type\":\"start\",\"path\":\"path\\\"with\\\"quotes\"}\n"
        );
    }

    #[test]
    fn test_format_start_json_empty_path() {
        assert_eq!(
            format_start_json(""),
            "{\"type\":\"start\",\"path\":\"\"}\n"
        );
    }

    #[test]
    fn test_format_end_json() {
        assert_eq!(format_end_json(), "{\"type\":\"end\"}\n");
    }

    #[test]
    fn test_format_match_json_full() {
        let m = Match {
            line_number: 5,
            column_start: 10,
            column_end: 15,
            line: "hello world".to_string(),
            path: "test.txt".to_string(),
        };
        let result = format_match_json(&m, None);
        assert_eq!(
            result,
            "{\"type\":\"match\",\"path\":\"test.txt\",\"line_number\":5,\"columns\":{\"start\":10,\"end\":15},\"line\":\"hello world\"}\n"
        );
    }

    #[test]
    fn test_format_match_json_with_context_after() {
        let m = Match {
            line_number: 1,
            column_start: 0,
            column_end: 1,
            line: "a".to_string(),
            path: "a.txt".to_string(),
        };
        let mut ctx = Context::new(0, 2);
        ctx.add_line("after1");
        ctx.add_line("after2");
        let result = format_match_json(&m, Some(&ctx));
        assert!(!result.contains("context_before"));
        assert!(result.contains("\"context_after\":[\"after1\",\"after2\"]"));
    }

    #[test]
    fn test_format_match_json_with_context_after_special_chars() {
        let m = Match {
            line_number: 5,
            column_start: 5,
            column_end: 8,
            line: "hello world".to_string(),
            path: "/path/to/file.txt".to_string(),
        };
        let mut ctx = Context::new(0, 1);
        ctx.add_line("line with \"quotes\"");
        let result = format_match_json(&m, Some(&ctx));
        assert_eq!(
            result,
            "{\"type\":\"match\",\"path\":\"/path/to/file.txt\",\"line_number\":5,\"columns\":{\"start\":5,\"end\":8},\"line\":\"hello world\",\"context_after\":[\"line with \\\"quotes\\\"\"]}\n"
        );
    }

    #[test]
    fn test_format_match_json_line_with_special_chars() {
        let m = Match {
            line_number: 5,
            column_start: 0,
            column_end: 4,
            line: "a\tb".to_string(),
            path: "/path.txt".to_string(),
        };
        let result = format_match_json(&m, None);
        assert_eq!(
            result,
            "{\"type\":\"match\",\"path\":\"/path.txt\",\"line_number\":5,\"columns\":{\"start\":0,\"end\":4},\"line\":\"a\\tb\"}\n"
        );
    }
}
