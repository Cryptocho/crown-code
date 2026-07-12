use regex::Regex;

#[derive(Debug, Clone)]
pub struct Match {
    pub line_number: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub line: String,
    pub path: String,
}

#[derive(Clone)]
pub struct Search {
    regex: Regex,
}

fn find_line_range(text: &str, offset: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let mut start = offset;
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
        end += 1;
    }
    (start, end)
}

pub fn calc_line_number(text: &str, offset: usize) -> usize {
    if text.is_empty() {
        return 1;
    }
    let bytes = text.as_bytes();
    let mut line = 1;
    let end = offset.min(text.len());
    for &b in bytes.iter().take(end) {
        if b == b'\n' {
            line += 1;
        }
    }
    line
}

pub fn get_line(text: &str, line_number: usize) -> Option<String> {
    if text.is_empty() || line_number < 1 {
        return None;
    }
    let bytes = text.as_bytes();
    let mut current_line = 1;
    let mut line_start = 0;
    for i in 0..bytes.len() {
        if current_line == line_number {
            let mut line_end = i;
            while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
                line_end += 1;
            }
            return Some(text[line_start..line_end].to_string());
        }
        if bytes[i] == b'\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }
    if current_line == line_number && line_start < text.len() {
        return Some(text[line_start..].to_string());
    }
    None
}

pub fn new_search(
    pattern: &str,
    case_insensitive: bool,
    multi_line: bool,
    dot_all: bool,
) -> Result<Search, regex::Error> {
    let mut opts = regex::RegexBuilder::new(pattern);
    if case_insensitive {
        opts.case_insensitive(true);
    }
    if multi_line {
        opts.multi_line(true);
    }
    if dot_all {
        opts.dot_matches_new_line(true);
    }
    let regex = opts.build()?;
    Ok(Search { regex })
}

impl Search {
    pub fn match_first(&self, text: &str, offset: usize) -> Option<Match> {
        if text.is_empty() || offset > text.len() {
            return None;
        }
        let m = self.regex.find_at(text, offset)?;
        let (line_start, line_end) = find_line_range(text, m.start());
        Some(Match {
            line_number: calc_line_number(text, m.start()),
            column_start: m.start(),
            column_end: m.end(),
            line: text[line_start..line_end].to_string(),
            path: String::new(),
        })
    }

    pub fn match_all(&self, text: &str, offset: usize) -> Vec<Match> {
        if text.is_empty() || offset >= text.len() {
            return Vec::new();
        }
        let mut results = Vec::new();
        let mut pos = offset;
        while pos <= text.len() {
            if let Some(m) = self.regex.find_at(text, pos) {
                let (line_start, line_end) = find_line_range(text, m.start());
                results.push(Match {
                    line_number: calc_line_number(text, m.start()),
                    column_start: m.start(),
                    column_end: m.end(),
                    line: text[line_start..line_end].to_string(),
                    path: String::new(),
                });
                pos = m.end().max(pos + 1);
                if pos >= text.len() {
                    break;
                }
            } else {
                break;
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_search_invalid_regex() {
        assert!(new_search("[invalid", false, false, false).is_err());
    }

    #[test]
    fn test_match_first_basic() {
        let s = new_search("foo", false, false, false).unwrap();
        let m = s.match_first("foo bar", 0).unwrap();
        assert_eq!(m.line_number, 1);
        assert_eq!(m.column_start, 0);
        assert_eq!(m.column_end, 3);
    }

    #[test]
    fn test_match_first_no_match() {
        let s = new_search("xyz", false, false, false).unwrap();
        assert!(s.match_first("foo bar", 0).is_none());
    }

    #[test]
    fn test_match_first_offset() {
        let s = new_search("foo", false, false, false).unwrap();
        assert!(s.match_first("foo foo", 4).is_some());
    }

    #[test]
    fn test_match_all() {
        let s = new_search("a", false, false, false).unwrap();
        let matches = s.match_all("a b a", 0);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_match_all_no_matches() {
        let s = new_search("xyz", false, false, false).unwrap();
        let matches = s.match_all("foo bar", 0);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_calc_line_number() {
        assert_eq!(calc_line_number("foo\nbar\nbaz", 4), 2);
    }

    #[test]
    fn test_calc_line_number_empty() {
        assert_eq!(calc_line_number("", 0), 1);
    }

    #[test]
    fn test_get_line() {
        let text = "foo\nbar\nbaz";
        assert_eq!(get_line(text, 2).unwrap(), "bar");
    }

    #[test]
    fn test_get_line_invalid() {
        assert!(get_line("foo", 5).is_none());
    }

    #[test]
    fn test_case_insensitive() {
        let s = new_search("FOO", true, false, false).unwrap();
        assert!(s.match_first("foo", 0).is_some());
    }

    #[test]
    fn test_multi_line() {
        let s = new_search("^bar", false, true, false).unwrap();
        assert!(s.match_first("foo\nbar", 0).is_some());
    }

    #[test]
    fn test_dot_all() {
        let s = new_search("foo.bar", false, false, true).unwrap();
        assert!(s.match_first("foo\nbar", 0).is_some());
    }

    #[test]
    fn test_match_at_start() {
        let s = new_search("hello", false, false, false).unwrap();
        let m = s.match_first("hello world", 0).unwrap();
        assert_eq!(m.column_start, 0);
        assert_eq!(m.column_end, 5);
    }

    #[test]
    fn test_skip_offset_past_end() {
        let s = new_search("test", false, false, false).unwrap();
        assert!(s.match_first("test", 10).is_none());
    }

    #[test]
    fn test_match_all_single() {
        let s = new_search("test", false, false, false).unwrap();
        let matches = s.match_all("this is a test", 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].column_start, 10);
    }

    #[test]
    fn test_calc_line_number_second_line() {
        let text = "line1\nline2\nline3";
        assert_eq!(calc_line_number(text, 7), 2);
    }

    #[test]
    fn test_calc_line_number_at_newline() {
        let text = "a\nb\nc";
        assert_eq!(calc_line_number(text, 1), 1);
        assert_eq!(calc_line_number(text, 2), 2);
    }

    #[test]
    fn test_get_line_first() {
        let text = "hello\nworld";
        assert_eq!(get_line(text, 1).unwrap(), "hello");
    }

    #[test]
    fn test_get_line_last_no_trailing_newline() {
        let text = "hello\nworld";
        assert_eq!(get_line(text, 2).unwrap(), "world");
    }

    #[test]
    fn test_get_line_zero() {
        assert!(get_line("hello", 0).is_none());
    }

    #[test]
    fn test_get_line_single() {
        assert_eq!(get_line("just one line", 1).unwrap(), "just one line");
    }

    #[test]
    fn test_without_case_insensitive_no_match() {
        let s = new_search("HELLO", false, false, false).unwrap();
        assert!(s.match_first("hello world", 0).is_none());
    }

    #[test]
    fn test_match_across_lines() {
        let s = new_search("line2", false, false, false).unwrap();
        let text = "line1\nline2\nline3";
        let m = s.match_first(text, 0).unwrap();
        assert_eq!(m.line_number, 2);
        assert_eq!(m.line, "line2");
    }

    #[test]
    fn test_empty_text() {
        let s = new_search("foo", false, false, false).unwrap();
        assert!(s.match_first("", 0).is_none());
    }
}
