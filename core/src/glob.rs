fn match_class(pattern: &[u8], pi: &mut usize, ch: u8) -> bool {
    if *pi >= pattern.len() {
        return false;
    }
    *pi += 1;
    let mut negate = false;
    if *pi < pattern.len() && pattern[*pi] == b'!' {
        negate = true;
        *pi += 1;
    }

    let mut matched = false;

    if *pi < pattern.len() && pattern[*pi] == b']' {
        if ch == b']' {
            matched = true;
        }
        *pi += 1;
    }

    while *pi < pattern.len() && pattern[*pi] != b']' {
        if *pi + 2 < pattern.len() && pattern[*pi + 1] == b'-' && pattern[*pi + 2] != b']' {
            let range_start = pattern[*pi];
            let range_end = pattern[*pi + 2];
            if ch >= range_start && ch <= range_end {
                matched = true;
            }
            *pi += 3;
        } else {
            if ch == pattern[*pi] {
                matched = true;
            }
            *pi += 1;
        }
    }

    if *pi < pattern.len() {
        *pi += 1;
    }

    if negate { !matched } else { matched }
}

fn fnmatch(pattern: &str, s: &str) -> bool {
    let pat = pattern.as_bytes();
    let st = s.as_bytes();
    let mut pi = 0;
    let mut si = 0;
    let mut star_pi: isize = -1;
    let mut match_si = 0;

    while si < st.len() {
        if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == st[si]) {
            pi += 1;
            si += 1;
        } else if pi < pat.len() && pat[pi] == b'[' {
            if match_class(pat, &mut pi, st[si]) {
                si += 1;
            } else if star_pi >= 0 {
                pi = (star_pi + 1) as usize;
                match_si += 1;
                si = match_si;
            } else {
                return false;
            }
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_pi = pi as isize;
            match_si = si;
            pi += 1;
        } else if star_pi >= 0 {
            pi = (star_pi + 1) as usize;
            match_si += 1;
            si = match_si;
        } else {
            return false;
        }
    }

    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }

    pi == pat.len()
}

pub(crate) fn fnmatch_pathname(pattern: &str, s: &str) -> bool {
    let pat = pattern.as_bytes();
    let st = s.as_bytes();
    let mut pi = 0;
    let mut si = 0;
    let mut star_pi: isize = -1;
    let mut match_si = 0;

    while si < st.len() {
        if pi < pat.len() && ((pat[pi] == b'?' && st[si] != b'/') || pat[pi] == st[si]) {
            pi += 1;
            si += 1;
        } else if pi < pat.len() && pat[pi] == b'[' {
            if st[si] != b'/' && match_class(pat, &mut pi, st[si]) {
                si += 1;
            } else if star_pi >= 0 {
                pi = (star_pi + 1) as usize;
                match_si += 1;
                si = match_si;
            } else {
                return false;
            }
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_pi = pi as isize;
            match_si = si;
            pi += 1;
        } else if star_pi >= 0 {
            pi = (star_pi + 1) as usize;
            match_si += 1;
            if match_si <= si || (match_si > 0 && st[match_si - 1] == b'/') {
                return false;
            }
            si = match_si;
        } else {
            return false;
        }
    }

    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }

    pi == pat.len()
}

pub fn match_glob(filename: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if pattern.as_bytes()[0] == b'!' {
        return !fnmatch(&pattern[1..], filename);
    }
    fnmatch(pattern, filename)
}

pub fn match_glob_pathname(filename: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if pattern.as_bytes()[0] == b'!' {
        return !fnmatch_pathname(&pattern[1..], filename);
    }
    fnmatch_pathname(pattern, filename)
}

