//! What `dk show` displays, and what folding does to it.
//!
//! The card is shown as its own text — the same lines, in the same colours,
//! as a piped `dk show` — with every heading a fold. No terminal here: `ui.rs`
//! draws the rows and turns keys into calls on [`View`], so folding is tested
//! without a tty.

use crate::card::README_HEADING;
use crate::editor::syntax::{self, Kind};
use crate::outline;

/// One line of the card as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub kind: Kind,
    /// `Some(depth)` for a heading, which folds everything below it up to the
    /// next heading of the same or a shallower depth. The card's `# name` is
    /// depth 0, its `## sections` 1, a README's own headings 2 and deeper.
    pub depth: Option<usize>,
}

#[derive(Debug)]
pub struct View {
    pub lines: Vec<Line>,
    folded: Vec<bool>,
    /// Index into `lines`, always a visible line.
    pub cursor: usize,
    /// First display row on screen (wrapped rows, not lines).
    pub scroll: usize,
}

impl View {
    /// The card's sections open, its README shut: the README is reference
    /// material, and opened it would bury the card under it.
    pub fn new(text: &str) -> View {
        let lines = parse(text);
        let folded = lines
            .iter()
            .map(|l| l.depth == Some(1) && l.text.trim_end().eq_ignore_ascii_case(README_HEADING))
            .collect();
        View { lines, folded, cursor: 0, scroll: 0 }
    }

    pub fn is_heading(&self, at: usize) -> bool {
        self.lines.get(at).is_some_and(|l| l.depth.is_some())
    }

    pub fn is_folded(&self, at: usize) -> bool {
        self.folded.get(at).copied().unwrap_or(false)
    }

    /// One past the last line a heading owns.
    pub fn end_of(&self, at: usize) -> usize {
        let Some(depth) = self.lines.get(at).and_then(|l| l.depth) else {
            return at + 1;
        };
        self.lines[at + 1..]
            .iter()
            .position(|l| l.depth.is_some_and(|d| d <= depth))
            .map_or(self.lines.len(), |offset| at + 1 + offset)
    }

    /// Non-blank lines a folded heading is hiding, for the `… n lines` hint.
    pub fn hidden(&self, at: usize) -> usize {
        self.lines[at + 1..self.end_of(at)].iter().filter(|l| !l.text.trim().is_empty()).count()
    }

    /// The lines on screen, in order: every line not inside a folded heading.
    pub fn visible(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.lines.len());
        let mut at = 0;
        while at < self.lines.len() {
            out.push(at);
            at = if self.is_folded(at) { self.end_of(at) } else { at + 1 };
        }
        out
    }

    /// The heading a line sits under. For a heading, the one it sits under.
    pub fn parent(&self, at: usize) -> Option<usize> {
        let limit = self.lines.get(at)?.depth;
        (0..at).rev().find(|&h| {
            self.lines[h].depth.is_some_and(|d| limit.map_or(true, |l| d < l)) && self.end_of(h) > at
        })
    }

    // ── moves ─────────────────────────────────────────────────────────────

    fn step(&mut self, forward: bool) {
        let rows = self.visible();
        let Some(i) = rows.iter().position(|&l| l == self.cursor) else { return };
        let next = if forward { rows.get(i + 1) } else { i.checked_sub(1).and_then(|j| rows.get(j)) };
        if let Some(&line) = next {
            self.cursor = line;
        }
    }

    pub fn down(&mut self) {
        self.step(true);
    }

    pub fn up(&mut self) {
        self.step(false);
    }

    pub fn by(&mut self, count: usize, forward: bool) {
        for _ in 0..count {
            self.step(forward);
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        if let Some(&last) = self.visible().last() {
            self.cursor = last;
        }
    }

    /// The next (or previous) visible heading.
    pub fn next_heading(&mut self, forward: bool) {
        let rows = self.visible();
        let found = if forward {
            rows.iter().find(|&&l| l > self.cursor && self.is_heading(l))
        } else {
            rows.iter().rev().find(|&&l| l < self.cursor && self.is_heading(l))
        };
        if let Some(&line) = found {
            self.cursor = line;
        }
    }

    // ── folds ─────────────────────────────────────────────────────────────

    /// Enter: a heading flips; a line inside a section shuts that section and
    /// lands on its heading, so a fold is reachable from anywhere in it.
    pub fn toggle(&mut self) {
        if self.is_heading(self.cursor) {
            if self.folded[self.cursor] {
                self.unfold(self.cursor);
            } else {
                self.folded[self.cursor] = true;
            }
        } else if let Some(h) = self.parent(self.cursor) {
            self.folded[h] = true;
            self.cursor = h;
        }
    }

    /// → : open the heading under the cursor.
    pub fn open(&mut self) {
        if self.is_heading(self.cursor) {
            self.unfold(self.cursor);
        }
    }

    /// Open a heading, and keep opening while what it shows is a single
    /// heading and nothing else — `## readme` holding only `# docket` opens
    /// straight to the README's sections instead of one more fold.
    fn unfold(&mut self, at: usize) {
        let mut at = at;
        loop {
            self.folded[at] = false;
            let end = self.end_of(at);
            let children: Vec<usize> = (at + 1..end).filter(|&i| self.is_heading(i) && self.parent(i) == Some(at)).collect();
            let [only] = children.as_slice() else { return };
            let prose = self.lines[at + 1..*only].iter().any(|l| !l.text.trim().is_empty());
            if prose || !self.folded[*only] {
                return;
            }
            at = *only;
        }
    }

    /// ← : shut an open heading; on a shut heading or a plain line, go to (and
    /// for a line, shut) the heading above.
    pub fn close(&mut self) {
        let at = self.cursor;
        if self.is_heading(at) && !self.folded[at] {
            self.folded[at] = true;
        } else if let Some(h) = self.parent(at) {
            if !self.is_heading(at) {
                self.folded[h] = true;
            }
            self.cursor = h;
        }
    }

    /// Everything open, README included.
    pub fn open_all(&mut self) {
        self.folded.iter_mut().for_each(|f| *f = false);
    }

    /// Only the outline: the title and header fields, then one row per
    /// section.
    pub fn close_all(&mut self) {
        for (f, line) in self.folded.iter_mut().zip(&self.lines) {
            *f = line.depth.is_some_and(|d| d >= 1);
        }
        self.settle();
    }

    /// Keep the cursor on a visible line after folds change under it.
    pub fn settle(&mut self) {
        while !self.visible().contains(&self.cursor) {
            match self.parent(self.cursor) {
                Some(h) => self.cursor = h,
                None => {
                    self.cursor = 0;
                    break;
                }
            }
        }
    }
}

