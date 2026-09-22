//! Where text lands on screen. Long lines wrap at the terminal width, so one
//! line of the card can occupy several screen rows; everything here converts
//! between the two. Pure arithmetic, no terminal.
//!
//! A character is assumed to be one column wide. Wide CJK glyphs and emoji
//! push the cursor slightly off on the lines that contain them; the text
//! itself is never affected.

/// A screen row: which logical line it came from, and the text on it. The
/// line number travels with the text so the caller can colour a row by what
/// kind of line it belongs to.
#[derive(Debug, PartialEq, Eq)]
pub struct Segment {
    pub line: usize,
    pub text: String,
}

/// Screen rows a line of `len` characters needs. Always one more slot than the
/// text strictly requires, so the cursor has somewhere to sit at the very end
/// of a line that exactly fills the width.
pub fn rows_of(len: usize, width: usize) -> usize {
    len / width.max(1) + 1
}

/// The screen row, counted from the top of the document, holding the cursor.
pub fn cursor_row(lines: &[String], row: usize, col: usize, width: usize) -> usize {
    let width = width.max(1);
    let above: usize = lines[..row]
        .iter()
        .map(|l| rows_of(l.chars().count(), width))
        .sum();
    above + col / width
}

/// Scroll just far enough to keep the cursor visible.
pub fn follow(top: usize, cursor: usize, height: usize) -> usize {
    let height = height.max(1);
    if cursor < top {
        cursor
    } else if cursor >= top + height {
        cursor + 1 - height
    } else {
        top
    }
}

/// The screen rows `top .. top + height`.
pub fn visible(lines: &[String], width: usize, top: usize, height: usize) -> Vec<Segment> {
    let width = width.max(1);
    let mut out = Vec::with_capacity(height);
    let mut index = 0;

    for (line, text) in lines.iter().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        for segment in 0..rows_of(chars.len(), width) {
            if index >= top + height {
                return out;
            }
            if index >= top {
                let start = segment * width;
                let end = (start + width).min(chars.len());
                out.push(Segment {
                    line,
                    text: chars[start.min(end)..end].iter().collect(),
                });
            }
            index += 1;
        }
    }
    out
}

/// Cut or pad to exactly `width` columns, for the bars at the bottom.
pub fn fit(text: &str, width: usize) -> String {
    let mut out: String = text.chars().take(width).collect();
    let used = out.chars().count();
    out.extend(std::iter::repeat(' ').take(width - used));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|s| s.to_string()).collect()
    }

    fn texts(segments: &[Segment]) -> Vec<&str> {
        segments.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn short_lines_take_one_row_and_full_lines_take_two() {
        assert_eq!(rows_of(0, 10), 1);
        assert_eq!(rows_of(9, 10), 1);
        assert_eq!(rows_of(10, 10), 2);
        assert_eq!(rows_of(25, 10), 3);
    }

    #[test]
    fn cursor_row_counts_wrapped_rows_above() {
        let text = lines(&["abcdefghij", "xy"]);
        assert_eq!(cursor_row(&text, 0, 0, 4), 0);
        assert_eq!(cursor_row(&text, 0, 9, 4), 2);
        assert_eq!(cursor_row(&text, 1, 1, 4), 3);
    }

    #[test]
    fn follow_scrolls_only_when_the_cursor_leaves_the_window() {
        assert_eq!(follow(0, 3, 10), 0);
        assert_eq!(follow(0, 10, 10), 1);
        assert_eq!(follow(5, 2, 10), 2);
        assert_eq!(follow(5, 14, 10), 5);
    }

    #[test]
    fn visible_wraps_and_windows() {
        let text = lines(&["abcdefghij", "xy"]);
        assert_eq!(texts(&visible(&text, 4, 0, 10)), vec!["abcd", "efgh", "ij", "xy"]);
        assert_eq!(texts(&visible(&text, 4, 1, 2)), vec!["efgh", "ij"]);
    }

    #[test]
    fn every_segment_names_its_source_line() {
        let segments = visible(&lines(&["abcdefghij", "xy"]), 4, 0, 10);
        assert_eq!(
            segments.iter().map(|s| s.line).collect::<Vec<_>>(),
            vec![0, 0, 0, 1]
        );
    }

    #[test]
    fn a_line_that_fills_the_width_gets_an_empty_row_for_the_cursor() {
        assert_eq!(texts(&visible(&lines(&["abcd"]), 4, 0, 10)), vec!["abcd", ""]);
    }

    #[test]
    fn fit_pads_and_truncates() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 4), "abcd");
        assert_eq!(fit("añ", 3), "añ ");
        assert_eq!(fit("x", 0), "");
    }
}
