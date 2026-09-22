//! docket's built-in editor.
//!
//! It exists so `dk edit` behaves the same on every machine and never depends
//! on which editor, or which build of it, happens to be installed.
//!
//! It is modal, like vi, because the alternative is binding every action to a
//! control chord and control chords are what people cannot remember. There are
//! three modes and the bottom bar always says which one you are in and what
//! the keys do in it:
//!
//! - **command** — where you land. Arrows move. `e` starts typing. `:` opens
//!   the command line. Space ticks a `[ ]` box, Tab hops to the next one.
//! - **insert** — you type, text appears. `Esc` goes back.
//! - **`:` line** — `:w` `:q` `:q!` `:wq` `:x`, as in vi.
//!
//! The split is strict: `buffer` owns the text and cursor, `view` owns the
//! wrapping arithmetic, and neither touches a terminal, so both are tested
//! directly. This file is the only part that talks to the screen.

mod buffer;
pub mod syntax;
mod view;

use std::fs;
use std::io::{self, IsTerminal, Stdout, Write};
use std::path::Path;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::style::Print;
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};

use crate::error::{Error, Result};
use crate::theme::{self, BOLD, CYAN, DIM, GREEN, RED, RESET, REVERSE, YELLOW};
use buffer::Buffer;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Saved,
    Unchanged,
    Discarded,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Mode {
    Command,
    Insert,
}

/// Open `path` for editing and block until the user quits.
pub fn run(path: &Path) -> Result<Outcome> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::other(
            "edit needs a terminal — or set $EDITOR to use your own editor",
        ));
    }
    let text = fs::read_to_string(path).map_err(|e| Error::io("read", path, e))?;

    let mut session = Session::new(path, &text);
    let screen = Screen::enter().map_err(terminal_error)?;
    let outcome = session.run();
    drop(screen); // restore the terminal before any error is printed
    outcome
}

fn terminal_error(e: io::Error) -> Error {
    Error::other(format!("terminal: {e}"))
}

/// Raw mode and the alternate screen, undone on drop — including when the loop
/// returns early with an error. The release profile must not set
/// `panic = "abort"`, or a panic would skip this and leave the shell unusable.
struct Screen;

