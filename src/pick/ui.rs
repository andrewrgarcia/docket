//! The picker's terminal loop: draw rows, turn keys and clicks into folds,
//! selections and ticks.
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

use super::state::{Kind, Mark, Picker, Row};

/// Header, then a blank line.
const LIST_TOP: u16 = 2;
/// Blank, cost, keys.
const FOOTER_ROWS: u16 = 3;

const CYAN: Color = Color::Rgb { r: 0, g: 255, b: 255 };
const GREEN: Color = Color::Rgb { r: 120, g: 220, b: 120 };
const AMBER: Color = Color::Rgb { r: 255, g: 200, b: 50 };
const RED: Color = Color::Rgb { r: 255, g: 95, b: 95 };
const STEM: Color = Color::Rgb { r: 90, g: 90, b: 90 };
const MUTED: Color = Color::Rgb { r: 130, g: 130, b: 130 };

/// Context cost as a colour, on the same thresholds `ygg` uses.
fn heat(tokens: usize) -> Color {
    if tokens >= 4_000 {
        RED
    } else if tokens >= 1_000 {
        AMBER
    } else if tokens >= 200 {
        GREEN
    } else {
        MUTED
    }
}

fn human(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub enum Outcome {
    Quit,
    /// `w` — write the selection.
    Write,
    /// `e` — edit this card, then come back.
    Edit(String),
}

/// Restores the terminal even if the loop returns early — a picker that leaves
/// the shell in raw mode is worse than no picker.
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

/// `on_tick` receives cards a keystroke changed, so a ticked box reaches disk
/// at once rather than at exit.
pub fn run(
    picker: &mut Picker,
    on_tick: &mut dyn FnMut(&[(String, String)]) -> Result<()>,
) -> Result<Outcome> {
    let _guard = TermGuard::enter().map_err(term)?;
    let mut out = io::stdout();
    let mut flash: Option<String> = None;

    loop {
        let rows = picker.rows();
        if picker.cursor >= rows.len() && !rows.is_empty() {
            picker.cursor = rows.len() - 1;
        }
        draw(picker, &rows, flash.as_deref(), &mut out).map_err(term)?;

        let verdict = match event::read().map_err(term)? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                flash = None;
                on_key(picker, &rows, key, &mut flash)
            }
            Event::Mouse(m) => {
                flash = None;
                on_mouse(picker, &rows, m);
                Verdict::Continue
            }
            _ => Verdict::Continue,
        };

        let changed = picker.take_dirty();
        if !changed.is_empty() {
            on_tick(&changed)?;
        }

        match verdict {
            Verdict::Continue => {}
            Verdict::Quit => return Ok(Outcome::Quit),
            Verdict::Edit => {
                if let Some(row) = rows.get(picker.cursor) {
                    return Ok(Outcome::Edit(picker.card(row.card).name.clone()));
                }
            }
            Verdict::Write => {
                if picker.selection().is_empty() {
                    flash = Some("nothing picked — space takes a card or a section".into());
                    continue;
                }
                return Ok(Outcome::Write);
            }
        }
    }
}

fn term(e: io::Error) -> Error {
    Error::other(format!("terminal: {e}"))
}

enum Verdict {
    Continue,
    Quit,
    Write,
    Edit,
}

fn on_key(picker: &mut Picker, rows: &[Row], key: KeyEvent, flash: &mut Option<String>) -> Verdict {
    let current = rows.get(picker.cursor).cloned();

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Verdict::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Verdict::Quit,
        KeyCode::Char('w') => return Verdict::Write,
        KeyCode::Char('e') => return Verdict::Edit,

        KeyCode::Up | KeyCode::Char('k') => picker.cursor = picker.cursor.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            if picker.cursor + 1 < rows.len() {
                picker.cursor += 1;
            }
        }
        KeyCode::Home => picker.cursor = 0,
        KeyCode::End => picker.cursor = rows.len().saturating_sub(1),

        KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
            if let Some(row) = &current {
                picker.toggle_fold(row);
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(row) = &current {
                if row.expanded || matches!(row.kind, Kind::Line { .. }) {
                    picker.fold(row, false);
                } else if let Some(parent) = parent_of(rows, picker.cursor) {
                    picker.cursor = parent;
                }
            }
        }

        // Space picks. Ticking a checkbox is `t`, because choosing what to
        // send and marking work done are different acts and sharing a key
        // would make one of them a surprise.
        KeyCode::Char(' ') => {
            if let Some(row) = &current {
                picker.toggle_select(row);
            }
        }
        KeyCode::Char('t') => match &current {
            Some(row) if picker.tick(row).is_some() => {}
            _ => *flash = Some("no checkbox on that line".into()),
        },

        KeyCode::Char('a') => picker.select_all(),
        KeyCode::Char('n') => picker.select_none(),
        KeyCode::Char('N') => picker.select_heading("now"),
        KeyCode::Char('X') => picker.select_heading("next"),
        KeyCode::Char('R') => picker.select_heading("readme"),

        KeyCode::Char('*') => picker.toggle_all_folds(),
        KeyCode::Char('o') => picker.expand_cards(),
        KeyCode::Char('z') => picker.collapse_all(),
        _ => {}
    }
    Verdict::Continue
}

