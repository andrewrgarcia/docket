//! The fold view's terminal loop.
//!
//! Same discipline as the tree: one frame built in a buffer, one write. Long
//! lines wrap rather than clip — this is for reading, and a card's `what:`
//! line is routinely wider than the terminal. The mouse is left to the
//! terminal so text can still be selected and copied; most terminals turn the
//! wheel into arrow keys on the alternate screen, which moves the cursor.

use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::editor::syntax::Kind;
use crate::error::{Error, Result};
use crate::theme::{paint_always, BOLD, CYAN, DIM, GREY};

use super::state::View;

/// Cursor bar, fold mark, space.
const GUTTER: usize = 3;
/// Blank, keys.
const FOOTER_ROWS: u16 = 2;

pub enum Outcome {
    Quit,
    /// `e` — edit the card, then come back to it.
    Edit,
}

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

pub fn run(view: &mut View, title: &str) -> Result<Outcome> {
    let _guard = TermGuard::enter().map_err(term)?;
    let mut out = io::stdout();

    loop {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        let page = height.saturating_sub(FOOTER_ROWS).max(1) as usize;
        draw(view, title, width as usize, page, &mut out).map_err(term)?;

        match event::read().map_err(term)? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match on_key(view, key, page) {
                Verdict::Continue => {}
                Verdict::Quit => return Ok(Outcome::Quit),
                Verdict::Edit => return Ok(Outcome::Edit),
            },
            _ => {}
        }
    }
}

fn term(e: io::Error) -> Error {
    Error::other(format!("terminal: {e}"))
}

enum Verdict {
    Continue,
    Quit,
    Edit,
}

fn on_key(view: &mut View, key: KeyEvent, page: usize) -> Verdict {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Verdict::Quit,
        KeyCode::Char('c') if ctrl => return Verdict::Quit,
        KeyCode::Char('e') => return Verdict::Edit,

        KeyCode::Down | KeyCode::Char('j') => view.down(),
        KeyCode::Up | KeyCode::Char('k') => view.up(),
        KeyCode::Char('d') if ctrl => view.by(page / 2, true),
        KeyCode::Char('u') if ctrl => view.by(page / 2, false),
        KeyCode::PageDown | KeyCode::Char(' ') => view.by(page.saturating_sub(1).max(1), true),
        KeyCode::PageUp | KeyCode::Char('b') => view.by(page.saturating_sub(1).max(1), false),
        KeyCode::Home | KeyCode::Char('g') => view.home(),
        KeyCode::End | KeyCode::Char('G') => view.end(),
        KeyCode::Tab | KeyCode::Char('n') => view.next_heading(true),
        KeyCode::BackTab | KeyCode::Char('N') => view.next_heading(false),

        KeyCode::Enter => view.toggle(),
        KeyCode::Right | KeyCode::Char('l') => view.open(),
        KeyCode::Left | KeyCode::Char('h') => view.close(),
        KeyCode::Char('o') => view.open_all(),
        KeyCode::Char('z') => view.close_all(),
        _ => {}
    }
    Verdict::Continue
}

/// A line cut into pieces no wider than `width` characters, at a space when
/// there is one to break at, mid-word only when a word is wider than the line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rest: Vec<char> = text.replace('\t', "    ").chars().collect();
    let mut out = Vec::new();
    while rest.len() > width {
        let cut = match rest[..=width].iter().rposition(|c| *c == ' ') {
            Some(space) if space > 0 => space,
            _ => width,
        };
        out.push(rest[..cut].iter().collect::<String>().trim_end().to_string());
        let skip = if rest.get(cut) == Some(&' ') { cut + 1 } else { cut };
        rest.drain(..skip);
    }
    out.push(rest.into_iter().collect());
    out
}

fn draw(view: &mut View, title: &str, width: usize, page: usize, out: &mut impl Write) -> io::Result<()> {
    let text_w = width.saturating_sub(GUTTER).max(1);

    // Every visible line as display rows: (line, first row of that line).
    let visible = view.visible();
    let mut rows: Vec<(usize, usize, String)> = Vec::new(); // (line, piece, text)
    let mut cursor_rows = (0, 0);
    for &line in &visible {
        let pieces = wrap(&view.lines[line].text, text_w);
        if line == view.cursor {
            cursor_rows = (rows.len(), rows.len() + pieces.len());
        }
        rows.extend(pieces.into_iter().enumerate().map(|(i, p)| (line, i, p)));
    }

    // Keep the whole cursor line on screen when it fits.
    if cursor_rows.0 < view.scroll {
        view.scroll = cursor_rows.0;
    } else if cursor_rows.1 > view.scroll + page {
        view.scroll = cursor_rows.1.saturating_sub(page).min(cursor_rows.0);
    }

    let mut buf: Vec<u8> = Vec::with_capacity(16 * 1024);
    for y in 0..page {
        queue!(buf, MoveTo(0, y as u16), Clear(ClearType::UntilNewLine))?;
        let Some((line, piece, text)) = rows.get(view.scroll + y) else { continue };
        let (line, piece) = (*line, *piece);
        let entry = &view.lines[line];

        let bar = if line == view.cursor { paint_always("▌", &[CYAN]) } else { " ".into() };
        let mark = match (piece, entry.depth.is_some(), view.is_folded(line)) {
            (0, true, true) => paint_always("▸", &[CYAN]),
            (0, true, false) => paint_always("▾", &[DIM]),
            _ => " ".into(),
        };
        let body = match entry.kind {
            Kind::Body => text.clone(),
            kind => paint_always(text, kind.codes()),
        };
        queue!(buf, Print(bar), Print(mark), Print(" "), Print(body))?;

        // A shut heading says how much it hides, on its last row, if it fits.
        let last_piece = rows.get(view.scroll + y + 1).map_or(true, |(l, _, _)| *l != line);
        if last_piece && view.is_folded(line) {
            let hint = format!("  … {} lines", view.hidden(line));
            if text.chars().count() + hint.chars().count() <= text_w {
                queue!(buf, Print(paint_always(&hint, &[GREY])))?;
            }
        }
    }

    // ── footer ──────────────────────────────────────────────────────────
    let keys = "⏎ fold · ←/→ close/open · tab next heading · z outline · o all · e edit · q quit";
    let place = format!(
        "{title}  {}%",
        if rows.len() <= page { 100 } else { ((view.scroll + page).min(rows.len()) * 100) / rows.len() }
    );
    let room = width.saturating_sub(place.chars().count() + 2);
    let keys: String = keys.chars().take(room).collect();
    let pad = width.saturating_sub(keys.chars().count() + place.chars().count());
    queue!(
        buf,
        MoveTo(0, page as u16),
        Clear(ClearType::UntilNewLine),
        MoveTo(0, page as u16 + 1),
        Clear(ClearType::UntilNewLine),
        Print(paint_always(&keys, &[DIM])),
        Print(" ".repeat(pad)),
        Print(paint_always(&place, &[DIM, BOLD])),
    )?;

    out.write_all(&buf)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wrapping_counts_characters_and_keeps_empty_lines() {
        assert_eq!(wrap("", 4), vec![""]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("añañ", 2), vec!["añ", "añ"]);
        assert_eq!(wrap("abc", 3), vec!["abc"]);
    }

    #[test]
    fn wrapping_breaks_at_spaces() {
        assert_eq!(wrap("the seminar was given", 10), vec!["the", "seminar", "was given"]);
        assert_eq!(wrap("one two three", 8), vec!["one two", "three"]);
        assert_eq!(wrap("  indented line here", 10), vec!["  indented", "line here"]);
    }
}