impl Screen {
    fn enter() -> io::Result<Screen> {
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        if let Err(e) = execute!(out, EnterAlternateScreen) {
            let _ = terminal::disable_raw_mode();
            return Err(e);
        }
        // Not every console supports bracketed paste. Without it a paste
        // arrives as ordinary keystrokes, which still works.
        let _ = execute!(out, EnableBracketedPaste);
        Ok(Screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = execute!(out, DisableBracketedPaste);
        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

struct Session<'a> {
    path: &'a Path,
    name: String,
    buffer: Buffer,
    mode: Mode,
    /// `Some` while the `:` line is open, holding what has been typed.
    command: Option<String>,
    top: usize,
    message: String,
    message_color: &'static str,
    saved_once: bool,
}

impl<'a> Session<'a> {
    fn new(path: &'a Path, text: &str) -> Session<'a> {
        let mut buffer = Buffer::from_text(text);
        buffer.jump_below("## now");
        Session {
            path,
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("card")
                .to_string(),
            buffer,
            mode: Mode::Command,
            command: None,
            top: 0,
            message: String::new(),
            message_color: DIM,
            saved_once: false,
        }
    }

    fn run(&mut self) -> Result<Outcome> {
        let mut out = io::stdout();
        loop {
            let (width, height) = size();
            self.draw(&mut out, width, height).map_err(terminal_error)?;

            match event::read().map_err(terminal_error)? {
                // Windows reports key releases as well; acting on them would
                // double every keystroke.
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if let Some(outcome) = self.key(key, width, height)? {
                        return Ok(outcome);
                    }
                }
                Event::Paste(text) => {
                    self.buffer.insert_text(&text);
                    self.mode = Mode::Insert;
                    self.note("pasted", GREEN);
                }
                _ => {}
            }
        }
    }

    /// `Some` means the session is over.
    fn key(&mut self, key: KeyEvent, width: usize, height: usize) -> Result<Option<Outcome>> {
        if self.command.is_some() {
            return self.command_line_key(key);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        // AltGr arrives as Ctrl+Alt on Windows and must still type its symbol.
        let chord = ctrl && !alt;

        // Movement works identically in both modes, so it is handled once.
        match key.code {
            KeyCode::Left => return self.moved(|b| b.left()),
            KeyCode::Right => return self.moved(|b| b.right()),
            KeyCode::Up => return self.moved(|b| b.up(width)),
            KeyCode::Down => return self.moved(|b| b.down(width)),
            KeyCode::Home if ctrl => return self.moved(|b| b.top()),
            KeyCode::End if ctrl => return self.moved(|b| b.bottom()),
            KeyCode::Home => return self.moved(|b| b.home()),
            KeyCode::End => return self.moved(|b| b.end()),
            KeyCode::PageUp => return self.moved(|b| b.page_up(text_rows(height), width)),
            KeyCode::PageDown => return self.moved(|b| b.page_down(text_rows(height), width)),
            _ => {}
        }

        match self.mode {
            Mode::Command => self.command_mode_key(key, chord),
            Mode::Insert => self.insert_mode_key(key, chord),
        }
    }

    fn moved(&mut self, action: impl FnOnce(&mut Buffer)) -> Result<Option<Outcome>> {
        action(&mut self.buffer);
        Ok(None)
    }

    fn command_mode_key(&mut self, key: KeyEvent, chord: bool) -> Result<Option<Outcome>> {
        match key.code {
            KeyCode::Char('e') if chord => self.insert("typing — Esc returns to command keys"),
            KeyCode::Char(c) if chord => {
                if c.eq_ignore_ascii_case(&'s') {
                    self.save()?;
                }
            }
            // `e` to edit, `i` because vi users will press it anyway.
            KeyCode::Char('e') | KeyCode::Char('i') => {
                self.insert("typing — Esc returns to command keys")
            }
            KeyCode::Char(':') => {
                self.command = Some(String::new());
                self.message.clear();
            }
            KeyCode::Char('u') => {
                if !self.buffer.undo() {
                    self.note("nothing to undo", YELLOW);
                }
            }
            KeyCode::Char('d') => self.buffer.cut_line(),
            KeyCode::Char('p') => {
                if !self.buffer.paste_line() {
                    self.note("nothing cut yet — d cuts a line", YELLOW);
                }
            }
            // Checkboxes: space flips the one under the cursor, Tab hops to
            // the next. That is the whole interactive-checklist feature —
            // the text stays plain `[ ]` / `[x]` markdown.
            KeyCode::Char(' ') | KeyCode::Char('x') => {
                if !self.buffer.toggle_checkbox() {
                    self.note("no checkbox on this line — type [ ] to make one", DIM);
                }
            }
            KeyCode::Tab => {
                if !self.buffer.next_checkbox() {
                    self.note("no checkboxes in this card", DIM);
                }
            }
            KeyCode::BackTab => {
                self.buffer.prev_checkbox();
            }
            KeyCode::Char('h') => self.buffer.left(),
            KeyCode::Char('l') => self.buffer.right(),
            KeyCode::Char('0') => self.buffer.home(),
            KeyCode::Char('$') => self.buffer.end(),
            KeyCode::Esc => self.note("already in command mode — press e to type", DIM),
            _ => {}
        }
        Ok(None)
    }

    fn insert_mode_key(&mut self, key: KeyEvent, chord: bool) -> Result<Option<Outcome>> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Command;
                self.note("command mode — e types, : runs a command", DIM);
            }
            KeyCode::Char(c) if chord && c.eq_ignore_ascii_case(&'s') => self.save()?,
            KeyCode::Char(c) if chord => {
                if c.eq_ignore_ascii_case(&'z') && !self.buffer.undo() {
                    self.note("nothing to undo", YELLOW);
                }
            }
            KeyCode::Char(c) => self.buffer.insert_char(c),
            KeyCode::Enter => self.buffer.newline(),
            KeyCode::Tab => self.buffer.insert_text("  "),
            KeyCode::Backspace => self.buffer.backspace(),
            KeyCode::Delete => self.buffer.delete(),
            _ => {}
        }
        Ok(None)
    }

