use std::fs;
use std::path::Path;

use crate::card::{slug, Card, Status};
use crate::derive;
use crate::id;
use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, BOLD, DIM, GREEN};
use crate::ui;

/// With a path: a project, described from whatever manifest it has. Without
/// one: an idea, which is the same thing minus a directory.
pub fn run(store: &Store, path: Option<&str>) -> Result<()> {
    match path {
        Some(p) => from_path(store, Path::new(p)),
        None => from_prompt(store),
    }
}

fn from_path(store: &Store, path: &Path) -> Result<()> {
    let absolute = fs::canonicalize(path).map_err(|e| Error::io("open", path, e))?;
    if !absolute.is_dir() {
        return Err(Error::other(format!(
            "{} is not a directory — a card points at a project, not a file",
            absolute.display()
        )));
    }

    let location = absolute.display().to_string();
    if let Some(existing) = store.cards()?.into_iter().find(|c| c.path == location) {
        return Err(Error::other(format!(
            "`{}` already points at that directory",
            existing.name
        )));
    }

    let found = derive::inspect(&absolute);

    // The project's own name beats the directory's. A crate in `repo/cli` is
    // `fur-cli`, and a card called `cli` is a card you cannot find later.
    let directory = absolute
        .file_name()
        .and_then(|s| s.to_str())
        .map(slug)
        .unwrap_or_default();
    let name = match slug(&found.name) {
        manifest if !manifest.is_empty() => manifest,
        _ => directory,
    };
    if name.is_empty() {
        return Err(Error::other(
            "cannot name that project — `dk add` with no path makes a card by hand",
        ));
    }

    create(
        store,
        &name,
        Status::Active,
        &found.what,
        &location,
        &found.agents,
        &found.readme,
    )
}

fn from_prompt(store: &Store) -> Result<()> {
    let name = slug(&ui::prompt("name: ")?);
    if name.is_empty() {
        return Err(Error::other("a card needs a name"));
    }
    let what = ui::prompt("what: ")?;
    create(store, &name, Status::Idea, &what, "", "", "")
}

#[allow(clippy::too_many_arguments)]
fn create(
    store: &Store,
    name: &str,
    status: Status,
    what: &str,
    path: &str,
    agents: &str,
    readme: &str,
) -> Result<()> {
    if store.exists(name) {
        return Err(Error::Exists(name.to_string()));
    }
    let fresh = id::generate(name, &store.ids()?);
    let body = Card::template(&fresh, name, status, what, path, agents, readme);
    store.write(name, &body)?;

    println!(
        "{}  {}",
        theme::paint(&fresh[..4], &[DIM]),
        theme::paint(name, &[BOLD, GREEN])
    );
    if !readme.is_empty() {
        println!(
            "{}",
            theme::paint(
                &format!(
                    "  README captured, {} lines, ~{} tokens",
                    readme.lines().count(),
                    readme.chars().count() / 4
                ),
                &[DIM]
            )
        );
    }
    if !agents.is_empty() {
        println!("{}", theme::paint(&format!("  {agents} noted"), &[DIM]));
    }
    Ok(())
}