pub fn match_any_glob(filename: &str, patterns: &[String]) -> bool {
    if filename.is_empty() || patterns.is_empty() {
        return false;
    }

    let mut matched = false;
    for pattern in patterns {
        if pattern.is_empty() {
            continue;
        }
        if pattern.as_bytes()[0] == b'!' {
            if fnmatch(&pattern[1..], filename) {
                return false;
            }
        } else {
            if fnmatch(pattern, filename) {
                matched = true;
            }
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(match_glob("hello", "hello"));
        assert!(!match_glob("hello", "world"));
    }

    #[test]
    fn test_wildcard_match() {
        assert!(match_glob("foo.txt", "*.txt"));
    }

    #[test]
    fn test_wildcard_no_match() {
        assert!(!match_glob("foo.txt", "*.rs"));
    }

    #[test]
    fn test_star_matches_everything() {
        assert!(match_glob("anything", "*"));
        assert!(match_glob("", "*"));
        assert!(match_glob("file.txt", "*.txt"));
        assert!(match_glob("file.txt", "f*"));
        assert!(match_glob("file.txt", "*e*"));
        assert!(!match_glob("file.txt", "*.md"));
    }

    #[test]
    fn test_question_mark() {
        assert!(match_glob("foo", "fo?"));
        assert!(match_glob("ab", "a?"));
        assert!(match_glob("ab", "?b"));
        assert!(!match_glob("ab", "?"));
        assert!(!match_glob("", "?"));
    }

    #[test]
    fn test_star_and_question_mark_combined() {
        assert!(match_glob("file.txt", "f?le.*"));
        assert!(match_glob("file.txt", "f?*e.*"));
        assert!(!match_glob("file.txt", "?z*"));
    }

    #[test]
    fn test_char_class() {
        assert!(match_glob("foo", "[fgh]oo"));
        assert!(match_glob("a", "[abc]"));
        assert!(match_glob("b", "[abc]"));
        assert!(!match_glob("d", "[abc]"));
    }

    #[test]
    fn test_range_class() {
        assert!(match_glob("a", "[a-z]"));
        assert!(match_glob("m", "[a-z]"));
        assert!(match_glob("z", "[a-z]"));
        assert!(match_glob("5", "[0-9]"));
        assert!(!match_glob("1", "[a-z]"));
        assert!(!match_glob("A", "[a-z]"));
    }

    #[test]
    fn test_negated_class() {
        assert!(match_glob("d", "[!abc]"));
        assert!(!match_glob("a", "[!abc]"));
        assert!(match_glob("A", "[!a-z]"));
        assert!(!match_glob("m", "[!a-z]"));
    }

    #[test]
    fn test_class_with_literal_bracket() {
        assert!(match_glob("]", "[]]"));
        assert!(match_glob("a]", "[a]]"));
        assert!(!match_glob("b", "[a]]"));
    }

    #[test]
    fn test_class_with_literal_dash() {
        assert!(match_glob("-", "[-a]"));
        assert!(match_glob("a", "[-a]"));
        assert!(!match_glob("b", "[-a]"));
        assert!(match_glob("-", "[a-]"));
        assert!(match_glob("a", "[a-]"));
        assert!(!match_glob("b", "[a-]"));
    }

    #[test]
    fn test_negation() {
        assert!(!match_glob("foo.txt", "!*.txt"));
        assert!(match_glob("foo.txt", "!*.rs"));
        assert!(match_glob("anything", "!"));
    }

    #[test]
    fn test_negate_with_wildcards() {
        assert!(match_glob("file.txt", "!*.md"));
        assert!(!match_glob("file.txt", "!*.txt"));
    }

    #[test]
    fn test_empty_pattern_returns_false() {
        assert!(!match_glob("anything", ""));
        assert!(!match_glob("", ""));
    }

    #[test]
    fn test_empty_string_matching() {
        assert!(match_glob("", "*"));
        assert!(!match_glob("", "!"));
        assert!(!match_glob("", "?"));
        assert!(match_glob("a", "!"));
    }

    #[test]
    fn test_multiple_patterns() {
        let patterns = vec!["*.txt".to_string(), "*.rs".to_string()];
        assert!(match_any_glob("foo.txt", &patterns));
        assert!(match_any_glob("main.rs", &patterns));
        assert!(!match_any_glob("foo.md", &patterns));
    }

    #[test]
    fn test_negation_in_multiple() {
        let patterns = vec!["*.txt".to_string(), "!secret.txt".to_string()];
        assert!(match_any_glob("readme.txt", &patterns));
        assert!(!match_any_glob("secret.txt", &patterns));
    }

    #[test]
    fn test_negation_short_circuit() {
        let patterns = vec!["*".to_string(), "!hello".to_string()];
        assert!(!match_any_glob("hello", &patterns));
        let patterns2 = vec!["*.txt".to_string(), "!t*t.txt".to_string()];
        assert!(!match_any_glob("test.txt", &patterns2));
        let patterns3 = vec!["!t*t.txt".to_string(), "*.txt".to_string()];
        assert!(!match_any_glob("test.txt", &patterns3));
    }

    #[test]
    fn test_negation_does_not_apply_to_unmatched() {
        let patterns = vec!["*".to_string(), "!world".to_string()];
        assert!(match_any_glob("hello", &patterns));
    }

    #[test]
    fn test_only_negation_patterns() {
        assert!(!match_any_glob("hello", &["!hello".to_string()]));
        assert!(!match_any_glob("hello", &["!*".to_string()]));
    }

    #[test]
    fn test_empty_patterns_array() {
        let empty: Vec<String> = vec![];
        assert!(!match_any_glob("hello", &empty));
        assert!(!match_any_glob("", &empty));
    }

    #[test]
    fn test_empty_string_filename() {
        assert!(!match_any_glob("", &["*".to_string()]));
        assert!(!match_any_glob("", &["!*".to_string()]));
    }

    #[test]
    fn test_empty_individual_patterns_skipped() {
        assert!(match_any_glob(
            "hello",
            &["".to_string(), "hello".to_string(), "".to_string()]
        ));
        assert!(!match_any_glob(
            "hello",
            &["".to_string(), "world".to_string(), "".to_string()]
        ));
    }

    #[test]
    fn test_pathname_star_no_slash() {
        assert!(fnmatch_pathname("*.txt", "foo.txt"));
        assert!(!fnmatch_pathname("*.txt", "sub/foo.txt"));
    }

    #[test]
    fn test_pathname_prefix() {
        assert!(fnmatch_pathname("*/foo.txt", "sub/foo.txt"));
        assert!(!fnmatch_pathname("*/foo.txt", "sub/sub/foo.txt"));
    }

    #[test]
    fn test_pathname_question_no_slash() {
        assert!(fnmatch_pathname("?", "a"));
        assert!(!fnmatch_pathname("?", "/"));
    }

    #[test]
    fn test_pathname_segment_star() {
        assert!(fnmatch_pathname("sub/*.txt", "sub/foo.txt"));
        assert!(!fnmatch_pathname("a/b/d/c.nim", "a/*/c.nim"));
    }

    #[test]
    fn test_pathname_char_class_no_slash() {
        assert!(fnmatch_pathname("[ab]", "a"));
        assert!(!fnmatch_pathname("[ab]", "/"));
    }

    #[test]
    fn test_pathname_negation() {
        assert!(!match_glob_pathname("foo.txt", "!*.txt"));
        assert!(!match_glob_pathname("file.nim", "!*.nim"));
    }

    #[test]
    fn test_pathname_exact() {
        assert!(fnmatch_pathname("foo/bar.txt", "foo/bar.txt"));
        assert!(!fnmatch_pathname("dir/sub/file.nim", "dir/other.nim"));
    }

    #[test]
    fn test_trailing_star() {
        assert!(fnmatch_pathname("foo/*", "foo/bar"));
        assert!(!fnmatch_pathname("dir/sub/file", "dir/*"));
    }

    #[test]
    fn test_pathname_empty_pattern() {
        assert!(!match_glob_pathname("anything", ""));
        assert!(!match_glob_pathname("", ""));
    }

    #[test]
    fn test_pathname_star_matches_empty() {
        assert!(match_glob_pathname("", "*"));
        assert!(!match_glob_pathname("/", "*"));
    }
}
