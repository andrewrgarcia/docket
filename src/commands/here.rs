use std::path::Path;

use crate::error::{Error, Result};
use crate::store::Store;

/// Print the card that owns the folder you are standing in: the card with a
/// place that is this folder or an ancestor of it. The deepest place wins, so a
/// project nested inside another resolves to the inner one.
///
/// stdout carries the card name alone, so `dk resume "$(dk here)"` works; the
/// place's label goes to stderr, and only for cards that have several.
pub fn run(store: &Store) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| Error::io("read", Path::new("."), e))?;
    let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);

    // (depth, card name, place label, places on the card)
    let mut best: Vec<(usize, String, String, usize)> = Vec::new();
    for card in store.cards()? {
        let all = card.all_places();
        for place in &all {
            let Ok(root) = std::fs::canonicalize(&place.path) else { continue };
            if cwd.starts_with(&root) {
                best.push((depth(&root), card.name.clone(), place.label.clone(), all.len()));
            }
        }
    }
    let Some(deepest) = best.iter().map(|b| b.0).max() else {
        return Err(Error::NoCard(cwd.display().to_string()));
    };
    best.retain(|b| b.0 == deepest);
    best.sort();
    best.dedup_by(|a, b| a.1 == b.1);
    match best.as_slice() {
        [(_, name, label, places)] => {
            if *places > 1 {
                eprintln!("in place: {label}");
            }
            println!("{name}");
            Ok(())
        }
        many => Err(Error::Ambiguous {
            query: cwd.display().to_string(),
            hits: many.iter().map(|b| b.1.clone()).collect(),
        }),
    }
}

fn depth(path: &Path) -> usize {
    path.components().count()
}