/// The row that owns the one at `index`.
fn parent_of(rows: &[Row], index: usize) -> Option<usize> {
    let row = rows.get(index)?;
    let want = match row.kind {
        Kind::Card => return None,
        Kind::Node { .. } => Kind::Card,
        Kind::Line { node, .. } => Kind::Node { index: node },
    };
    rows[..index]
        .iter()
        .rposition(|r| r.card == row.card && r.kind == want)
}

fn on_mouse(picker: &mut Picker, rows: &[Row], m: MouseEvent) {
    let hit = |picker: &Picker, m: &MouseEvent| {
        (m.row >= LIST_TOP)
            .then(|| picker.scroll + (m.row - LIST_TOP) as usize)
            .filter(|i| *i < rows.len())
    };
    match m.kind {
        // Left click on the box column picks; anywhere else folds.
        MouseEventKind::Down(MouseButton::Left) => {
            let Some(index) = hit(picker, &m) else { return };
            picker.cursor = index;
            let row = rows[index].clone();
            if m.column <= 4 {
                picker.toggle_select(&row);
            } else {
                picker.toggle_fold(&row);
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            let Some(index) = hit(picker, &m) else { return };
            picker.cursor = index;
            picker.toggle_select(&rows[index].clone());
        }
        MouseEventKind::ScrollUp => picker.cursor = picker.cursor.saturating_sub(1),
        MouseEventKind::ScrollDown => {
            if picker.cursor + 1 < rows.len() {
                picker.cursor += 1;
            }
        }
        _ => {}
    }
}

fn draw(picker: &mut Picker, rows: &[Row], flash: Option<&str>, out: &mut impl Write) -> io::Result<()> {
    let (width, height) = terminal::size().unwrap_or((80, 24));
    let list_height = height.saturating_sub(LIST_TOP + FOOTER_ROWS).max(1) as usize;

    if picker.cursor < picker.scroll {
        picker.scroll = picker.cursor;
    } else if picker.cursor >= picker.scroll + list_height {
        picker.scroll = picker.cursor + 1 - list_height;
    }

    let mut buf: Vec<u8> = Vec::with_capacity(16 * 1024);
    let final_out = out;
    let out = &mut buf;

    // ── header ──────────────────────────────────────────────────────────
    let (cards, sections, ticked, boxes) = picker.totals();
    queue!(
        out,
        MoveTo(0, 0),
        Clear(ClearType::UntilNewLine),
        SetForegroundColor(Color::Magenta),
        SetAttribute(Attribute::Bold),
        Print("▦ dk pick"),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print("  "),
        SetForegroundColor(MUTED),
        Print(format!("{cards} cards · {sections} headings · ☑ {ticked}/{boxes}")),
        ResetColor,
        MoveTo(0, 1),
        Clear(ClearType::UntilNewLine),
    )?;

    // ── rows ────────────────────────────────────────────────────────────
    let name_width = rows
        .iter()
        .map(|r| r.prefix.chars().count() + r.text.chars().count() + 2)
        .max()
        .unwrap_or(0)
        .min(width as usize / 2);

    for (line, index) in (picker.scroll..rows.len().min(picker.scroll + list_height)).enumerate() {
        let row = &rows[index];
        let y = LIST_TOP + line as u16;
        queue!(out, MoveTo(0, y), Clear(ClearType::UntilNewLine))?;

        let is_cursor = index == picker.cursor;
        let (box_text, box_color) = match row.mark {
            Mark::All => ("[x]", GREEN),
            Mark::Partial => ("[~]", AMBER),
            Mark::None => ("[ ]", MUTED),
        };

        // Everything left of the label is fixed width, so the label gets
        // whatever is left. A row that overruns the terminal wraps, and a
        // wrapped row scrolls the ones below it off the screen — which is how
        // a README's 200-character HTML line corrupts the whole frame.
        let gutter = 2 + 4 + row.prefix.chars().count() + 2;
        let reserve = if matches!(row.kind, Kind::Line { .. }) { 1 } else { 14 };
        let room = (width as usize).saturating_sub(gutter + reserve);

        queue!(
            out,
            Print(if is_cursor { "▶ " } else { "  " }),
            SetForegroundColor(box_color),
            Print(box_text),
            ResetColor,
            Print(" "),
            SetForegroundColor(STEM),
            Print(&row.prefix),
            ResetColor,
        )?;

        // Characters actually drawn after the gutter, so the token column
        // pads from what is on screen rather than from what was asked for.
        let drawn;

        match row.kind {
            Kind::Card => {
                let card = picker.card(row.card);
                let (filled, total) = card.completion();
                let tail = format!("  {}  {filled}/{total}", &card.id[..4.min(card.id.len())]);
                let name = cut(&row.text, room.saturating_sub(tail.chars().count()));
                drawn = name.chars().count() + tail.chars().count();
                queue!(
                    out,
                    SetForegroundColor(CYAN),
                    SetAttribute(Attribute::Bold),
                    Print(marker(row)),
                    Print(name),
                    SetAttribute(Attribute::Reset),
                    ResetColor,
                    SetForegroundColor(MUTED),
                    Print(tail),
                    ResetColor,
                )?;
            }
            // A card's own section is amber; anything nested inside one came
            // from a README and is drawn cooler, so the two never blur.
            Kind::Node { .. } => {
                let title = cut(&row.text, room);
                drawn = title.chars().count();
                queue!(
                    out,
                    SetForegroundColor(if row.depth > 0 { MUTED } else { AMBER }),
                    Print(marker(row)),
                    Print(title),
                    ResetColor,
                )?
            }
            Kind::Line { .. } => {
                drawn = 0; // lines carry no token column
                match checkbox::find(&row.text) {
                    Some(mark) => queue!(
                        out,
                        SetForegroundColor(if mark.done { MUTED } else { GREEN }),
                        Print(if mark.done { "✓ " } else { "□ " }),
                        Print(cut(&strip_box(&row.text), room)),
                        ResetColor,
                    )?,
                    None => queue!(
                        out,
                        SetForegroundColor(MUTED),
                        Print("  "),
                        Print(cut(row.text.trim_start(), room)),
                        ResetColor,
                    )?,
                }
            }
        }

        // token column, aligned past the longest name
        if !matches!(row.kind, Kind::Line { .. }) {
            let used = row.prefix.chars().count() + drawn + 2;
            let pad = name_width.saturating_sub(used) + 2;
            let tok = human(row.tokens);
            if gutter + drawn + pad + 12 < width as usize {
                queue!(
                    out,
                    Print(" ".repeat(pad + 7usize.saturating_sub(tok.chars().count()))),
                    SetForegroundColor(heat(row.tokens)),
                    Print(tok),
                    ResetColor,
                    SetForegroundColor(MUTED),
                    Print(" tok"),
                    ResetColor,
                )?;
            }
        }
    }

    // Rows the list no longer fills after a collapse would keep stale text.
    let drawn = rows.len().saturating_sub(picker.scroll).min(list_height);
    for line in drawn..list_height {
        queue!(out, MoveTo(0, LIST_TOP + line as u16), Clear(ClearType::UntilNewLine))?;
    }

    // ── footer ──────────────────────────────────────────────────────────
    let (picked_cards, picked_sections, tokens) = picker.cost();
    queue!(
        out,
        MoveTo(0, height.saturating_sub(2)),
        Clear(ClearType::UntilNewLine),
        SetAttribute(Attribute::Bold),
        SetForegroundColor(heat(tokens)),
        Print(format!(
            "▦ {picked_sections} headings from {picked_cards} cards · {} tok",
            human(tokens)
        )),
        ResetColor,
        SetAttribute(Attribute::Reset),
        SetForegroundColor(MUTED),
        Print("  → DOCKET.md"),
        ResetColor,
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
                "space pick · enter/→ open · ← close · o cards · * all · t tick · N now · R readme · a all · n none · e edit · w write · q quit",
                width.saturating_sub(1) as usize
            )),
            ResetColor
        )?,
    }

    final_out.write_all(&buf)?;
    final_out.flush()
}

/// The fold marker for a row: open, shut, or nothing to open.
fn marker(row: &Row) -> &'static str {
    if !row.foldable {
        "· "
    } else if row.expanded {
        "▾ "
    } else {
        "▸ "
    }
}

/// Cut to `max` characters, ending in an ellipsis when something was lost.
/// Counts characters, never bytes.
fn cut(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if text.chars().count() <= max {
        return text.to_string();
    }
    if max == 1 {
        return "…".into();
    }
    let kept: String = text.chars().take(max - 1).collect();
    format!("{}…", kept.trim_end())
}

/// The text of a checkbox line without its `- [ ] ` marker.
fn strip_box(line: &str) -> String {
    let after = line.trim_start().trim_start_matches("- ").trim_start_matches("* ");
    after
        .get(3..)
        .map(|rest| rest.trim_start().to_string())
        .unwrap_or_else(|| after.to_string())
}

fn fit(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}