    /// The `:` line. Enter runs it, Esc abandons it, Backspace on an empty
    /// line closes it — the three things people try.
    fn command_line_key(&mut self, key: KeyEvent) -> Result<Option<Outcome>> {
        let Some(line) = self.command.as_mut() else {
            return Ok(None);
        };
        match key.code {
            KeyCode::Esc => {
                self.command = None;
            }
            KeyCode::Backspace => {
                if line.pop().is_none() {
                    self.command = None;
                }
            }
            KeyCode::Char(c) => line.push(c),
            KeyCode::Enter => {
                let typed = self.command.take().unwrap_or_default();
                return self.execute(typed.trim());
            }
            _ => {}
        }
        Ok(None)
    }

    fn execute(&mut self, command: &str) -> Result<Option<Outcome>> {
        match command {
            "w" => {
                self.save()?;
                Ok(None)
            }
            "q" => {
                if self.buffer.is_dirty() {
                    self.note("unsaved changes — :wq saves and quits, :q! discards", RED);
                    Ok(None)
                } else {
                    Ok(Some(self.finished()))
                }
            }
            "q!" => Ok(Some(Outcome::Discarded)),
            "wq" | "x" => {
                self.save()?;
                Ok(Some(Outcome::Saved))
            }
            "" => Ok(None),
            other => {
                self.note(&format!("unknown command :{other} — try :w :q :q! :wq"), RED);
                Ok(None)
            }
        }
    }

    fn insert(&mut self, message: &str) {
        self.mode = Mode::Insert;
        self.note(message, DIM);
    }

    fn finished(&self) -> Outcome {
        if self.saved_once {
            Outcome::Saved
        } else {
            Outcome::Unchanged
        }
    }

    /// Write beside the card, then rename over it, so a full disk or a pulled
    /// plug leaves the old card intact rather than half of the new one.
    fn save(&mut self) -> Result<()> {
        let scratch = self.path.with_extension("md.tmp");
        fs::write(&scratch, self.buffer.text()).map_err(|e| Error::io("write", &scratch, e))?;
        fs::rename(&scratch, self.path).map_err(|e| Error::io("save", self.path, e))?;
        self.buffer.mark_saved();
        self.saved_once = true;
        self.note("saved", GREEN);
        Ok(())
    }

    fn note(&mut self, text: &str, color: &'static str) {
        self.message = text.to_string();
        self.message_color = color;
    }

    fn draw(&mut self, out: &mut Stdout, width: usize, height: usize) -> io::Result<()> {
        let rows = text_rows(height);
        let (row, col) = self.buffer.cursor();
        let cursor = view::cursor_row(self.buffer.lines(), row, col, width);
        self.top = view::follow(self.top, cursor, rows);

        queue!(out, Hide)?;
        let segments = view::visible(self.buffer.lines(), width, self.top, rows);
        let kinds = syntax::classify(self.buffer.lines());

        for y in 0..rows {
            queue!(out, MoveTo(0, y as u16), Clear(ClearType::CurrentLine))?;
            if let Some(segment) = segments.get(y) {
                let codes = kinds[segment.line].codes();
                queue!(out, Print(theme::paint_always(&segment.text, codes)))?;
            }
        }

        // The bars need two spare rows; on a tiny window the text wins.
        if height >= 3 {
            self.draw_bars(out, width, height, row, col)?;
        }

        let x = (col % width.max(1)) as u16;
        let y = (cursor - self.top) as u16;
        queue!(out, MoveTo(x, y), Show)?;
        out.flush()
    }

