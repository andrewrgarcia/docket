//! The tree's terminal loop: draw rows, turn keys and clicks into folds.
//!
//! One frame, one write. Painting straight to stdout across many small queued
//! calls lets the terminal present a half-drawn tree, so the frame is built in
//! a buffer and flushed once.

use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{
        self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

use crate::checkbox;
use crate::error::{Error, Result};

use super::state::{Kind, Row, Tree};

/// First tree row: the header, then a blank line.
const LIST_TOP: u16 = 2;
/// Blank, summary, keys.
const FOOTER_ROWS: u16 = 3;

const CYAN: Color = Color::Rgb { r: 0, g: 255, b: 255 };
const GREEN: Color = Color::Rgb { r: 120, g: 220, b: 120 };
const AMBER: Color = Color::Rgb { r: 255, g: 200, b: 50 };
const STEM: Color = Color::Rgb { r: 90, g: 90, b: 90 };
const MUTED: Color = Color::Rgb { r: 130, g: 130, b: 130 };

/// How full a card is, as a colour: empty is grey, half is amber, done green.
fn fill_color(done: usize, total: usize) -> Color {
    if total == 0 || done == 0 {
        MUTED
    } else if done == total {
        GREEN
    } else {
        AMBER
    }
}

/// What the user asked for on the way out.
pub enum Outcome {
    Quit,
    /// `e` — edit this card, then come back.
    Edit(String),
}

/// Restores the terminal even if the loop returns early — a browser that
/// leaves the shell in raw mode is worse than no browser.
struct TermGuard;

impl TermGuard {
    fn enter() -> io::Result<TermGuard> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture, Hide)?;
        Ok(TermGuard)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, DisableMouseCapture, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

/// `on_tick` is called with the cards a keystroke changed, so they reach disk
/// the moment they are ticked rather than at exit.
pub fn run(tree: &mut Tree, on_tick: &mut dyn FnMut(&[(String, String)]) -> Result<()>) -> Result<Outcome> {
    let _guard = TermGuard::enter().map_err(term)?;
    let mut out = io::stdout();
    let mut flash: Option<String> = None;

    loop {
        let rows = tree.rows();
        if tree.cursor >= rows.len() && !rows.is_empty() {
            tree.cursor = rows.len() - 1;
        }
        draw(tree, &rows, flash.as_deref(), &mut out).map_err(term)?;

        match event::read().map_err(term)? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                flash = None;
                match on_key(tree, &rows, key, &mut flash) {
                    Verdict::Continue => {}
                    Verdict::Quit => return Ok(Outcome::Quit),
                    Verdict::Edit => {
                        if let Some(row) = rows.get(tree.cursor) {
                            return Ok(Outcome::Edit(tree.card(row.card).name.clone()));
                        }
                    }
                }
                let changed = tree.take_dirty();
                if !changed.is_empty() {
                    on_tick(&changed)?;
                }
            }
            Event::Mouse(m) => {
                flash = None;
                on_mouse(tree, &rows, m);
                let changed = tree.take_dirty();
                if !changed.is_empty() {
                    on_tick(&changed)?;
                }
            }
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

fn on_key(tree: &mut Tree, rows: &[Row], key: KeyEvent, flash: &mut Option<String>) -> Verdict {
    let current = rows.get(tree.cursor).cloned();

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Verdict::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Verdict::Quit,
        KeyCode::Char('e') => return Verdict::Edit,

        KeyCode::Up | KeyCode::Char('k') => tree.cursor = tree.cursor.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            if tree.cursor + 1 < rows.len() {
                tree.cursor += 1;
            }
        }
        KeyCode::Home => tree.cursor = 0,
        KeyCode::End => tree.cursor = rows.len().saturating_sub(1),

        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(row) = &current {
                tree.fold(row, true);
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(row) = &current {
                // A shut row jumps to its parent rather than doing nothing.
                if row.expanded || matches!(row.kind, Kind::Line { .. }) {
                    tree.fold(row, false);
                } else if let Some(parent) = parent_of(rows, tree.cursor) {
                    tree.cursor = parent;
                }
            }
        }
        KeyCode::Enter => {
            if let Some(row) = &current {
                match row.kind {
                    Kind::Line { .. } => {
                        if tree.tick(row).is_none() {
                            *flash = Some("no checkbox on that line".into());
                        }
                    }
                    _ => tree.toggle_fold(row),
                }
            }
        }
        KeyCode::Char(' ') => {
            if let Some(row) = &current {
                if tree.tick(row).is_none() {
                    tree.toggle_fold(row);
                }
            }
        }
        KeyCode::Char('*') => tree.toggle_all(),
        KeyCode::Char('o') => tree.expand_cards(),
        KeyCode::Char('z') => tree.collapse_all(),
        _ => {}
    }
    Verdict::Continue
}

/// The row that owns the one at `index`.
fn parent_of(rows: &[Row], index: usize) -> Option<usize> {
    let row = rows.get(index)?;
    let want = match row.kind {
        Kind::Card => return None,
        Kind::Section { .. } => Kind::Card,
        Kind::Line { section, .. } => Kind::Section { index: section },
    };
    rows[..index]
        .iter()
        .rposition(|r| r.card == row.card && r.kind == want)
}

