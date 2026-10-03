//! The book index: what bare `dk` shows when there is more than one book.
//!
//! One row per book, the default marked and under the cursor at the start.
//! Enter prints that book's cards and leaves; `p` opens `pick` on it. Nothing
//! is remembered afterwards — choosing a book here is for this one look, which
//! is the same rule `-b` follows.
//!
//! The layout is computed as plain text first (`layout`), so its widths can be
//! tested, and painted second.

use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::books::{self, Summary};
use crate::error::{Error, Result};

const MUTED: Color = Color::Rgb { r: 130, g: 130, b: 130 };
const STEM: Color = Color::Rgb { r: 90, g: 90, b: 90 };
const GREEN: Color = Color::Rgb { r: 120, g: 220, b: 120 };
const RED: Color = Color::Rgb { r: 255, g: 95, b: 95 };

/// Header, then a blank line.
const LIST_TOP: u16 = 2;
/// Blank, then keys.
const FOOTER_ROWS: u16 = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    /// Enter: list this book's cards.
    List(String),
    /// `p`: open `pick` on this book.
    Pick(String),
    Quit,
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Stay,
    Done(Choice),
}

/// Restores the terminal however the loop ends.
struct TermGuard;

impl TermGuard {
    fn enter() -> io::Result<TermGuard> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(TermGuard)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

pub fn run(rows: &[Summary]) -> Result<Choice> {
    if rows.is_empty() {
        return Ok(Choice::Quit);
    }
    let _guard = TermGuard::enter().map_err(term)?;
    let mut out = io::stdout();
    let mut cursor = start(rows);
    let mut scroll = 0;
    let mut flash: Option<String> = None;

    loop {
        draw(rows, cursor, &mut scroll, flash.as_deref(), &mut out).map_err(term)?;
        let Event::Key(key) = event::read().map_err(term)? else { continue };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        flash = None;
        match on_key(rows, &mut cursor, key) {
            Step::Stay => {}
            Step::Done(Choice::List(name) | Choice::Pick(name))
                if rows.iter().any(|r| r.name == name && r.counts.is_none()) =>
            {
                flash = Some(format!("`{name}`'s folder is missing — `dk book` shows where it should be"));
            }
            Step::Done(choice) => return Ok(choice),
        }
    }
}

fn term(e: io::Error) -> Error {
    Error::other(format!("terminal: {e}"))
}

/// The cursor starts on the default book, so Enter alone does what `dk`
/// used to do.
fn start(rows: &[Summary]) -> usize {
    rows.iter().position(|r| r.default).unwrap_or(0)
}

fn on_key(rows: &[Summary], cursor: &mut usize, key: KeyEvent) -> Step {
    let last = rows.len().saturating_sub(1);
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Step::Done(Choice::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Step::Done(Choice::Quit),
        KeyCode::Up | KeyCode::Char('k') => *cursor = cursor.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => *cursor = (*cursor + 1).min(last),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = last,
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
            if let Some(row) = rows.get(*cursor) {
                return Step::Done(Choice::List(row.name.clone()));
            }
        }
        KeyCode::Char('p') => {
            if let Some(row) = rows.get(*cursor) {
                return Step::Done(Choice::Pick(row.name.clone()));
            }
        }
        _ => {}
    }
    Step::Stay
}

/// The text of one row, already fitted to `width` characters: mark, name,
/// counts, then as much of the path as is left. The path is what gives way.
#[derive(Debug, PartialEq, Eq)]
struct Line {
    mark: String,
    name: String,
    counts: String,
    path: String,
    missing: bool,
}

impl Line {
    fn len(&self) -> usize {
        [&self.mark, &self.name, &self.counts, &self.path]
            .iter()
            .map(|s| s.chars().count())
            .sum()
    }
}

