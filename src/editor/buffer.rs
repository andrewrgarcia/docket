//! The text and the cursor. Nothing here knows a terminal exists, which is what
//! lets every editing rule be tested without one.
//!
//! Columns are counted in characters, never bytes, so a multi-byte character
//! can never be split.

const HISTORY: usize = 500;

/// Consecutive edits of the same kind share one undo step, so Ctrl-Z removes a
/// typed word rather than a single letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    Insert,
    Remove,
    /// Newlines, pastes, line cuts: always their own step.
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

#[derive(Debug)]
pub struct Buffer {
    lines: Vec<String>,
    row: usize,
    col: usize,
    dirty: bool,
    history: Vec<Snapshot>,
    last: Option<Edit>,
    cut: Option<String>,
}

impl Buffer {
    pub fn from_text(text: &str) -> Buffer {
        let mut lines: Vec<String> = text
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();
        // "a\n" splits into ["a", ""]; the trailing empty piece is the final
        // newline, not a line of its own.
        if lines.len() > 1 && lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        Buffer {
            lines,
            row: 0,
            col: 0,
            dirty: false,
            history: Vec::new(),
            last: None,
            cut: None,
        }
    }

    /// Always ends with exactly one newline.
    pub fn text(&self) -> String {
        let mut out = self.lines.join("\n");
        out.push('\n');
        out
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Put the cursor on the line after the first line equal to `heading`.
    pub fn jump_below(&mut self, heading: &str) {
        if let Some(at) = self.lines.iter().position(|l| l.trim_end() == heading) {
            self.row = (at + 1).min(self.lines.len() - 1);
            self.col = self.len(self.row);
        }
    }

    // ------------------------------------------------------------- editing

    pub fn insert_char(&mut self, c: char) {
        self.checkpoint(Edit::Insert);
        self.raw_insert(c);
    }

    /// A paste. Terminals send `\r`, `\n` or `\r\n` for a line break depending
    /// on platform and mood, so all three mean the same thing here.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.checkpoint(Edit::Block);
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    self.raw_newline();
                }
                '\n' => self.raw_newline(),
                '\t' => {
                    self.raw_insert(' ');
                    self.raw_insert(' ');
                }
                c if c.is_control() => {}
                c => self.raw_insert(c),
            }
        }
    }

    pub fn newline(&mut self) {
        self.checkpoint(Edit::Block);
        self.raw_newline();
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            self.checkpoint(Edit::Remove);
            let at = self.byte_at(self.row, self.col - 1);
            self.lines[self.row].remove(at);
            self.col -= 1;
        } else if self.row > 0 {
            self.checkpoint(Edit::Block);
            let tail = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.len(self.row);
            self.lines[self.row].push_str(&tail);
        }
    }

    pub fn delete(&mut self) {
        if self.col < self.len(self.row) {
            self.checkpoint(Edit::Remove);
            let at = self.byte_at(self.row, self.col);
            self.lines[self.row].remove(at);
        } else if self.row + 1 < self.lines.len() {
            self.checkpoint(Edit::Block);
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
        }
    }

    pub fn cut_line(&mut self) {
        self.checkpoint(Edit::Block);
        let line = self.lines.remove(self.row);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = self.row.min(self.lines.len() - 1);
        self.col = self.col.min(self.len(self.row));
        self.cut = Some(line);
    }

    /// Returns false when nothing has been cut yet.
    pub fn paste_line(&mut self) -> bool {
        let Some(line) = self.cut.clone() else {
            return false;
        };
        self.checkpoint(Edit::Block);
        self.lines.insert(self.row, line);
        self.col = 0;
        true
    }

    /// Flip the checkbox on the cursor line. False when there is none.
    pub fn toggle_checkbox(&mut self) -> bool {
        let Some(flipped) = crate::checkbox::toggle(&self.lines[self.row]) else {
            return false;
        };
        self.checkpoint(Edit::Block);
        self.lines[self.row] = flipped;
        true
    }

    /// Move to the next line holding a checkbox, wrapping round. False when
    /// the card has none.
    pub fn next_checkbox(&mut self) -> bool {
        self.seek_checkbox(1)
    }

    pub fn prev_checkbox(&mut self) -> bool {
        self.seek_checkbox(-1)
    }

    fn seek_checkbox(&mut self, step: isize) -> bool {
        let n = self.lines.len() as isize;
        let mut row = self.row as isize;
        for _ in 0..n {
            row = (row + step).rem_euclid(n);
            if crate::checkbox::find(&self.lines[row as usize]).is_some() {
                self.last = None;
                self.row = row as usize;
                self.col = self.len(self.row);
                return true;
            }
        }
        false
    }

    /// Returns false when there is nothing left to undo.
    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.history.pop() else {
            return false;
        };
        self.lines = snapshot.lines;
        self.row = snapshot.row;
        self.col = snapshot.col;
        self.dirty = true;
        self.last = None;
        true
    }

    // ------------------------------------------------------------ movement
    //
    // Vertical movement takes the screen width because long lines wrap: "up"
    // means the row above on screen, which may be the same logical line.

    pub fn left(&mut self) {
        self.last = None;
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.len(self.row);
        }
    }

    pub fn right(&mut self) {
        self.last = None;
        if self.col < self.len(self.row) {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn up(&mut self, width: usize) {
        self.last = None;
        let width = width.max(1);
        if self.col >= width {
            self.col -= width;
        } else if self.row > 0 {
            self.row -= 1;
            let len = self.len(self.row);
            let last_segment = (len / width) * width;
            self.col = (last_segment + self.col).min(len);
        }
    }

    pub fn down(&mut self, width: usize) {
        self.last = None;
        let width = width.max(1);
        let len = self.len(self.row);
        if self.col + width <= len {
            self.col += width;
        } else if self.col / width < len / width {
            self.col = len;
        } else if self.row + 1 < self.lines.len() {
            let screen_col = self.col % width;
            self.row += 1;
            self.col = screen_col.min(self.len(self.row));
        }
    }

    pub fn page_up(&mut self, rows: usize, width: usize) {
        for _ in 0..rows {
            self.up(width);
        }
    }

    pub fn page_down(&mut self, rows: usize, width: usize) {
        for _ in 0..rows {
            self.down(width);
        }
    }

    pub fn home(&mut self) {
        self.last = None;
        self.col = 0;
    }

    pub fn end(&mut self) {
        self.last = None;
        self.col = self.len(self.row);
    }

    pub fn top(&mut self) {
        self.last = None;
        self.row = 0;
        self.col = 0;
    }

    pub fn bottom(&mut self) {
        self.last = None;
        self.row = self.lines.len() - 1;
        self.col = self.len(self.row);
    }

    // ------------------------------------------------------------ internals

    fn len(&self, row: usize) -> usize {
        self.lines[row].chars().count()
    }

    fn byte_at(&self, row: usize, col: usize) -> usize {
        let line = &self.lines[row];
        line.char_indices().nth(col).map_or(line.len(), |(i, _)| i)
    }

    fn raw_insert(&mut self, c: char) {
        let at = self.byte_at(self.row, self.col);
        self.lines[self.row].insert(at, c);
        self.col += 1;
    }

    fn raw_newline(&mut self) {
        let at = self.byte_at(self.row, self.col);
        let tail = self.lines[self.row].split_off(at);
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.col = 0;
    }

    fn checkpoint(&mut self, kind: Edit) {
        let grouped = kind != Edit::Block && self.last == Some(kind);
        if !grouped {
            if self.history.len() == HISTORY {
                self.history.remove(0);
            }
            self.history.push(Snapshot {
                lines: self.lines.clone(),
                row: self.row,
                col: self.col,
            });
        }
        self.last = Some(kind);
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(buffer: &mut Buffer, text: &str) {
        for c in text.chars() {
            buffer.insert_char(c);
        }
    }

    #[test]
    fn round_trips_text_with_one_trailing_newline() {
        assert_eq!(Buffer::from_text("a\nb\n").text(), "a\nb\n");
        assert_eq!(Buffer::from_text("a\nb").text(), "a\nb\n");
        assert_eq!(Buffer::from_text("").text(), "\n");
        assert_eq!(Buffer::from_text("a\r\nb\r\n").text(), "a\nb\n");
    }

    #[test]
    fn keeps_blank_lines_in_the_middle() {
        assert_eq!(Buffer::from_text("a\n\n\nb\n").lines().len(), 4);
    }

    #[test]
    fn a_clean_buffer_is_not_dirty_until_edited() {
        let mut b = Buffer::from_text("a\n");
        b.right();
        assert!(!b.is_dirty());
        b.insert_char('x');
        assert!(b.is_dirty());
        b.mark_saved();
        assert!(!b.is_dirty());
    }

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut b = Buffer::from_text("ac\n");
        b.right();
        b.insert_char('b');
        assert_eq!(b.text(), "abc\n");
        assert_eq!(b.cursor(), (0, 2));
    }

    #[test]
    fn multibyte_characters_are_never_split() {
        let mut b = Buffer::from_text("añb\n");
        b.end();
        b.backspace();
        b.backspace();
        assert_eq!(b.text(), "a\n");
        typed(&mut b, "日本");
        b.left();
        b.insert_char('x');
        assert_eq!(b.text(), "a日x本\n");
    }

    #[test]
    fn enter_splits_and_backspace_rejoins() {
        let mut b = Buffer::from_text("hello world\n");
        for _ in 0..5 {
            b.right();
        }
        b.newline();
        assert_eq!(b.text(), "hello\n world\n");
        assert_eq!(b.cursor(), (1, 0));
        b.backspace();
        assert_eq!(b.text(), "hello world\n");
        assert_eq!(b.cursor(), (0, 5));
    }

    #[test]
    fn delete_at_line_end_pulls_the_next_line_up() {
        let mut b = Buffer::from_text("a\nb\n");
        b.end();
        b.delete();
        assert_eq!(b.text(), "ab\n");
        b.delete();
        assert_eq!(b.text(), "a\n");
    }

    #[test]
    fn edits_at_the_edges_do_nothing() {
        let mut b = Buffer::from_text("a\n");
        b.backspace();
        b.left();
        b.up(80);
        assert_eq!(b.cursor(), (0, 0));
        b.end();
        b.delete();
        b.right();
        b.down(80);
        assert_eq!(b.text(), "a\n");
        assert_eq!(b.cursor(), (0, 1));
    }

    #[test]
    fn paste_accepts_every_kind_of_line_break() {
        for pasted in ["one\ntwo", "one\r\ntwo", "one\rtwo"] {
            let mut b = Buffer::from_text("");
            b.insert_text(pasted);
            assert_eq!(b.text(), "one\ntwo\n", "for {pasted:?}");
        }
    }

    #[test]
    fn paste_drops_control_characters_and_expands_tabs() {
        let mut b = Buffer::from_text("");
        b.insert_text("a\tb\u{1b}[31mc");
        assert_eq!(b.text(), "a  b[31mc\n");
    }

    #[test]
    fn undo_removes_a_typed_run_not_a_letter() {
        let mut b = Buffer::from_text("");
        typed(&mut b, "hello");
        assert!(b.undo());
        assert_eq!(b.text(), "\n");
        assert!(!b.undo());
    }

    #[test]
    fn moving_the_cursor_starts_a_new_undo_step() {
        let mut b = Buffer::from_text("");
        typed(&mut b, "ab");
        b.left();
        b.right();
        typed(&mut b, "cd");
        b.undo();
        assert_eq!(b.text(), "ab\n");
    }

    #[test]
    fn a_paste_is_one_undo_step() {
        let mut b = Buffer::from_text("x\n");
        b.insert_text("one\ntwo\nthree");
        b.undo();
        assert_eq!(b.text(), "x\n");
    }

    #[test]
    fn cut_and_paste_move_a_line() {
        let mut b = Buffer::from_text("a\nb\nc\n");
        b.cut_line();
        assert_eq!(b.text(), "b\nc\n");
        b.down(80);
        assert!(b.paste_line());
        assert_eq!(b.text(), "b\na\nc\n");
    }

    #[test]
    fn cutting_the_only_line_leaves_an_empty_one() {
        let mut b = Buffer::from_text("only\n");
        b.cut_line();
        assert_eq!(b.text(), "\n");
        assert_eq!(b.cursor(), (0, 0));
    }

    #[test]
    fn paste_line_without_a_cut_reports_it() {
        assert!(!Buffer::from_text("a\n").paste_line());
    }

    #[test]
    fn up_and_down_follow_wrapped_rows() {
        // Width 4: "abcdefghij" shows as abcd / efgh / ij
        let mut b = Buffer::from_text("abcdefghij\nxy\n");
        b.down(4);
        assert_eq!(b.cursor(), (0, 4));
        b.down(4);
        assert_eq!(b.cursor(), (0, 8));
        b.down(4);
        assert_eq!(b.cursor(), (1, 0));
        b.up(4);
        assert_eq!(b.cursor(), (0, 8));
        b.up(4);
        assert_eq!(b.cursor(), (0, 4));
    }

    #[test]
    fn down_onto_a_short_last_row_clamps_to_the_line_end() {
        let mut b = Buffer::from_text("abcdefghij\n");
        for _ in 0..7 {
            b.right();
        }
        b.down(4);
        assert_eq!(b.cursor(), (0, 10));
    }

    #[test]
    fn a_zero_width_terminal_does_not_divide_by_zero() {
        let mut b = Buffer::from_text("abc\ndef\n");
        b.down(0);
        b.up(0);
        assert_eq!(b.cursor().0, 0);
    }

    #[test]
    fn toggling_a_checkbox_is_one_undo_step() {
        let mut b = Buffer::from_text("[ ] a\nprose\n");
        assert!(b.toggle_checkbox());
        assert_eq!(b.lines()[0], "[x] a");
        assert!(b.undo());
        assert_eq!(b.lines()[0], "[ ] a");
        b.down(80);
        assert!(!b.toggle_checkbox(), "prose has no box");
    }

    #[test]
    fn next_and_prev_checkbox_wrap_round() {
        let mut b = Buffer::from_text("## now\n[ ] a\ntext\n[x] b\n");
        assert!(b.next_checkbox());
        assert_eq!(b.cursor().0, 1);
        assert!(b.next_checkbox());
        assert_eq!(b.cursor().0, 3);
        assert!(b.next_checkbox());
        assert_eq!(b.cursor().0, 1, "wrapped");
        assert!(b.prev_checkbox());
        assert_eq!(b.cursor().0, 3);
        assert!(!Buffer::from_text("no boxes\n").next_checkbox());
    }

    #[test]
    fn jump_below_lands_under_the_heading() {
        let mut b = Buffer::from_text("# x\nstatus: idea\n\n## now\nparser\n\n## next\n");
        b.jump_below("## now");
        assert_eq!(b.cursor(), (4, 6));
        b.jump_below("## missing");
        assert_eq!(b.cursor(), (4, 6));
    }
}
