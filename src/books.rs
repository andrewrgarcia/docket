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

    /// The book a word means: its exact name, else the one book whose name or
    /// hash starts with it — the same rule cards follow.
    pub fn resolve(&self, query: &str) -> Result<String> {
        if self.books.contains_key(query) {
            return Ok(query.to_string());
        }
        let by_id = crate::id::looks_like(query);
        let hits: Vec<&String> = self
            .books
            .keys()
            .filter(|name| name.starts_with(query) || (by_id && hash(name).starts_with(query)))
            .collect();
        match hits.as_slice() {
            [only] => Ok((*only).clone()),
            [] => Err(Error::NoBook {
                name: query.to_string(),
                known: self.books.keys().cloned().collect(),
            }),
            many => Err(Error::usage(format!(
                "`{query}` matches several books ({}) — type more of it",
                many.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ))),
        }
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
/// The registry's default is the "current book": `dk book <name>` sets it and
/// bare `dk` lists it. It is the only remembered choice, it lives in the
/// registry file rather than in hidden state, and `dk` prints `book: <name>`
/// above every list so a card cannot land in the wrong collection unseen.
pub fn choose(
    registry: Option<&Registry>,
    explicit: Option<&str>,
    book_env: Option<&str>,
    home_env: Option<PathBuf>,
) -> Result<Choice> {
    let named = |reg: Option<&Registry>, name: &str| -> Result<Choice> {
        match reg {
            Some(reg) => {
                let name = reg.resolve(name)?;
                let path = reg.path_of(&name)?.clone();
                Ok(Choice::Book { name, path })
            }
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
            "several books ({}) and none is current — `dk book <name>` picks one, `-b <name>` names one for a single command",
            reg.books.keys().cloned().collect::<Vec<_>>().join(", ")
        ))),
    }
}

/// One book as `dk book` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub name: String,
    /// Eight hex characters derived from the name; see [`hash`].
    pub hash: String,
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
            hash: hash(name),
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

/// A book's hash: eight hex characters from its name, so it is the same on
/// every machine and survives the folder moving. Books are not created with
/// an id the way cards are, and the name is the one thing that already
/// identifies a book everywhere it is used.
pub fn hash(name: &str) -> String {
    crate::id::of_text(&format!("book:{name}"))
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
    fn a_book_resolves_by_name_prefix_or_hash() {
        let r = reg(Some("a"), &[("docket", "/a"), ("demo", "/b"), ("work", "/c")]);
        assert_eq!(r.resolve("docket").unwrap(), "docket");
        assert_eq!(r.resolve("wo").unwrap(), "work");
        assert_eq!(r.resolve(&hash("demo")[..4]).unwrap(), "demo");
        assert!(matches!(r.resolve("d"), Err(Error::Usage(_))), "docket and demo both start with d");
        assert!(matches!(r.resolve("zz"), Err(Error::NoBook { .. })));
    }

    #[test]
    fn a_book_hash_is_stable_and_eight_hex() {
        assert_eq!(hash("docket"), hash("docket"));
        assert_ne!(hash("docket"), hash("demo"));
        assert_eq!(hash("docket").len(), 8);
        assert!(crate::id::looks_like(&hash("docket")));
    }

    #[test]
    fn choosing_a_book_accepts_its_hash() {
        let r = reg(None, &[("docket", "/a"), ("demo", "/b")]);
        let got = choose(Some(&r), Some(&hash("demo")[..5]), None, None).unwrap();
        assert_eq!(got, Choice::Book { name: "demo".into(), path: "/b".into() });
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
