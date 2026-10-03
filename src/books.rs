//! Books: several stores, one small registry that names them.
//!
//! A book is a store — a folder of cards with `sessions/` inside — and nothing
//! about a store changes. Everything new lives here: a registry file that maps
//! a short name to a folder, and the rules for which book a command uses.
//!
//! The registry is hand-editable TOML, read with a reader of our own because
//! the format is two shapes (`default = "x"` and a `[books]` table of
//! `name = "path"`) and a parser crate would outweigh the thing it parses.
//! With no registry file, docket behaves exactly as it did before books.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub const DEFAULT_FILE: &str = "books.toml";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Registry {
    pub default: Option<String>,
    pub books: BTreeMap<String, PathBuf>,
}

/// Where a command's store comes from, before any folder is touched.
#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    /// A registered book, by name.
    Book { name: String, path: PathBuf },
    /// `$DOCKET_HOME`: the store, wherever the user said it is.
    Home(PathBuf),
    /// Nothing chosen: the platform data directory, as before books existed.
    Legacy,
}

impl Registry {
    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    pub fn path_of(&self, name: &str) -> Result<&PathBuf> {
        self.books.get(name).ok_or_else(|| Error::NoBook {
            name: name.to_string(),
            known: self.books.keys().cloned().collect(),
        })
    }

    /// Read the registry file. A missing file is `None`, which means "no
    /// books, behave as always"; a file that cannot be understood is an error
    /// that names the line, because quietly ignoring it would hide the
    /// user's books.
    pub fn load() -> Result<Option<Registry>> {
        let file = config_file()?;
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io("read", &file, e)),
        };
        let registry = Registry::parse(&text)
            .map_err(|(line, why)| Error::other(format!("{}:{line}: {why}", file.display())))?;
        Ok(Some(registry).filter(|r| !r.is_empty()))
    }

    /// Write the registry, or remove the file when no books remain so that
    /// docket goes back to being a one-store tool.
    pub fn save(&self) -> Result<()> {
        let file = config_file()?;
        if self.is_empty() {
            return match fs::remove_file(&file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(Error::io("remove", &file, e)),
            };
        }
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io("create", parent, e))?;
        }
        fs::write(&file, self.render()).map_err(|e| Error::io("write", &file, e))
    }

    /// `(line number, reason)` on failure.
    pub fn parse(text: &str) -> std::result::Result<Registry, (usize, String)> {
        let mut registry = Registry::default();
        let mut in_books = false;

        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                match line {
                    "[books]" => in_books = true,
                    other => return Err((n, format!("unknown section `{other}`"))),
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err((n, "expected `name = \"value\"`".into()));
            };
            let key = key.trim();
            let value = unquote(value.trim()).ok_or((n, "the value must be in double quotes".to_string()))?;

            if in_books {
                if !valid_name(key) {
                    return Err((n, format!("`{key}` is not a book name")));
                }
                if registry.books.insert(key.to_string(), PathBuf::from(value)).is_some() {
                    return Err((n, format!("book `{key}` appears twice")));
                }
            } else if key == "default" {
                registry.default = Some(value);
            } else {
                return Err((n, format!("unknown key `{key}`")));
            }
        }
        Ok(registry)
    }

    pub fn render(&self) -> String {
        let mut out = String::from("# docket books — `dk book` edits this; editing it by hand is fine.\n");
        if let Some(default) = &self.default {
            out.push_str(&format!("default = {}\n", quote(default)));
        }
        out.push_str("\n[books]\n");
        for (name, path) in &self.books {
            out.push_str(&format!("{name} = {}\n", quote(&path.to_string_lossy())));
        }
        out
    }
}

/// `$DOCKET_CONFIG` is the registry file itself, so a test or a second
/// profile can point at its own; otherwise the platform config directory.
pub fn config_file() -> Result<PathBuf> {
    if let Some(p) = env::var_os("DOCKET_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    Ok(dirs::config_dir()
        .ok_or_else(|| Error::other("no config directory on this system — set DOCKET_CONFIG"))?
        .join("docket")
        .join(DEFAULT_FILE))
}

/// The one store docket had before books: `$DOCKET_HOME`, else the platform
/// data directory.
pub fn legacy_root() -> Result<PathBuf> {
    match env::var_os("DOCKET_HOME") {
        Some(p) => Ok(PathBuf::from(p)),
        None => Ok(dirs::data_dir()
            .ok_or_else(|| Error::other("no data directory on this system — set DOCKET_HOME"))?
            .join("docket")),
    }
}

/// Where `dk book new <name>` puts a folder when no path is given. Beside the
/// legacy store, never inside it: a store holds cards, not other stores.
pub fn default_book_dir(name: &str) -> Result<PathBuf> {
    Ok(dirs::data_dir()
        .ok_or_else(|| Error::other("no data directory on this system — give a path"))?
        .join("docket-books")
        .join(name))
}

