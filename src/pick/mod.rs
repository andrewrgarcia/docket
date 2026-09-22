//! The picker: tick cards, watch the token cost, write the file.
//!
//! Same shape as `ygg pick`, because the job is the same one — choose what the
//! model sees, and see what it costs before you send it. Arrow keys move,
//! space ticks, enter writes.

use std::io::{self, IsTerminal, Stdout, Write};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};

use crate::card::Card;
use crate::error::{Error, Result};
use crate::theme::{self, BOLD, CYAN, DIM, GREEN, MAGENTA, RESET, REVERSE, YELLOW};

/// What the user decided.
#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    /// Write these cards, in the order the list showed them.
    Write(Vec<String>),
    Cancelled,
}

pub fn run(cards: &[Card]) -> Result<Choice> {
    if cards.is_empty() {
        return Err(Error::other("no cards yet — `dk add .` in a project"));
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::other("pick needs a terminal — `dk out` writes everything"));
    }

    let mut picker = Picker::new(cards);
    let screen = Screen::enter().map_err(terminal_error)?;
    let choice = picker.run();
    drop(screen);
    choice
}

fn terminal_error(e: io::Error) -> Error {
    Error::other(format!("terminal: {e}"))
}

struct Screen;

impl Screen {
    fn enter() -> io::Result<Screen> {
        terminal::enable_raw_mode()?;
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen, Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(e);
        }
        Ok(Screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

struct Row<'a> {
    card: &'a Card,
    ticked: bool,
}

struct Picker<'a> {
    rows: Vec<Row<'a>>,
    cursor: usize,
    top: usize,
}

impl<'a> Picker<'a> {
    fn new(cards: &'a [Card]) -> Picker<'a> {
        Picker {
            rows: cards.iter().map(|card| Row { card, ticked: false }).collect(),
            cursor: 0,
            top: 0,
        }
    }

    fn run(&mut self) -> Result<Choice> {
        let mut out = io::stdout();
        loop {
            let (width, height) = size();
            self.draw(&mut out, width, height).map_err(terminal_error)?;

            let Event::Key(key) = event::read().map_err(terminal_error)? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.rows.len() - 1,
                KeyCode::PageUp => self.move_by(-(visible_rows(height) as isize)),
                KeyCode::PageDown => self.move_by(visible_rows(height) as isize),
                KeyCode::Char(' ') | KeyCode::Char('x') => {
                    self.rows[self.cursor].ticked = !self.rows[self.cursor].ticked;
                }
                KeyCode::Char('a') => {
                    let all = self.rows.iter().all(|r| r.ticked);
                    self.rows.iter_mut().for_each(|r| r.ticked = !all);
                }
                KeyCode::Char('n') => self.rows.iter_mut().for_each(|r| r.ticked = false),
                KeyCode::Enter => return Ok(Choice::Write(self.ticked())),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(Choice::Cancelled),
                KeyCode::Char('c') if ctrl => return Ok(Choice::Cancelled),
                _ => {}
            }
        }
    }

    fn ticked(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|r| r.ticked)
            .map(|r| r.card.name.clone())
            .collect()
    }

    fn move_by(&mut self, delta: isize) {
        let last = self.rows.len() as isize - 1;
        let next = (self.cursor as isize + delta).clamp(0, last);
        self.cursor = next as usize;
    }

    fn draw(&mut self, out: &mut Stdout, width: usize, height: usize) -> io::Result<()> {
        let rows = visible_rows(height);
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + rows {
            self.top = self.cursor + 1 - rows;
        }

        queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
        queue!(
            out,
            Print(theme::paint_always(
                &fit(" DOCKET — pick what the model sees", width),
                &[REVERSE, BOLD, MAGENTA]
            ))
        )?;

        let ids: Vec<String> = self.rows.iter().map(|r| r.card.id.clone()).collect();
        let name_w = self
            .rows
            .iter()
            .map(|r| r.card.name.chars().count())
            .max()
            .unwrap_or(4);

        for screen in 0..rows {
            let index = self.top + screen;
            queue!(out, MoveTo(0, (screen + 1) as u16), Clear(ClearType::CurrentLine))?;
            let Some(row) = self.rows.get(index) else {
                continue;
            };

            let mark = if row.ticked { "[x]" } else { "[ ]" };
            let pointer = if index == self.cursor { ">" } else { " " };
            let line = format!(
                "{pointer} {mark} {:<8}  {:<name_w$}  {:>6}  {:>7}  {}",
                row.card.short_id(&ids),
                row.card.name,
                row.card.status.as_str(),
                format!("~{}t", row.card.tokens()),
                row.card.what,
            );
            let line = fit(&line, width);

            let codes: &[&str] = if index == self.cursor {
                &[REVERSE, BOLD]
            } else if row.ticked {
                &[GREEN]
            } else if row.card.status.is_cold() {
                &[DIM]
            } else {
                &[]
            };
            queue!(out, Print(theme::paint_always(&line, codes)))?;
        }

        let picked = self.rows.iter().filter(|r| r.ticked).count();
        let tokens: usize = self
            .rows
            .iter()
            .filter(|r| r.ticked)
            .map(|r| r.card.tokens())
            .sum();
        let summary = format!(
            " {picked} of {} picked   ~{tokens} tokens",
            self.rows.len()
        );
        queue!(
            out,
            MoveTo(0, (height - 2) as u16),
            Print(theme::paint_always(&fit(&summary, width), &[REVERSE, CYAN, BOLD])),
            MoveTo(0, (height - 1) as u16),
            Clear(ClearType::CurrentLine),
            Print(help(width.saturating_sub(1))),
        )?;
        out.flush()
    }
}

fn help(width: usize) -> String {
    let pairs = [
        ("↑↓", "move"),
        ("space", "tick"),
        ("a", "all"),
        ("n", "none"),
        ("enter", "write"),
        ("q", "cancel"),
    ];
    let plain: String = pairs
        .iter()
        .map(|(k, l)| format!(" {k} {l} "))
        .collect::<Vec<_>>()
        .join("·");
    if plain.chars().count() > width {
        return fit(&plain, width);
    }
    let painted: String = pairs
        .iter()
        .map(|(k, l)| {
            format!(
                " {} {} ",
                theme::paint_always(k, &[BOLD, YELLOW]),
                theme::paint_always(l, &[DIM])
            )
        })
        .collect::<Vec<_>>()
        .join(&theme::paint_always("·", &[DIM]));
    format!("{painted}{}{RESET}", " ".repeat(width - plain.chars().count()))
}

fn fit(text: &str, width: usize) -> String {
    let mut out: String = text.chars().take(width).collect();
    let used = out.chars().count();
    out.extend(std::iter::repeat(' ').take(width - used));
    out
}

fn size() -> (usize, usize) {
    let (w, h) = terminal::size().unwrap_or((80, 24));
    ((w as usize).max(20), (h as usize).max(5))
}

/// Rows for cards: everything but the title and the two bottom bars.
fn visible_rows(height: usize) -> usize {
    height.saturating_sub(3).max(1)
}
