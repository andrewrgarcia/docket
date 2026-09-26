//! `dk pick` — the whole store as a fold-out tree, with a checkbox beside
//! every card and every section.
//!
//! This used to be two screens: `pick` chose whole cards, `tree` browsed
//! sections. They were the same screen with different verbs, so they are one
//! screen now. Granularity is the point: a card's `## now` is worth sending
//! far more often than its README.

mod state;
mod ui;

use std::fs;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crate::brief;
use crate::clipboard;
use crate::pack;
use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, BOLD, CYAN, DIM, GREEN};

pub use state::Picker;

pub fn run(store: &Store, out: Option<&str>) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::other("pick needs a terminal — `dk out` writes everything"));
    }

    let cards = store.cards()?;
    if cards.is_empty() {
        return Err(Error::other("no cards yet — `dk add .` in a project"));
    }
    let mut picker = Picker::new(cards.clone());

    let outcome = ui::run(&mut picker)?;
    let selection = picker.selection();

    match outcome {
        ui::Outcome::Quit => Ok(()),
        ui::Outcome::Print => {
            let text = brief::build_selection(&cards, &selection);
            let path = PathBuf::from(out.unwrap_or(brief::DEFAULT_FILE));
            fs::write(&path, &text).map_err(|e| Error::io("write", &path, e))?;
            report("wrote", &path.display().to_string(), &selection, brief::tokens(&text));
            Ok(())
        }
        ui::Outcome::Copy => {
            let text = brief::build_selection(&cards, &selection);
            match clipboard::copy(&text) {
                Some(clipboard::Route::Tool(name)) => {
                    report("copied", name, &selection, brief::tokens(&text));
                    Ok(())
                }
                // OSC 52 is size-capped and silently dropped by some
                // terminals, with no way to tell — so it is reported as
                // attempted, never as done.
                Some(clipboard::Route::Osc52) => {
                    report("sent to the terminal", "OSC 52", &selection, brief::tokens(&text));
                    println!(
                        "{}",
                        theme::paint(
                            "  if nothing landed, your terminal dropped it — use p and open the file",
                            &[DIM]
                        )
                    );
                    Ok(())
                }
                None => Err(Error::other(format!(
                    "no clipboard — {}, or use `p` to write the file",
                    clipboard::install_hint()
                ))),
            }
        }
        ui::Outcome::Zip => {
            let index = brief::index_only(&cards, &selection);
            let files: Vec<(String, String)> = selection
                .iter()
                .filter_map(|(name, chosen)| {
                    let card = cards.iter().find(|c| &c.name == name)?;
                    Some((name.clone(), pack::card_markdown(card, chosen)))
                })
                .collect();

            let path = PathBuf::from(out.unwrap_or(brief::DEFAULT_ZIP));
            let count = pack::write(&path, &index, &files)?;
            let tokens: usize = files.iter().map(|(_, body)| brief::tokens(body)).sum();
            report("packed", &format!("{} ({count} files)", path.display()), &selection, tokens);
            Ok(())
        }
    }
}

/// One line, the same shape whichever key was pressed.
fn report(verb: &str, target: &str, selection: &[(String, Vec<usize>)], tokens: usize) {
    let headings: usize = selection.iter().map(|(_, chosen)| chosen.len()).sum();
    println!(
        "{} {}  {}  {}",
        theme::paint(verb, &[GREEN, BOLD]),
        theme::paint(target, &[BOLD]),
        theme::paint(
            &format!("{headings} headings from {} cards", selection.len()),
            &[CYAN]
        ),
        theme::paint(&format!("~{tokens} tokens"), &[DIM]),
    );
}