/// Which book a command uses. First match wins:
///
/// 1. the book named on this command (`-b name`, or `name/card`) — `explicit`
/// 2. `$DOCKET_BOOK`
/// 3. `$DOCKET_HOME`, the store the user pointed at by hand
/// 4. the registry's default; failing that, its only book
/// 5. no registry: the legacy store
///
/// Nothing remembers a "current book" between commands. A sticky selection is
/// how a card lands in the wrong collection.
pub fn choose(
    registry: Option<&Registry>,
    explicit: Option<&str>,
    book_env: Option<&str>,
    home_env: Option<PathBuf>,
) -> Result<Choice> {
    let named = |reg: Option<&Registry>, name: &str| -> Result<Choice> {
        match reg {
            Some(reg) => Ok(Choice::Book { name: name.to_string(), path: reg.path_of(name)?.clone() }),
            None => Err(Error::NoBook { name: name.to_string(), known: Vec::new() }),
        }
    };

    if let Some(name) = explicit {
        return named(registry, name);
    }
    if let Some(name) = book_env.filter(|n| !n.is_empty()) {
        return named(registry, name);
    }
    if let Some(home) = home_env {
        return Ok(Choice::Home(home));
    }
    let Some(reg) = registry else {
        return Ok(Choice::Legacy);
    };
    if let Some(default) = &reg.default {
        return named(Some(reg), default);
    }
    let mut names = reg.books.keys();
    match (names.next(), names.next()) {
        (Some(only), None) => named(Some(reg), only),
        _ => Err(Error::usage(format!(
            "several books ({}) and no default — name one with `-b <name>`, or `dk book use <name>`",
            reg.books.keys().cloned().collect::<Vec<_>>().join(", ")
        ))),
    }
}

/// Whether bare `dk` should show the book index rather than one book's
/// cards: only when nothing has already chosen a store (no `-b`, no
/// `DOCKET_BOOK`, no `DOCKET_HOME`) and there is more than one book to choose
/// from. With one book, or with none, `dk` is the card list it always was.
pub fn index_wanted(
    registry: Option<&Registry>,
    explicit: Option<&str>,
    book_env: Option<&str>,
    home_env: Option<&Path>,
) -> bool {
    explicit.is_none()
        && book_env.filter(|b| !b.is_empty()).is_none()
        && home_env.is_none()
        && registry.is_some_and(|r| r.books.len() > 1)
}

/// One book as the index and `dk book` show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub name: String,
    pub path: PathBuf,
    pub default: bool,
    /// `None` when the folder cannot be read — it has moved or gone.
    pub counts: Option<Counts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub cards: usize,
    pub active: usize,
    /// Days since the most recently touched card, if there are any cards.
    pub newest: Option<u64>,
}

/// Every registered book, by name. Read-only: this parses the card files
/// itself rather than going through `Store::cards`, which backfills ids, so
/// looking at the index never rewrites a card.
pub fn summaries(registry: &Registry) -> Vec<Summary> {
    registry
        .books
        .iter()
        .map(|(name, path)| Summary {
            name: name.clone(),
            path: path.clone(),
            default: registry.default.as_deref() == Some(name),
            counts: counts(path),
        })
        .collect()
}

fn counts(path: &Path) -> Option<Counts> {
    let entries = fs::read_dir(path).ok()?;
    let mut cards = 0;
    let mut active = 0;
    let mut newest: Option<u64> = None;
    for entry in entries.flatten() {
        let file = entry.path();
        if file.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else { continue };
        let Ok(body) = fs::read_to_string(&file) else { continue };
        let card = crate::card::Card::parse(stem, &body, 0);
        cards += 1;
        if !card.status.is_cold() {
            active += 1;
        }
        let days = crate::card::age_days(&file, &card.path);
        newest = Some(newest.map_or(days, |n| n.min(days)));
    }
    Some(Counts { cards, active, newest })
}

/// `today`, `3d`, or `-` when a book has no cards.
pub fn age_label(days: Option<u64>) -> String {
    match days {
        None => "-".into(),
        Some(0) => "today".into(),
        Some(d) => format!("{d}d"),
    }
}

/// `bcrp/moxi` → `(Some("bcrp"), "moxi")`. Card names never contain a slash,
/// so a slash is a book. Anything whose left half is not a book name (`../x`)
/// is left alone and fails later as an unknown card, never as a guess.
pub fn split_qualified(name: &str) -> (Option<&str>, &str) {
    match name.split_once('/') {
        Some((book, card)) if valid_name(book) && !card.is_empty() => (Some(book), card),
        _ => (None, name),
    }
}

