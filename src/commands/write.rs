//! The commands that change a card without an editor: `set`, `todo`, `tick`,
//! `note`, `write`, `undo`. They exist so an AI (or a script) can keep a card
//! current from a shell — nothing a person does changes, and every one of
//! these keeps the previous version for `dk undo`.
//!
//! Each prints one line saying what changed, so a session's log reads as a
//! list of facts rather than a guess about what happened.

use std::io::{self, IsTerminal, Read};

use crate::cardtext;
use crate::error::{Error, Result};
use crate::store::Store;

/// The text a command was given: the words after the card, or stdin for `-`.
/// A lone `-` with a terminal on stdin would wait forever for nothing, so it
/// is refused.
pub fn text_arg(words: &str) -> Result<String> {
    if words != "-" {
        return Ok(words.to_string());
    }
    if io::stdin().is_terminal() {
        return Err(Error::usage("`-` reads the text from stdin — pipe it in"));
    }
    let mut text = String::new();
    io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| Error::other(format!("cannot read stdin: {e}")))?;
    if text.trim().is_empty() {
        return Err(Error::usage("stdin was empty"));
    }
    Ok(text)
}

/// Read the card, apply `edit`, keep the old version, write the new one.
fn change(store: &Store, query: &str, edit: impl FnOnce(&str) -> Result<String>) -> Result<String> {
    let card = store.get(query)?;
    let new = edit(&card.body)?;
    if new == card.body {
        return Ok(card.name);
    }
    store.snapshot(&card.name)?;
    store.write(&card.name, &new)?;
    Ok(card.name)
}

pub fn set(store: &Store, query: &str, key: &str, value: &str) -> Result<()> {
    let value = text_arg(value)?;
    let name = if key.eq_ignore_ascii_case("state") {
        change(store, query, |b| Ok(cardtext::set_state(b, &value)))?
    } else {
        change(store, query, |b| cardtext::set_field(b, key, &value))?
    };
    println!("{name}: {} set", key.to_ascii_lowercase());
    Ok(())
}

pub fn todo(store: &Store, query: &str, text: &str) -> Result<()> {
    let text = text_arg(text)?;
    let name = change(store, query, |b| Ok(cardtext::todo(b, &text)))?;
    println!("{name}: added to ## next");
    Ok(())
}

pub fn tick(store: &Store, query: &str, text: &str) -> Result<()> {
    let name = change(store, query, |b| cardtext::tick(b, text))?;
    println!("{name}: ticked");
    Ok(())
}

pub fn note(store: &Store, query: &str, section: Option<&str>, text: &str) -> Result<()> {
    let text = text_arg(text)?;
    let heading = section.unwrap_or("notes");
    if heading.trim().trim_start_matches('#').trim().eq_ignore_ascii_case("readme") {
        return Err(Error::usage("`## readme` is the project's README, linked — write in the project, not the card"));
    }
    let name = change(store, query, |b| Ok(cardtext::append(b, heading, &text)))?;
    println!("{name}: added to ## {}", heading.trim().trim_start_matches('#').trim());
    Ok(())
}

/// Replace the whole card. The id is the card's identity, so a text without
/// one gets the old one, and a text with a different one is refused.
pub fn write(store: &Store, query: &str, source: &str) -> Result<()> {
    let text = if source == "-" {
        text_arg("-")?
    } else {
        std::fs::read_to_string(source).map_err(|e| Error::io("read", std::path::Path::new(source), e))?
    };
    let name = change(store, query, |old| {
        let old_id = old.lines().find_map(|l| l.strip_prefix("id:")).map(str::trim).unwrap_or("");
        let new_id = text
            .lines()
            .take_while(|l| !l.starts_with("## "))
            .find_map(|l| l.strip_prefix("id:"))
            .map(str::trim);
        let mut body = text.trim_end().to_string();
        body.push('\n');
        match new_id {
            Some(id) if id != old_id => Err(Error::usage(format!(
                "the new text has id `{id}` but the card's id is `{old_id}` — ids never change"
            ))),
            Some(_) => Ok(body),
            None if old_id.is_empty() => Ok(body),
            None => {
                let mut lines: Vec<&str> = body.lines().collect();
                let at = usize::from(lines.first().is_some_and(|l| l.starts_with("# ")));
                let id_line = format!("id: {old_id}");
                lines.insert(at, &id_line);
                Ok(format!("{}\n", lines.join("\n")))
            }
        }
    })?;
    println!("{name}: rewritten (dk undo {name} takes it back)");
    Ok(())
}

pub fn undo(store: &Store, query: &str) -> Result<()> {
    let card = store.get(query)?;
    if store.undo(&card.name)? {
        println!("{}: restored the previous version (dk undo again swaps back)", card.name);
    } else {
        println!("{}: nothing to undo", card.name);
    }
    Ok(())
}