fn on_mouse(tree: &mut Tree, rows: &[Row], m: MouseEvent) {
    let hit = |tree: &Tree, m: &MouseEvent| {
        (m.row >= LIST_TOP)
            .then(|| tree.scroll + (m.row - LIST_TOP) as usize)
            .filter(|i| *i < rows.len())
    };
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let Some(index) = hit(tree, &m) else { return };
            tree.cursor = index;
            let row = rows[index].clone();
            // Click mirrors Enter: folds open, checkboxes tick.
            if matches!(row.kind, Kind::Line { .. }) {
                tree.tick(&row);
            } else {
                tree.toggle_fold(&row);
            }
        }
        MouseEventKind::ScrollUp => tree.cursor = tree.cursor.saturating_sub(1),
        MouseEventKind::ScrollDown => {
            if tree.cursor + 1 < rows.len() {
                tree.cursor += 1;
            }
        }
        _ => {}
    }
}

fn draw(tree: &mut Tree, rows: &[Row], flash: Option<&str>, out: &mut impl Write) -> io::Result<()> {
    let (width, height) = terminal::size().unwrap_or((80, 24));
    let list_height = height.saturating_sub(LIST_TOP + FOOTER_ROWS).max(1) as usize;

    if tree.cursor < tree.scroll {
        tree.scroll = tree.cursor;
    } else if tree.cursor >= tree.scroll + list_height {
        tree.scroll = tree.cursor + 1 - list_height;
    }

    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let final_out = out;
    let out = &mut buf;

    // ── header ──────────────────────────────────────────────────────────
    let (cards, sections, done, total) = tree.summary();
    queue!(
        out,
        MoveTo(0, 0),
        Clear(ClearType::UntilNewLine),
        SetForegroundColor(Color::Magenta),
        SetAttribute(Attribute::Bold),
        Print("▦ dk tree"),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print("  "),
        SetForegroundColor(MUTED),
        Print(format!("{cards} cards · {sections} sections")),
        ResetColor,
    )?;
    queue!(out, MoveTo(0, 1), Clear(ClearType::UntilNewLine))?;

    // ── rows ────────────────────────────────────────────────────────────
    for (line, index) in (tree.scroll..rows.len().min(tree.scroll + list_height)).enumerate() {
        let row = &rows[index];
        let y = LIST_TOP + line as u16;
        queue!(out, MoveTo(0, y), Clear(ClearType::UntilNewLine))?;

        let is_cursor = index == tree.cursor;
        queue!(out, Print(if is_cursor { "▶ " } else { "  " }))?;
        queue!(out, SetForegroundColor(STEM), Print(&row.prefix), ResetColor)?;

        match row.kind {
            Kind::Card => {
                let card = tree.card(row.card);
                let (filled, sections) = card.completion();
                queue!(
                    out,
                    SetForegroundColor(CYAN),
                    SetAttribute(Attribute::Bold),
                    Print(if row.expanded { "▾ " } else { "▸ " }),
                    Print(&row.text),
                    SetAttribute(Attribute::Reset),
                    ResetColor,
                    SetForegroundColor(MUTED),
                    Print(format!("  {}", &card.id[..4.min(card.id.len())])),
                    ResetColor,
                    SetForegroundColor(fill_color(filled, sections)),
                    Print(format!("  {filled}/{sections}")),
                    ResetColor,
                )?;
            }
            Kind::Section { .. } => {
                queue!(
                    out,
                    SetForegroundColor(AMBER),
                    Print(if row.expanded { "▾ " } else { "▸ " }),
                    Print(&row.text),
                    ResetColor,
                )?;
            }
            Kind::Line { .. } => match checkbox::find(&row.text) {
                Some(box_) => {
                    let text = row.text.trim_start();
                    queue!(
                        out,
                        SetForegroundColor(if box_.done { MUTED } else { GREEN }),
                        Print(if box_.done { "✓ " } else { "□ " }),
                        Print(strip_box(text)),
                        ResetColor,
                    )?;
                }
                None => {
                    queue!(out, SetForegroundColor(MUTED), Print("  "), Print(row.text.trim_start()), ResetColor)?;
                }
            },
        }
    }

    // Rows the list no longer fills after a collapse would keep stale text.
    let drawn = rows.len().saturating_sub(tree.scroll).min(list_height);
    for line in drawn..list_height {
        queue!(out, MoveTo(0, LIST_TOP + line as u16), Clear(ClearType::UntilNewLine))?;
    }

    // ── footer ──────────────────────────────────────────────────────────
    queue!(
        out,
        MoveTo(0, height.saturating_sub(2)),
        Clear(ClearType::UntilNewLine),
        SetAttribute(Attribute::Bold),
        SetForegroundColor(fill_color(done, total)),
        Print(if total == 0 {
            "no checkboxes yet".to_string()
        } else {
            format!("☑ {done}/{total} ticked")
        }),
        ResetColor,
        SetAttribute(Attribute::Reset),
        MoveTo(0, height.saturating_sub(1)),
        Clear(ClearType::UntilNewLine),
    )?;

    match flash {
        Some(message) => queue!(
            out,
            SetForegroundColor(AMBER),
            SetAttribute(Attribute::Bold),
            Print(format!("⚠  {message}")),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?,
        None => queue!(
            out,
            SetForegroundColor(MUTED),
            Print(fit(
                "enter/click open · space tick · → open · ← close · o cards · * all · z none · e edit · q quit",
                width as usize
            )),
            ResetColor
        )?,
    }

    final_out.write_all(&buf)?;
    final_out.flush()
}

/// The text of a checkbox line without its `- [ ] ` marker.
fn strip_box(line: &str) -> String {
    let after = line
        .trim_start()
        .trim_start_matches("- ")
        .trim_start_matches("* ");
    after
        .get(3..)
        .map(|rest| rest.trim_start().to_string())
        .unwrap_or_else(|| after.to_string())
}

fn fit(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}
