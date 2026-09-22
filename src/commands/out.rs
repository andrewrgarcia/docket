use std::fs;
use std::path::PathBuf;

use crate::brief;
use crate::error::{Error, Result};
use crate::pick::{self, Choice};
use crate::store::Store;
use crate::theme::{self, BOLD, CYAN, DIM, GREEN};

/// Everything, no questions asked.
pub fn all(store: &Store, out: Option<&str>) -> Result<()> {
    let cards = store.cards()?;
    if cards.is_empty() {
        return Err(Error::other("nothing to write — `dk add .` first"));
    }
    write(&brief::build(&cards, &[]), cards.len(), out)
}

/// The picker, then the same file.
pub fn picked(store: &Store, out: Option<&str>) -> Result<()> {
    let cards = store.cards()?;
    match pick::run(&cards)? {
        Choice::Cancelled => {
            println!("{}", theme::paint("cancelled", &[DIM]));
            Ok(())
        }
        Choice::Write(chosen) if chosen.is_empty() => Err(Error::other(
            "nothing picked — space ticks a card, enter writes the file",
        )),
        Choice::Write(chosen) => write(&brief::build(&cards, &chosen), chosen.len(), out),
    }
}

fn write(text: &str, count: usize, out: Option<&str>) -> Result<()> {
    let path = PathBuf::from(out.unwrap_or(brief::DEFAULT_FILE));
    fs::write(&path, text).map_err(|e| Error::io("write", &path, e))?;
    println!(
        "{} {}  {}  {}",
        theme::paint("wrote", &[GREEN, BOLD]),
        theme::paint(&path.display().to_string(), &[BOLD]),
        theme::paint(&format!("{count} cards"), &[CYAN]),
        theme::paint(&format!("~{} tokens", brief::tokens(text)), &[DIM]),
    );
    Ok(())
}