/// Lowercase letters, digits, `-` and `_`, starting with a letter or digit.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// A usable book name from a folder name, or `None` when nothing is left.
pub fn name_from_folder(path: &Path) -> Option<String> {
    let base = path.file_name()?.to_str()?;
    Some(crate::card::slug(base)).filter(|s| valid_name(s))
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            out.push(chars.next()?);
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(default: Option<&str>, books: &[(&str, &str)]) -> Registry {
        Registry {
            default: default.map(String::from),
            books: books.iter().map(|(n, p)| (n.to_string(), PathBuf::from(p))).collect(),
        }
    }

    #[test]
    fn a_registry_round_trips_including_windows_paths() {
        let original = reg(Some("work"), &[("work", r"C:\Users\a\docket"), ("home", "/notes/docket")]);
        assert_eq!(Registry::parse(&original.render()).unwrap(), original);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let text = "# mine\n\ndefault = \"a\"\n\n[books]\n# first\na = \"/x\"\n";
        assert_eq!(Registry::parse(text).unwrap(), reg(Some("a"), &[("a", "/x")]));
    }

    #[test]
    fn a_bad_line_is_reported_with_its_number() {
        assert_eq!(Registry::parse("[books]\nnope\n").unwrap_err().0, 2);
        assert_eq!(Registry::parse("[books]\nA = \"/x\"\n").unwrap_err().0, 2);
        assert_eq!(Registry::parse("[books]\na = /x\n").unwrap_err().0, 2);
        assert_eq!(Registry::parse("[other]\n").unwrap_err().0, 1);
        assert_eq!(Registry::parse("[books]\na = \"/x\"\na = \"/y\"\n").unwrap_err().0, 3);
    }

    #[test]
    fn names_are_plain() {
        for ok in ["bcrp", "work-2", "a_b", "9lives"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "Work", "a/b", "-x", "a b", "..", "é"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_slash_makes_a_book_only_when_the_left_half_is_one() {
        assert_eq!(split_qualified("bcrp/moxi"), (Some("bcrp"), "moxi"));
        assert_eq!(split_qualified("moxi"), (None, "moxi"));
        assert_eq!(split_qualified("../x"), (None, "../x"));
        assert_eq!(split_qualified("bcrp/"), (None, "bcrp/"));
    }

    #[test]
    fn the_command_line_beats_the_environment_beats_home_beats_the_default() {
        let r = reg(Some("a"), &[("a", "/a"), ("b", "/b"), ("c", "/c")]);
        let book = |c: Choice| match c {
            Choice::Book { name, .. } => name,
            other => panic!("{other:?}"),
        };
        assert_eq!(book(choose(Some(&r), Some("c"), Some("b"), Some("/h".into())).unwrap()), "c");
        assert_eq!(book(choose(Some(&r), None, Some("b"), Some("/h".into())).unwrap()), "b");
        assert_eq!(choose(Some(&r), None, None, Some("/h".into())).unwrap(), Choice::Home("/h".into()));
        assert_eq!(book(choose(Some(&r), None, None, None).unwrap()), "a");
    }

    #[test]
    fn no_registry_means_the_store_as_before() {
        assert_eq!(choose(None, None, None, None).unwrap(), Choice::Legacy);
        assert_eq!(choose(None, None, None, Some("/h".into())).unwrap(), Choice::Home("/h".into()));
    }

    #[test]
    fn naming_a_book_with_no_registry_is_an_error() {
        assert!(matches!(choose(None, Some("a"), None, None), Err(Error::NoBook { .. })));
    }

    #[test]
    fn several_books_and_no_default_ask_which() {
        let r = reg(None, &[("a", "/a"), ("b", "/b")]);
        assert!(matches!(choose(Some(&r), None, None, None), Err(Error::Usage(_))));
        let one = reg(None, &[("a", "/a")]);
        assert!(matches!(choose(Some(&one), None, None, None), Ok(Choice::Book { .. })));
    }

    #[test]
    fn an_unknown_book_names_the_known_ones() {
        let r = reg(Some("a"), &[("a", "/a"), ("b", "/b")]);
        let Err(e) = choose(Some(&r), Some("zz"), None, None) else { panic!("should fail") };
        let msg = e.to_string();
        assert!(msg.contains("zz") && msg.contains("a, b"), "{msg}");
    }

    #[test]
    fn the_index_shows_only_when_nothing_has_chosen_and_there_is_a_choice() {
        let two = reg(Some("a"), &[("a", "/a"), ("b", "/b")]);
        let one = reg(Some("a"), &[("a", "/a")]);
        assert!(index_wanted(Some(&two), None, None, None));
        assert!(!index_wanted(Some(&one), None, None, None));
        assert!(!index_wanted(None, None, None, None));
        assert!(!index_wanted(Some(&two), Some("b"), None, None));
        assert!(!index_wanted(Some(&two), None, Some("b"), None));
        assert!(index_wanted(Some(&two), None, Some(""), None));
        assert!(!index_wanted(Some(&two), None, None, Some(Path::new("/h"))));
    }

    #[test]
    fn ages_read_as_words() {
        assert_eq!(age_label(None), "-");
        assert_eq!(age_label(Some(0)), "today");
        assert_eq!(age_label(Some(12)), "12d");
    }

    #[test]
    fn a_folder_name_becomes_a_book_name_when_it_can() {
        assert_eq!(name_from_folder(Path::new("/x/My Cards")).as_deref(), Some("my-cards"));
        assert_eq!(name_from_folder(Path::new("/x/docket")).as_deref(), Some("docket"));
        assert_eq!(name_from_folder(Path::new("/")), None);
    }
}
