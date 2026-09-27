use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, BOLD, DIM};

/// Open a card in VS Code. `dk code` with no card opens the whole store as a
/// folder, which is the useful case: every card in one sidebar, with search
/// and multi-file edit across all of them.
///
/// This replaced a `dk open` that handed the file to whatever the desktop had
/// registered for markdown. In practice that was a coin flip between a text
/// editor, a previewer that could not edit, and nothing at all. Naming the
/// editor is more honest than guessing.
pub fn card(store: &Store, query: &str) -> Result<()> {
    let card = store.get(query)?;
    launch(&store.card_path(&card.name))
}

pub fn store_dir(store: &Store) -> Result<()> {
    launch(store.root())
}

/// `$DOCKET_CODE` wins; otherwise the usual VS Code binaries, and the forks
/// people actually use, in the order they are likely to be installed.
const EDITORS: &[&str] = &["code", "codium", "cursor", "windsurf", "code-insiders"];

fn launch(path: &Path) -> Result<()> {
    if let Some(custom) = std::env::var_os("DOCKET_CODE") {
        let custom = custom.to_string_lossy().into_owned();
        let mut words = custom.split_whitespace();
        let program = words
            .next()
            .ok_or_else(|| Error::other("$DOCKET_CODE is empty"))?;
        let args: Vec<&str> = words.collect();
        return spawn(program, &args, path)
            .map(|()| report(program, path))
            .map_err(|e| Error::other(format!("cannot run `{program}`: {e}")));
    }

    for program in EDITORS {
        if spawn(program, &[], path).is_ok() {
            report(program, path);
            return Ok(());
        }
    }
    Err(Error::other(
        "no VS Code on PATH — set $DOCKET_CODE to your editor, or use `dk edit`",
    ))
}

/// Detached, with stdio silenced: the window outlives docket, and Electron's
/// startup chatter is not docket's to print.
fn spawn(program: &str, args: &[&str], path: &Path) -> std::io::Result<()> {
    Command::new(program)
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn report(program: &str, path: &Path) {
    println!(
        "{} {}",
        theme::paint(program, &[BOLD]),
        theme::paint(&path.display().to_string(), &[DIM])
    );
}