fn layout(rows: &[Summary], width: usize) -> Vec<Line> {
    let name_w = rows.iter().map(|r| r.name.chars().count()).max().unwrap_or(4).max(4);
    rows.iter()
        .map(|r| {
            let mark = if r.default { "* " } else { "  " }.to_string();
            let name = format!("{:<name_w$}  ", r.name);
            let (counts, path, missing) = match &r.counts {
                Some(c) => (
                    format!(
                        "{:>3} cards  {:>3} active  {:>6}  ",
                        c.cards,
                        c.active,
                        books::age_label(c.newest)
                    ),
                    r.path.display().to_string(),
                    false,
                ),
                None => (
                    format!("{:>3} cards  {:>3} active  {:>6}  ", "-", "-", "-"),
                    format!("{} (missing folder)", r.path.display()),
                    true,
                ),
            };
            let mut line = Line { mark, name, counts, path, missing };
            // Fit: the path shrinks first, then the counts, then the name.
            let fixed = line.mark.chars().count() + line.name.chars().count() + line.counts.chars().count();
            line.path = cut(&line.path, width.saturating_sub(fixed));
            if line.len() > width {
                line.counts = cut(&line.counts, width.saturating_sub(line.mark.chars().count() + line.name.chars().count()));
            }
            if line.len() > width {
                line.name = cut(&line.name, width.saturating_sub(line.mark.chars().count()));
            }
            if line.len() > width {
                line.mark = cut(&line.mark, width);
            }
            line
        })
        .collect()
}

const KEYS: &[(&str, &str, &str)] = &[
    ("enter", "open", "open"),
    ("↑↓", "move", "move"),
    ("p", "pick", "pick"),
    ("q", "quit", "quit"),
];

fn legend(width: usize) -> String {
    for stage in 0..3 {
        let line = KEYS
            .iter()
            .map(|(key, long, short)| match stage {
                0 => format!("{key} {long}"),
                1 => format!("{key} {short}"),
                _ => (*key).to_string(),
            })
            .collect::<Vec<_>>()
            .join(" · ");
        if line.chars().count() <= width {
            return line;
        }
    }
    cut(&KEYS.iter().map(|(k, _, _)| *k).collect::<Vec<_>>().join(" · "), width)
}

fn draw(rows: &[Summary], cursor: usize, scroll: &mut usize, flash: Option<&str>, out: &mut impl Write) -> io::Result<()> {
    let (width, height) = terminal::size().unwrap_or((80, 24));
    let w = width as usize;
    let list_height = height.saturating_sub(LIST_TOP + FOOTER_ROWS).max(1) as usize;
    if cursor < *scroll {
        *scroll = cursor;
    } else if cursor >= *scroll + list_height {
        *scroll = cursor + 1 - list_height;
    }

    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let cards: usize = rows.iter().filter_map(|r| r.counts.map(|c| c.cards)).sum();
    let header = cut(&format!("{} books · {cards} cards", rows.len()), w.saturating_sub(12));
    queue!(
        buf,
        MoveTo(0, 0),
        Clear(ClearType::All),
        SetForegroundColor(Color::Magenta),
        SetAttribute(Attribute::Bold),
        Print(cut("▦ dk books", w)),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print("  "),
        SetForegroundColor(MUTED),
        Print(header),
        ResetColor,
    )?;

    let lines = layout(rows, w);
    for (i, index) in (*scroll..rows.len().min(*scroll + list_height)).enumerate() {
        let (Some(line), Some(row)) = (lines.get(index), rows.get(index)) else { continue };
        let y = LIST_TOP + i as u16;
        queue!(buf, MoveTo(0, y))?;
        if index == cursor {
            queue!(buf, SetAttribute(Attribute::Reverse))?;
        }
        queue!(
            buf,
            SetForegroundColor(GREEN),
            Print(&line.mark),
            ResetColor,
        )?;
        if index == cursor {
            queue!(buf, SetAttribute(Attribute::Reverse))?;
        }
        if row.default {
            queue!(buf, SetAttribute(Attribute::Bold))?;
        }
        queue!(buf, Print(&line.name), SetAttribute(Attribute::NormalIntensity))?;
        queue!(
            buf,
            SetForegroundColor(MUTED),
            Print(&line.counts),
            SetForegroundColor(if line.missing { RED } else { STEM }),
            Print(&line.path),
            ResetColor,
            SetAttribute(Attribute::Reset),
        )?;
    }

    let footer_y = height.saturating_sub(1);
    queue!(buf, MoveTo(0, footer_y), SetForegroundColor(MUTED))?;
    match flash {
        Some(msg) => queue!(buf, SetForegroundColor(RED), Print(cut(msg, w)))?,
        None => queue!(buf, Print(legend(w)))?,
    }
    queue!(buf, ResetColor)?;

    out.write_all(&buf)?;
    out.flush()
}

