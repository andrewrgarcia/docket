use std::fs;
use std::path::PathBuf;

use crate::brief;
use crate::error::{Error, Result};
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

/// The picker owns its own writing, since it knows which sections were taken.
pub fn picked(store: &Store, out: Option<&str>) -> Result<()> {
    crate::pick::run(store, out)
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