    fn draw_bars(
        &self,
        out: &mut Stdout,
        width: usize,
        height: usize,
        row: usize,
        col: usize,
    ) -> io::Result<()> {
        let (label, label_color) = match (self.command.is_some(), self.mode) {
            (true, _) => (" COMMAND LINE ", YELLOW),
            (false, Mode::Insert) => (" INSERT ", GREEN),
            (false, Mode::Command) => (" COMMAND ", CYAN),
        };
        let flag = if self.buffer.is_dirty() { " [modified]" } else { "" };
        let status = format!(
            "{} {}{}   Ln {}, Col {}",
            theme::paint_always(label, &[REVERSE, label_color, BOLD]),
            self.name,
            flag,
            row + 1,
            col + 1,
        );
        // The label carries its own escape codes, so the padding is measured
        // on the plain text and appended afterwards.
        let plain = format!("{label} {}{}   Ln {}, Col {}", self.name, flag, row + 1, col + 1);
        let padding = width.saturating_sub(plain.chars().count().min(width));

        queue!(
            out,
            MoveTo(0, (height - 2) as u16),
            Clear(ClearType::CurrentLine),
            Print(status),
            Print(" ".repeat(padding)),
            MoveTo(0, (height - 1) as u16),
            Clear(ClearType::CurrentLine),
        )?;

        // One column short: writing the bottom-right cell scrolls some
        // consoles.
        let help_width = width.saturating_sub(1);
        let bottom = match (&self.command, self.mode) {
            (Some(typed), _) => theme::paint_always(&view::fit(&format!(":{typed}"), help_width), &[YELLOW, BOLD]),
            (None, Mode::Insert) => keys(
                &[("Esc", "command keys"), ("^S", "save"), ("^Z", "undo")],
                help_width,
            ),
            (None, Mode::Command) => keys(
                &[
                    ("e", "type"),
                    (":w", "save"),
                    (":q", "quit"),
                    (":wq", "save+quit"),
                    ("space", "tick"),
                    ("tab", "next box"),
                    ("u", "undo"),
                    ("d/p", "cut/paste"),
                ],
                help_width,
            ),
        };
        queue!(out, Print(bottom))?;

        if !self.message.is_empty() && self.command.is_none() {
            let note = format!("  {}", self.message);
            let room = help_width.saturating_sub(plain_keys_width());
            if note.chars().count() < room {
                queue!(
                    out,
                    MoveTo((help_width - note.chars().count()) as u16, (height - 1) as u16),
                    Print(theme::paint_always(&note, &[self.message_color])),
                )?;
            }
        }
        Ok(())
    }
}

/// `key label` pairs, the key bright and the label dim.
fn keys(pairs: &[(&str, &str)], width: usize) -> String {
    let plain: String = pairs
        .iter()
        .map(|(k, l)| format!(" {k} {l} "))
        .collect::<Vec<_>>()
        .join("·");
    if plain.chars().count() > width {
        return view::fit(&plain, width);
    }
    let painted: String = pairs
        .iter()
        .map(|(k, l)| {
            format!(
                " {} {} ",
                theme::paint_always(k, &[BOLD, CYAN]),
                theme::paint_always(l, &[DIM])
            )
        })
        .collect::<Vec<_>>()
        .join(&theme::paint_always("·", &[DIM]));
    let padding = width - plain.chars().count();
    format!("{painted}{}{RESET}", " ".repeat(padding))
}

/// A conservative guess at how much of the help bar the keys occupy, used only
/// to decide whether a message fits beside them.
fn plain_keys_width() -> usize {
    60
}

fn size() -> (usize, usize) {
    let (w, h) = terminal::size().unwrap_or((80, 24));
    ((w as usize).max(1), (h as usize).max(1))
}

/// Rows left for text once the status and help bars are taken out.
fn text_rows(height: usize) -> usize {
    if height >= 3 {
        height - 2
    } else {
        height.max(1)
    }
}
