use crate::derive;
use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, BOLD, DIM, GREEN, YELLOW};
use std::path::Path;

/// Re-read each project's README into its card.
///
/// `add` captures the README once, which is right — a card is yours, and
/// docket rewriting your notes behind your back would make it untrustworthy.
/// But a README changes, and re-adding a project is not a thing you should
/// have to do. So refreshing is a command you run, never something that
/// happens on its own, and it touches nothing above the `## readme` heading.
pub fn run(store: &Store, query: Option<&str>) -> Result<()> {
    let cards = match query {
        Some(name) => vec![store.get(name)?],
        None => store.cards()?,
    };

    let mut updated = 0;
    let mut unchanged = 0;
    let mut skipped = Vec::new();

    for card in &cards {
        if card.path.is_empty() {
            skipped.push((card.name.clone(), "no path — it is an idea"));
            continue;
        }
        let dir = Path::new(&card.path);
        if !dir.is_dir() {
            skipped.push((card.name.clone(), "path is gone"));
            continue;
        }

        let found = derive::inspect(dir);
        if found.readme.is_empty() {
            skipped.push((card.name.clone(), "no README there"));
            continue;
        }

        let body = card.with_readme(&found.readme);
        if body == card.body {
            unchanged += 1;
            continue;
        }
        store.write_quietly(&card.name, &body)?;
        println!(
            "{} {}  {}",
            theme::paint("updated", &[GREEN, BOLD]),
            card.name,
            theme::paint(
                &format!("{} lines, ~{} tokens", found.readme.lines().count(), found.readme.chars().count() / 4),
                &[DIM]
            )
        );
        updated += 1;
    }

    for (name, why) in &skipped {
        println!("{} {name}  {}", theme::paint("skipped", &[YELLOW]), theme::paint(why, &[DIM]));
    }

    if cards.is_empty() {
        return Err(Error::other("no cards yet — `dk add .` first"));
    }
    println!(
        "{}",
        theme::paint(
            &format!("{updated} updated, {unchanged} already current, {} skipped", skipped.len()),
            &[DIM]
        )
    );
    Ok(())
}
