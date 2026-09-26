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
use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, BOLD, CYAN, DIM, GREEN};

pub use state::Picker;

pub fn run(store: &Store, out: Option<&str>) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::other("pick needs a terminal — `dk out` writes everything"));
    }

    loop {
        let cards = store.cards()?;
        if cards.is_empty() {
            return Err(Error::other("no cards yet — `dk add .` in a project"));
        }
        let mut picker = Picker::new(cards.clone());

        let mut save = |changed: &[(String, String)]| -> Result<()> {
            for (name, body) in changed {
                store.write(name, body)?;
            }
            Ok(())
        };

        match ui::run(&mut picker, &mut save)? {
            ui::Outcome::Quit => return Ok(()),
            ui::Outcome::Edit(name) => {
                // The guard has dropped by now, so the editor gets a clean
                // terminal; the tree is rebuilt from disk on the way back.
                crate::commands::edit_card(store, &name)?;
            }
            ui::Outcome::Write => {
                let selection = picker.selection();
                let text = brief::build_selection(&cards, &selection);
                let path = PathBuf::from(out.unwrap_or(brief::DEFAULT_FILE));
                fs::write(&path, &text).map_err(|e| Error::io("write", &path, e))?;

                let sections: usize = selection.iter().map(|(_, s)| s.len()).sum();
                println!(
                    "{} {}  {}  {}",
                    theme::paint("wrote", &[GREEN, BOLD]),
                    theme::paint(&path.display().to_string(), &[BOLD]),
                    theme::paint(
                        &format!("{sections} sections from {} cards", selection.len()),
                        &[CYAN]
                    ),
                    theme::paint(&format!("~{} tokens", brief::tokens(&text)), &[DIM]),
                );
                return Ok(());
            }
        }
    }
}