/// Lines, their colours, and which are headings. Colours come from the same
/// classifier as the piped output, so the view is the card you know.
pub fn parse(text: &str) -> Vec<Line> {
    let texts: Vec<String> = text.lines().map(String::from).collect();
    let kinds = syntax::classify(&texts);

    let mut out = Vec::with_capacity(texts.len());
    let mut fence: Option<usize> = None;
    let mut in_readme = false;
    // (markdown level, depth) of the README headings still open.
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for (i, (text, kind)) in texts.into_iter().zip(kinds).enumerate() {
        let ticks = outline::fence_marker(&text);
        match (fence, ticks) {
            (None, Some(n)) => fence = Some(n),
            (Some(open), Some(n)) if n >= open => fence = None,
            _ => {}
        }
        let in_fence = fence.is_some() || ticks.is_some();
        let level = if in_fence { None } else { outline::heading(&text).map(|(level, _)| level) };

        let depth = level.map(|level| {
            if i == 0 && kind == Kind::Title {
                0
            } else if in_readme {
                while stack.last().is_some_and(|(open, _)| *open >= level) {
                    stack.pop();
                }
                let depth = stack.last().map_or(2, |(_, d)| d + 1);
                stack.push((level, depth));
                depth
            } else if level == 2 && text.trim_end().eq_ignore_ascii_case(README_HEADING) {
                in_readme = true;
                1
            } else {
                // The card's own `## section` is 1; a `###` inside it is 2.
                level.saturating_sub(1).max(1)
            }
        });
        out.push(Line { text, kind, depth });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "\
# moxi
id: 0003aaaa
status: active

## now
state: parser
[ ] one

## next
[x] two

## readme

# Moxi

intro

## Install

```
# not a heading
cargo install moxi
```

### From source

build it
";

    fn line_of(view: &View, text: &str) -> usize {
        view.lines.iter().position(|l| l.text == text).unwrap_or_else(|| panic!("no line {text:?}"))
    }

    fn shown(view: &View) -> Vec<&str> {
        view.visible().into_iter().map(|i| view.lines[i].text.as_str()).collect()
    }

    #[test]
    fn headings_get_depths_from_the_title_down_through_the_readme() {
        let view = View::new(CARD);
        let depth = |t: &str| view.lines[line_of(&view, t)].depth;
        assert_eq!(depth("# moxi"), Some(0));
        assert_eq!(depth("## now"), Some(1));
        assert_eq!(depth("## readme"), Some(1));
        assert_eq!(depth("# Moxi"), Some(2), "the README's title sits under ## readme");
        assert_eq!(depth("## Install"), Some(3));
        assert_eq!(depth("### From source"), Some(4));
        assert_eq!(depth("# not a heading"), None, "a shell comment in a fence");
        assert_eq!(depth("state: parser"), None);
    }

    #[test]
    fn the_readme_starts_folded_and_the_card_open() {
        let view = View::new(CARD);
        let rows = shown(&view);
        assert!(rows.contains(&"## readme"));
        assert!(rows.contains(&"[ ] one"));
        assert!(!rows.contains(&"intro"));
        assert_eq!(view.hidden(line_of(&view, "## readme")), 9);
    }

    #[test]
    fn folding_a_section_hides_it_up_to_the_next_section() {
        let mut view = View::new(CARD);
        view.cursor = line_of(&view, "## now");
        view.toggle();
        let rows = shown(&view);
        assert!(!rows.contains(&"state: parser") && !rows.contains(&"[ ] one"));
        assert!(rows.contains(&"## next") && rows.contains(&"[x] two"));
        view.toggle();
        assert!(shown(&view).contains(&"[ ] one"));
    }

    #[test]
    fn folding_the_title_leaves_one_line() {
        let mut view = View::new(CARD);
        view.toggle();
        assert_eq!(shown(&view), vec!["# moxi"]);
    }

    #[test]
    fn enter_on_a_body_line_shuts_its_section_and_lands_on_it() {
        let mut view = View::new(CARD);
        view.cursor = line_of(&view, "[ ] one");
        view.toggle();
        assert_eq!(view.cursor, line_of(&view, "## now"));
        assert!(view.is_folded(view.cursor));
    }

    #[test]
    fn readme_headings_fold_inside_the_readme() {
        let mut view = View::new(CARD);
        view.open_all();
        view.cursor = line_of(&view, "## Install");
        view.toggle();
        let rows = shown(&view);
        assert!(rows.contains(&"intro") && rows.contains(&"## Install"));
        assert!(!rows.contains(&"cargo install moxi") && !rows.contains(&"### From source"));
    }

    #[test]
    fn close_all_is_the_outline() {
        let mut view = View::new(CARD);
        view.cursor = line_of(&view, "[x] two");
        view.close_all();
        assert_eq!(shown(&view), vec!["# moxi", "id: 0003aaaa", "status: active", "", "## now", "## next", "## readme"]);
        assert_eq!(view.cursor, line_of(&view, "## next"), "the cursor climbs out of what closed");
    }

    #[test]
    fn left_climbs_and_right_opens() {
        let mut view = View::new(CARD);
        view.cursor = line_of(&view, "## next");
        view.close();
        assert!(view.is_folded(view.cursor));
        view.close();
        assert_eq!(view.cursor, 0, "a shut section climbs to the title");
        view.cursor = line_of(&view, "## next");
        view.open();
        assert!(!view.is_folded(view.cursor));
    }

    #[test]
    fn moves_skip_what_is_folded() {
        let mut view = View::new(CARD);
        view.cursor = line_of(&view, "## readme");
        view.down();
        assert_eq!(view.cursor, line_of(&view, "## readme"), "nothing below a folded last section");
        view.cursor = line_of(&view, "## now");
        view.next_heading(true);
        assert_eq!(view.cursor, line_of(&view, "## next"));
        view.next_heading(false);
        view.next_heading(false);
        assert_eq!(view.cursor, 0);
    }

    #[test]
    fn opening_a_fold_that_holds_one_heading_opens_that_too() {
        let mut view = View::new(CARD);
        view.close_all();
        view.cursor = line_of(&view, "## readme");
        view.open();
        let rows = shown(&view);
        assert!(rows.contains(&"# Moxi") && rows.contains(&"intro"), "{rows:?}");
        assert!(rows.contains(&"## Install") && !rows.contains(&"cargo install moxi"), "{rows:?}");
    }

    #[test]
    fn colours_match_the_piped_output() {
        let view = View::new(CARD);
        assert_eq!(view.lines[0].kind, Kind::Title);
        assert_eq!(view.lines[line_of(&view, "## now")].kind, Kind::Section);
        assert_eq!(view.lines[line_of(&view, "[ ] one")].kind, Kind::Open);
        assert_eq!(view.lines[line_of(&view, "[x] two")].kind, Kind::Ticked);
    }
}
