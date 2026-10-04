//! `dk show` in a terminal — the card as you know it, with every heading a
//! fold, from the card's `# name` down through the headings of its README.

mod state;
mod ui;

use std::io::{self, IsTerminal};

use crate::error::Result;
use crate::store::Store;

pub use state::View;

/// Whether the fold view can run: both ends must be a terminal. A pipe, a
/// redirect or an agent's shell gets the plain card instead.
pub fn wanted() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && crate::theme::enabled()
}

/// Read a card. `e` hands it to the editor and comes back to the view,
/// re-read from disk, with the cursor where it was if that line still exists.
pub fn run(store: &Store, query: &str) -> Result<()> {
    let mut cursor = 0;
    loop {
        let card = store.get(query)?;
        let mut view = View::new(&crate::ui::card_text(&card));
        view.cursor = cursor.min(view.lines.len().saturating_sub(1));
        view.settle();

        let title = match store.book() {
            Some(book) => format!("{book}/{}", card.name),
            None => card.name.clone(),
        };
        match ui::run(&mut view, &title)? {
            ui::Outcome::Quit => return Ok(()),
            ui::Outcome::Edit => {
                cursor = view.cursor;
                crate::commands::edit::run(store, &card.name)?;
            }
        }
    }
}
