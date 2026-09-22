use std::env;
use std::path::Path;
use std::process::Command;

use crate::editor::{self, Outcome};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, DIM, GREEN, YELLOW};

/// Which editor opens the card.
#[derive(Debug, PartialEq, Eq)]
enum Choice {
    /// docket's own. The default, because it is the only editor guaranteed to
    /// exist and to behave the same on every machine.
    BuiltIn,
    /// A command the user configured, with any arguments it carries.
    External(String),
}

pub fn run(store: &Store, query: &str) -> Result<()> {
    let card = store.get(query)?;
    let path = store.card_path(&card.name);

    match choose(|key| env::var(key).ok()) {
        Choice::BuiltIn => {
            let (line, color) = match editor::run(&path)? {
                Outcome::Saved => ("saved", GREEN),
                Outcome::Unchanged => ("no changes", DIM),
                Outcome::Discarded => ("changes discarded", YELLOW),
            };
            println!("{} {}", card.name, theme::paint(line, &[color]));
            Ok(())
        }
        Choice::External(command) => external(&command, &path),
    }
}

/// `$DOCKET_EDITOR`, then `$VISUAL`, then `$EDITOR`; the first one that is set
/// and non-empty wins. The value `builtin` forces docket's editor even when
/// `$EDITOR` is set for the sake of other programs. Nothing set means built-in
/// — docket never goes looking for a vi to fall back on.
///
/// Takes the lookup as a function so the rule can be tested without touching
/// the real, process-wide environment.
fn choose(var: impl Fn(&str) -> Option<String>) -> Choice {
    let configured = ["DOCKET_EDITOR", "VISUAL", "EDITOR"]
        .iter()
        .filter_map(|key| var(*key))
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty());

    match configured {
        None => Choice::BuiltIn,
        Some(value) if value.eq_ignore_ascii_case("builtin") => Choice::BuiltIn,
        Some(value) => Choice::External(value),
    }
}

/// `EDITOR="code --wait"` and the like: the first word is the program, the rest
/// are arguments placed before the filename.
fn external(command: &str, path: &Path) -> Result<()> {
    let mut words = command.split_whitespace();
    let Some(program) = words.next() else {
        return Err(Error::other("the configured editor is empty"));
    };

    let status = Command::new(program)
        .args(words)
        .arg(path)
        .status()
        .map_err(|e| {
            Error::other(format!(
                "cannot run `{program}`: {e} — fix $EDITOR, or set DOCKET_EDITOR=builtin"
            ))
        })?;

    if !status.success() {
        return Err(Error::other(format!("`{program}` exited with {status}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(vars: &[(&str, &str)]) -> Choice {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        choose(move |key| {
            vars.iter()
                .find(|(k, _)| k.as_str() == key)
                .map(|(_, v)| v.clone())
        })
    }

    #[test]
    fn nothing_configured_means_the_built_in_editor() {
        assert_eq!(with(&[]), Choice::BuiltIn);
        assert_eq!(with(&[("EDITOR", "  ")]), Choice::BuiltIn);
    }

    #[test]
    fn a_configured_editor_is_respected() {
        assert_eq!(with(&[("EDITOR", "nano")]), Choice::External("nano".into()));
        assert_eq!(
            with(&[("EDITOR", "code --wait")]),
            Choice::External("code --wait".into())
        );
    }

    #[test]
    fn precedence_is_docket_then_visual_then_editor() {
        let all = [("EDITOR", "a"), ("VISUAL", "b"), ("DOCKET_EDITOR", "c")];
        assert_eq!(with(&all), Choice::External("c".into()));
        assert_eq!(with(&all[..2]), Choice::External("b".into()));
    }

    #[test]
    fn builtin_overrides_a_system_wide_editor() {
        let vars = [("EDITOR", "vim"), ("DOCKET_EDITOR", "builtin")];
        assert_eq!(with(&vars), Choice::BuiltIn);
    }
}