/// Cut to `max` characters, ending in an ellipsis when something was lost.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    match max {
        0 => String::new(),
        1 => "…".into(),
        _ => format!("{}…", text.chars().take(max - 1).collect::<String>()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::Counts;
    use std::path::PathBuf;

    fn row(name: &str, default: bool, cards: Option<usize>) -> Summary {
        Summary {
            name: name.into(),
            path: PathBuf::from(format!("/home/someone/notes/a/rather/long/path/to/{name}")),
            default,
            counts: cards.map(|n| Counts { cards: n, active: n, newest: Some(2) }),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn the_cursor_starts_on_the_default_book() {
        let rows = [row("a", false, Some(1)), row("b", true, Some(2)), row("c", false, Some(0))];
        assert_eq!(start(&rows), 1);
        let none = [row("a", false, Some(1)), row("b", false, Some(2))];
        assert_eq!(start(&none), 0);
    }

    #[test]
    fn keys_move_within_bounds_and_choose() {
        let rows = [row("a", true, Some(1)), row("b", false, Some(2))];
        let mut cursor = 0;
        assert_eq!(on_key(&rows, &mut cursor, key(KeyCode::Up)), Step::Stay);
        assert_eq!(cursor, 0);
        on_key(&rows, &mut cursor, key(KeyCode::Down));
        on_key(&rows, &mut cursor, key(KeyCode::Char('j')));
        assert_eq!(cursor, 1, "the cursor stops at the last book");
        assert_eq!(on_key(&rows, &mut cursor, key(KeyCode::Enter)), Step::Done(Choice::List("b".into())));
        assert_eq!(on_key(&rows, &mut cursor, key(KeyCode::Char('p'))), Step::Done(Choice::Pick("b".into())));
        assert_eq!(on_key(&rows, &mut cursor, key(KeyCode::Char('q'))), Step::Done(Choice::Quit));
        assert_eq!(on_key(&rows, &mut cursor, key(KeyCode::Esc)), Step::Done(Choice::Quit));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(on_key(&rows, &mut cursor, ctrl_c), Step::Done(Choice::Quit));
    }

    #[test]
    fn no_row_overruns_the_terminal_whatever_its_width() {
        let rows = [
            row("docket", true, Some(14)),
            row("a-book-with-a-long-name", false, Some(3)),
            row("gone", false, None),
        ];
        for width in [10usize, 20, 30, 40, 60, 80, 120, 200] {
            for line in layout(&rows, width) {
                assert!(line.len() <= width, "width {width}: {line:?} is {}", line.len());
            }
        }
    }

    #[test]
    fn the_path_gives_way_before_the_counts() {
        let rows = [row("docket", true, Some(14))];
        let line = &layout(&rows, 50)[0];
        assert!(line.counts.contains("14 cards"), "{line:?}");
        assert!(line.path.ends_with('…'), "{line:?}");
    }

    #[test]
    fn a_missing_folder_says_so() {
        let rows = [row("gone", false, None)];
        let line = &layout(&rows, 200)[0];
        assert!(line.missing && line.path.contains("missing folder"), "{line:?}");
    }

    #[test]
    fn the_legend_fits_and_shrinks() {
        for width in [5usize, 10, 20, 40, 80] {
            assert!(legend(width).chars().count() <= width, "{width}");
        }
        assert!(legend(80).contains("enter open"));
    }
}
