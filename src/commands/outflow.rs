use std::io::{self, IsTerminal, Read, Write};

use crate::brief;
use crate::clipboard;
use crate::diff;
use crate::error::{Error, Result};
use crate::reply;
use crate::store::Store;
use crate::ui;

/// Clipboard when there is one, stdout when there is not. The fallback is not
/// a failure mode — `docket out > brief.md` and `docket out | ssh …` are how
/// this gets used on a server.
pub fn out(store: &Store) -> Result<()> {
    let cards = store.cards()?;
    if cards.is_empty() {
        return Err(Error::other("nothing to send — `docket add .` first"));
    }
    let checked = store.prune_selection()?;
    let text = brief::build(&cards, &checked);

    let piped = !io::stdout().is_terminal();
    if piped || !clipboard::copy(&text) {
        print!("{text}");
        io::stdout()
            .flush()
            .map_err(|e| Error::other(format!("cannot write to stdout: {e}")))?;
        if !piped {
            eprintln!("{}", brief::summary(cards.len(), checked.len(), false));
        }
        return Ok(());
    }

    println!("{}", brief::summary(cards.len(), checked.len(), true));
    Ok(())
}

/// Reads an AI reply, shows what each card block would change, and writes only
/// what the user approves. Whole-card replacement, not patching: a model emits
/// a correct card far more reliably than it emits a correct diff.
pub fn apply(store: &Store) -> Result<()> {
    let (text, interactive) = incoming()?;
    let blocks = reply::parse(&text);
    if blocks.is_empty() {
        return Err(Error::other(
            "no `# name` blocks in that reply — nothing to apply",
        ));
    }

    store.begin_undo()?;
    let mut applied = 0;
    let mut skipped = 0;

    for block in blocks {
        let path = store.card_path(&block.name);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let incoming_body = format!("{}\n", block.body.trim_end());

        if existing.trim() == incoming_body.trim() {
            skipped += 1;
            continue;
        }

        let changes = diff::diff(&existing, &incoming_body);
        let (added, removed) = diff::tally(&changes);
        let verb = if path.exists() { "update" } else { "create" };
        println!("{verb} {} (+{added} -{removed})", block.name);
        print!("{}", diff::render(&changes));

        // With no one at the keyboard the diff is still printed and `undo`
        // still works, so applying is the useful default rather than a
        // silent no-op.
        let go = if interactive { ui::confirm("  apply? [y/N] ")? } else { true };
        if go {
            store.journal(&block.name)?;
            store.write(&block.name, &incoming_body)?;
            applied += 1;
        } else {
            skipped += 1;
        }
    }

    match (applied, skipped) {
        (0, 0) => println!("nothing to do"),
        (0, s) => println!("nothing applied, {s} left alone"),
        (a, 0) => println!("{a} applied — `docket undo` reverses it"),
        (a, s) => println!("{a} applied, {s} left alone — `docket undo` reverses it"),
    }
    Ok(())
}

/// Piped stdin wins (`docket in < reply.md`), then the clipboard, then an
/// interactive paste. The flag says whether a human is present to answer the
/// per-card prompt.
fn incoming() -> Result<(String, bool)> {
    if !io::stdin().is_terminal() {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| Error::other(format!("cannot read from stdin: {e}")))?;
        if !buffer.trim().is_empty() {
            return Ok((buffer, false));
        }
    }
    if let Some(text) = clipboard::paste() {
        return Ok((text, true));
    }
    eprintln!("(no clipboard tool — paste the reply, then Ctrl-D)");
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| Error::other(format!("cannot read from stdin: {e}")))?;
    if buffer.trim().is_empty() {
        return Err(Error::other("nothing to read"));
    }
    // stdin is spent, so there is no one left to answer prompts.
    Ok((buffer, false))
}
