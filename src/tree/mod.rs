//! `dk tree` — every card, every section, folded or opened at will.

mod state;
mod ui;

use std::io::{self, IsTerminal};

use crate::error::{Error, Result};
use crate::store::Store;

pub use state::Tree;

/// Browse the store. Ticking a checkbox writes through to the card at once,
/// because a view that silently holds unsaved state is a view you cannot
/// trust; `e` hands the card to the editor and comes back.
pub fn run(store: &Store) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::other("tree needs a terminal — `dk out` writes a file"));
    }

    loop {
        let cards = store.cards()?;
        if cards.is_empty() {
            return Err(Error::other("no cards yet — `dk add .` in a project"));
        }
        let mut tree = Tree::new(cards);

        let mut save = |changed: &[(String, String)]| -> Result<()> {
            for (name, body) in changed {
                store.write(name, body)?;
            }
            Ok(())
        };

        match ui::run(&mut tree, &mut save)? {
            ui::Outcome::Quit => return Ok(()),
            ui::Outcome::Edit(name) => {
                // The guard has dropped by now, so the editor gets a clean
                // terminal; the tree is rebuilt from disk on the way back.
                crate::commands::edit_card(store, &name)?;
            }
        }
    }
}
