use crate::card::slug;
use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, BOLD, DIM, GREEN};

use super::list;

/// Renames the file, rewrites the `#` heading to match, and carries the check
/// state across. The filename is a card's identity, so this is the only way to
/// change a name — editing the heading alone would leave the two disagreeing.
pub fn run(store: &Store, query: &str, requested: &str) -> Result<()> {
    let card = store.get(query)?;
    let new = slug(requested);

    if new.is_empty() {
        return Err(Error::other("a card needs a name"));
    }
    if new == card.name {
        println!("{} already", card.name);
        return Ok(());
    }
    if store.exists(&new) {
        return Err(Error::Exists(new));
    }

    let body = retitle(&card.body, &new);
    store.write(&new, &body)?;
    store.delete(&card.name)?;

    println!(
        "{} {} {}",
        theme::paint(&card.name, &[DIM]),
        theme::paint("->", &[DIM]),
        theme::paint(&new, &[BOLD, GREEN])
    );
    list::run(store)
}

/// Replace the leading `# heading`, or add one when the card has none.
fn retitle(body: &str, name: &str) -> String {
    let mut lines: Vec<&str> = body.lines().collect();
    match lines.first() {
        Some(first) if first.starts_with("# ") => {
            let heading = format!("# {name}");
            lines[0] = &heading;
            let mut out = lines.join("\n");
            out.push('\n');
            out
        }
        _ => format!("# {name}\n{body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_the_heading_and_keeps_the_rest() {
        let body = "# cli\nstatus: active\nwhat: a diary\n\n## now\nparser\n";
        let out = retitle(body, "fur-cli");
        assert!(out.starts_with("# fur-cli\n"));
        assert!(out.contains("## now\nparser"));
        assert!(!out.contains("# cli\n"));
    }

    #[test]
    fn adds_a_heading_when_there_is_none() {
        let out = retitle("status: active\n", "fur-cli");
        assert_eq!(out, "# fur-cli\nstatus: active\n");
    }
}
