use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::resume::{self, Built};
use crate::store::Store;
use crate::theme::{self, GREEN, GREY, RED, YELLOW};

/// Write `RESUME.md`: the card, the newest sessions, the code slice.
///
/// stdout carries the path written and nothing else, so `cat "$(dk resume
/// moxi)"` works; what each part cost goes to stderr, where it is seen before
/// the file is pasted and never ends up in a pipe by accident.
pub fn run(store: &Store, query: &str, out: Option<&str>, place: Option<&str>) -> Result<()> {
    let card = store.get(query)?;
    let target = PathBuf::from(out.unwrap_or(resume::DEFAULT_FILE));
    refuse_the_store(store, &target)?;

    let built = resume::build(&card, store.root(), place)?;
    fs::write(&target, &built.text).map_err(|e| Error::io("write", &target, e))?;

    report(&built);
    println!("{}", target.display());
    Ok(())
}

/// A `.md` file dropped into the store root would be read as a card on the next
/// `dk`, so the one place a resume may not go is there. Compared by real path,
/// so `../store/RESUME.md` and a symlinked store are caught as well.
fn refuse_the_store(store: &Store, target: &Path) -> Result<()> {
    let parent = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    if let (Ok(there), Ok(store_root)) = (fs::canonicalize(&parent), fs::canonicalize(store.root())) {
        if there == store_root {
            return Err(Error::usage(format!(
                "{} is the docket store — a .md file there would become a card; pick another place with --out",
                store_root.display()
            )));
        }
    }
    Ok(())
}

fn report(built: &Built) {
    for (label, tokens) in [
        ("card", built.card),
        ("sessions", built.sessions),
        ("code", built.code),
        ("total", built.total),
    ] {
        eprintln!("{label:<9}{}", heat(tokens));
    }
}

/// Right-aligned, on the same thresholds the pickers use: grey under 200, green
/// under 1k, amber under 4k, red above.
fn heat(tokens: usize) -> String {
    let words = if tokens < 1_000 {
        format!("{tokens} tok")
    } else {
        format!("{:.1}k tok", tokens as f64 / 1_000.0)
    };
    let colour = match tokens {
        0..=199 => GREY,
        200..=999 => GREEN,
        1_000..=3_999 => YELLOW,
        _ => RED,
    };
    theme::paint(&format!("{words:>9}"), &[colour])
}
