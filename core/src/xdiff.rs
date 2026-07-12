use similar::{ChangeTag, TextDiff};

fn ends_with_newline(s: &str) -> bool {
    !s.is_empty() && s.as_bytes()[s.len() - 1] == b'\n'
}

fn strip_trailing_newline(s: &str) -> &str {
    if !s.is_empty() && s.as_bytes()[s.len() - 1] == b'\n' {
        &s[..s.len() - 1]
    } else {
        s
    }
}

pub fn diff(a: &str, b: &str, ctx_len: usize) -> String {
    if a == b {
        return String::new();
    }

    let a_no_nl = !ends_with_newline(a) && !a.is_empty();
    let b_no_nl = !ends_with_newline(b) && !b.is_empty();

    let norm_a = strip_trailing_newline(a);
    let norm_b = strip_trailing_newline(b);

    if norm_a.is_empty() && norm_b.is_empty() {
        return String::new();
    }

    if norm_a.is_empty() {
        let lines: Vec<&str> = norm_b.lines().collect();
        let mut result = String::new();
        result.push_str(&format!("@@ -0,0 +1,{} @@\n", lines.len()));
        for line in &lines {
            result.push_str(&format!("+{}\n", line));
        }
        return result;
    }

    if norm_b.is_empty() {
        let lines: Vec<&str> = norm_a.lines().collect();
        let mut result = String::new();
        result.push_str(&format!("@@ -1,{} +0,0 @@\n", lines.len()));
        for line in &lines {
            result.push_str(&format!("-{}\n", line));
        }
        return result;
    }

    let diff = TextDiff::from_lines(norm_a, norm_b);

    if !diff
        .ops()
        .iter()
        .any(|op| op.tag() != similar::DiffTag::Equal)
    {
        let lines_a: Vec<&str> = norm_a.lines().collect();
        let lines_b: Vec<&str> = norm_b.lines().collect();
        if lines_a.len() == lines_b.len() && !lines_a.is_empty() {
            let mut same_except_last = true;
            for k in 0..lines_a.len().saturating_sub(1) {
                if lines_a[k] != lines_b[k] {
                    same_except_last = false;
                    break;
                }
            }
            if same_except_last
                && lines_a[lines_a.len() - 1] == lines_b[lines_b.len() - 1]
                && a_no_nl != b_no_nl
            {
                let last_idx = lines_a.len() - 1;
                let mut result = String::new();
                result.push_str(&format!(
                    "@@ -{},{} +{},{} @@\n",
                    last_idx + 1,
                    1,
                    last_idx + 1,
                    1
                ));
                result.push_str(&format!("-{}\n", lines_a[last_idx]));
                result.push_str(&format!("+{}\n", lines_b[last_idx]));
                return result;
            }
        }
        return String::new();
    }

    let groups = diff.grouped_ops(ctx_len);
    let mut result = String::new();

    for (gi, group) in groups.iter().enumerate() {
        let is_last = gi == groups.len() - 1;
        let first_op = group.first().unwrap();
        let last_op = group.last().unwrap();

        let old_start = first_op.old_range().start;
        let new_start = first_op.new_range().start;
        let old_end = last_op.old_range().end;
        let new_end = last_op.new_range().end;

        let count_a = old_end - old_start;
        let count_b = new_end - new_start;

        let start_a_disp = if count_a == 0 {
            old_start
        } else {
            old_start + 1
        };
        let start_b_disp = if count_b == 0 {
            new_start
        } else {
            new_start + 1
        };

        result.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            start_a_disp, count_a, start_b_disp, count_b
        ));

        let mut last_delete_idx: Option<usize> = None;
        let mut last_add_idx: Option<usize> = None;
        let mut lines_out: Vec<(char, String)> = Vec::new();

        let changes = diff.iter_all_changes().filter(|c| {
            let old_idx = c.old_index().unwrap_or(0);
            match c.tag() {
                ChangeTag::Equal => old_idx >= old_start && old_idx < old_end,
                ChangeTag::Delete => old_idx >= old_start && old_idx < old_end,
                ChangeTag::Insert => {
                    let ni = c.new_index().unwrap();
                    ni >= new_start && ni < new_end
                }
            }
        });

        for change in changes {
            let line = change.value().strip_suffix('\n').unwrap_or(change.value());
            match change.tag() {
                ChangeTag::Equal => {
                    lines_out.push((' ', line.to_string()));
                }
                ChangeTag::Delete => {
                    last_delete_idx = Some(lines_out.len());
                    lines_out.push(('-', line.to_string()));
                }
                ChangeTag::Insert => {
                    last_add_idx = Some(lines_out.len());
                    lines_out.push(('+', line.to_string()));
                }
            }
        }

        for &(tag, ref line) in &lines_out {
            result.push_str(&format!("{}{}\n", tag, line));
        }

        if is_last {
            let no_nl =
                (a_no_nl && last_delete_idx.is_some()) || (b_no_nl && last_add_idx.is_some());
            if no_nl {
                result.push_str("\\ No newline at end of file\n");
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_inputs() {
        assert_eq!(diff("", "", 3), "");
    }

    #[test]
    fn test_same_content() {
        assert_eq!(diff("hello\nworld\n", "hello\nworld\n", 3), "");
    }

    #[test]
    fn test_single_change() {
        let result = diff("hello\n", "world\n", 3);
        assert!(result.contains("-hello"));
        assert!(result.contains("+world"));
        assert!(result.contains("@@"));
    }

    #[test]
    fn test_addition() {
        let result = diff("", "hello\n", 3);
        assert!(result.contains("+hello"));
        assert!(result.contains("@@ -0,0 +1,1 @@"));
    }

    #[test]
    fn test_deletion() {
        let result = diff("hello\n", "", 3);
        assert!(result.contains("-hello"));
        assert!(result.contains("@@ -1,1 +0,0 @@"));
    }

    #[test]
    fn test_context_window() {
        let a = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let b = "a\nb\nX\nd\ne\nY\ng\nh\n";
        let result = diff(a, b, 2);
        assert!(result.contains("-c"));
        assert!(result.contains("+X"));
        assert!(result.contains("-f"));
        assert!(result.contains("+Y"));
    }

    #[test]
    fn test_no_newline_at_eof() {
        let result = diff("hello", "world", 3);
        assert!(result.contains("\\ No newline at end of file"));
    }

    #[test]
    fn test_header_format() {
        let result = diff("hello\n", "world\n", 3);
        assert!(result.starts_with("@@ -1,1 +1,1 @@"));
    }

    #[test]
    fn test_single_line_added() {
        let result = diff("line1\n", "line1\nline2\n", 3);
        assert!(result.contains("+line2"));
    }

    #[test]
    fn test_single_line_deleted() {
        let result = diff("line1\nline2\n", "line1\n", 3);
        assert!(result.contains("-line2"));
    }

    #[test]
    fn test_ctx_len_zero() {
        let a = "line1\nline2\nline3\nline4\nline5\n";
        let b = "line1\nline2\nCHG3\nline4\nline5\n";
        let result = diff(a, b, 0);
        assert!(!result.contains(" line2"));
        assert!(result.contains("-line3"));
        assert!(result.contains("+CHG3"));
    }

    #[test]
    fn test_ctx_len_one() {
        let a = "line1\nline2\nline3\nline4\nline5\n";
        let b = "line1\nline2\nCHG3\nline4\nline5\n";
        let result = diff(a, b, 1);
        assert!(!result.contains(" line1"));
        assert!(result.contains(" line2"));
        assert!(result.contains(" line4"));
    }

    #[test]
    fn test_merge_adjacent_hunks() {
        let a = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\n";
        let b = "line1\nCHG2\nline3\nCHG4\nline5\nline6\nline7\nline8\n";
        let result = diff(a, b, 1);
        assert!(result.contains("-line2"));
        assert!(result.contains("+CHG2"));
        assert!(result.contains("-line4"));
        assert!(result.contains("+CHG4"));
        assert_eq!(result.find("@@ -"), result.rfind("@@ -"));
    }

    #[test]
    fn test_hunk_header_zero_count_addition() {
        let result = diff("", "line1\nline2\n", 3);
        assert!(result.contains("@@ -0,0 +1,2 @@"));
    }

    #[test]
    fn test_hunk_header_zero_count_deletion() {
        let result = diff("line1\nline2\n", "", 3);
        assert!(result.contains("@@ -1,2 +0,0 @@"));
    }

    #[test]
    fn test_mixed_trailing_newline() {
        let result = super::diff("hello\n", "hello", 3);
        assert!(!result.is_empty(), "result should not be empty");
        assert!(
            result.contains("-hello"),
            "expected -hello in result: {:?}",
            result
        );
    }

    #[test]
    fn test_multiline_different_lengths() {
        let a = "line1\nline2\nline3\n";
        let b = "line1\nadded1\nadded2\nline2\nline3\n";
        let result = diff(a, b, 3);
        assert!(result.contains("+added1"));
        assert!(result.contains("+added2"));
        assert!(result.contains(" line1"));
    }

    #[test]
    fn test_large_identical_prefix() {
        let mut a = String::new();
        let mut b = String::new();
        for i in 1..=20 {
            a.push_str(&format!("line{i}\n"));
            b.push_str(&format!("line{i}\n"));
        }
        b = b.replace("line10\n", "modified10\n");
        let result = diff(&a, &b, 2);
        assert!(result.contains("-line10"));
        assert!(result.contains("+modified10"));
        assert!(!result.contains(" line6\n"));
        assert!(!result.contains(" line14\n"));
    }
}
