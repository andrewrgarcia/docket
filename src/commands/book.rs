use std::fs;
use std::path::{Path, PathBuf};

use crate::books::{self, Registry};
use crate::cli::BookCmd;
use crate::error::{Error, Result};
use crate::theme::{self, BOLD, DIM, GREEN, RED};

pub fn run(cmd: BookCmd) -> Result<()> {
    match cmd {
        BookCmd::List => list(),
        BookCmd::New { name, path } => new(&name, path.as_deref()),
        BookCmd::Add { path, name } => add(&path, name.as_deref()),
        BookCmd::Rm(name) => rm(&name),
        BookCmd::Use(name) => use_book(&name),
    }
}

fn list() -> Result<()> {
    let Some(registry) = Registry::load()? else {
        let only = books::legacy_root()?;
        println!("one store, no books registered: {}", only.display());
        println!("`dk book new <name>` makes a second collection and keeps this one as the default");
        return Ok(());
    };
    print_list(&registry);
    warn_about_home();
    Ok(())
}

/// `DOCKET_HOME` outranks the default book, so with it set a bare `dk` shows
/// that one store and never the index. Say so where books are managed, since
/// that is where the surprise is felt.
fn warn_about_home() {
    if let Some(home) = std::env::var_os("DOCKET_HOME") {
        eprintln!(
            "note: DOCKET_HOME is set ({}) — it overrides your default book and hides the index. Remove it from your shell profile to let `dk book` decide.",
            PathBuf::from(home).display()
        );
    }
}

/// The books as plain rows — what `dk book` prints, and what bare `dk` prints
/// in place of the index when its output is not a terminal.
pub fn print_list(registry: &Registry) {
    let rows = books::summaries(registry);
    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let hashes: Vec<String> = rows.iter().map(|r| r.hash.clone()).collect();
    let shorts: Vec<String> = hashes.iter().map(|h| short_hash(h, &hashes)).collect();
    let hash_w = shorts.iter().map(|s| s.len()).max().unwrap_or(4).max(4);

    println!(
        "{}",
        theme::paint(
            &format!("  {:<hash_w$}  {:<name_w$}  {:>5}  {:>6}  {:>6}  PATH", "HASH", "NAME", "CARDS", "ACTIVE", "NEWEST"),
            &[DIM]
        )
    );
    for (r, short) in rows.iter().zip(&shorts) {
        let mark = if r.default { theme::paint("*", &[GREEN]) } else { " ".into() };
        let hash = theme::paint(&format!("{short:<hash_w$}"), &[DIM]);
        let name = if r.default { theme::paint(&format!("{:<name_w$}", r.name), &[BOLD]) } else { format!("{:<name_w$}", r.name) };
        match &r.counts {
            Some(c) => println!(
                "{mark} {hash}  {name}  {:>5}  {:>6}  {:>6}  {}",
                c.cards,
                c.active,
                books::age_label(c.newest),
                theme::paint(&r.path.display().to_string(), &[DIM])
            ),
            None => println!(
                "{mark} {hash}  {name}  {:>5}  {:>6}  {:>6}  {} {}",
                "-",
                "-",
                "-",
                r.path.display(),
                theme::paint("(missing folder)", &[RED])
            ),
        }
    }
    if registry.default.is_none() && registry.books.len() > 1 {
        println!("no current book: `dk book <name>` picks one");
    }
}

/// The shortest prefix of `hash`, at least four characters, that no other
/// book's hash shares — what the card list does for card ids.
fn short_hash(hash: &str, all: &[String]) -> String {
    for length in 4..=hash.len() {
        let prefix = &hash[..length];
        if !all.iter().any(|other| other != hash && other.starts_with(prefix)) {
            return prefix.to_string();
        }
    }
    hash.to_string()
}

fn new(name: &str, path: Option<&str>) -> Result<()> {
    check_name(name)?;
    let folder = match path {
        Some(p) => PathBuf::from(p),
        None => books::default_book_dir(name)?,
    };
    if folder.exists() && !folder.is_dir() {
        return Err(Error::usage(format!("{} is a file, not a folder", folder.display())));
    }
    fs::create_dir_all(&folder).map_err(|e| Error::io("create", &folder, e))?;
    register(name, &folder)
}

fn add(path: &str, name: Option<&str>) -> Result<()> {
    let folder = PathBuf::from(path);
    if !folder.is_dir() {
        return Err(Error::usage(format!("{path} is not a folder — `dk book new <name> {path}` makes it")));
    }
    let name = match name {
        Some(n) => n.to_string(),
        None => books::name_from_folder(&absolute(&folder)?)
            .ok_or_else(|| Error::usage("cannot name a book after that folder — give a name: `dk book add <path> <name>`"))?,
    };
    check_name(&name)?;
    register(&name, &folder)
}

/// Add a book to the registry. The first time a registry is made, the store
/// the user already has is registered too, and stays the default: starting a
/// second collection must never make the first one disappear from `dk`.
fn register(name: &str, folder: &Path) -> Result<()> {
    let folder = absolute(folder)?;
    let existing = Registry::load()?;
    let mut registry = existing.clone().unwrap_or_default();

    if registry.books.contains_key(name) {
        return Err(Error::BookExists(format!("book `{name}` already exists — `dk book` lists them")));
    }
    if let Some((other, _)) = registry.books.iter().find(|(_, p)| same_folder(p, &folder)) {
        return Err(Error::BookExists(format!("{} is already the book `{other}`", folder.display())));
    }

    let mut note = None;
    if existing.is_none() {
        if let Some((legacy_name, legacy)) = legacy_book(name, &folder)? {
            registry.books.insert(legacy_name.clone(), legacy.clone());
            registry.default = Some(legacy_name.clone());
            note = Some(format!("your existing store {} is now the book `{legacy_name}` (the default)", legacy.display()));
        }
    }

    registry.books.insert(name.to_string(), folder.clone());
    if registry.default.is_none() {
        registry.default = Some(name.to_string());
    }
    registry.save()?;

    if let Some(note) = note {
        println!("{note}");
    }
    println!("book `{name}` → {}{}", folder.display(), if registry.default.as_deref() == Some(name) { " (default)" } else { "" });
    warn_about_home();
    Ok(())
}

/// The store docket already uses, when it has cards and is not the folder
/// being registered. Named after its folder, or `main` when that name is
/// unusable or taken.
fn legacy_book(taken: &str, adding: &Path) -> Result<Option<(String, PathBuf)>> {
    let root = books::legacy_root()?;
    if !root.is_dir() || same_folder(&root, adding) {
        return Ok(None);
    }
    let has_cards = fs::read_dir(&root)
        .map(|it| it.flatten().any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md")))
        .unwrap_or(false);
    if !has_cards {
        return Ok(None);
    }
    let root = absolute(&root)?;
    let name = books::name_from_folder(&root).filter(|n| n != taken).unwrap_or_else(|| "main".into());
    if name == taken {
        return Err(Error::usage("name the new book something other than `main` — that is what your existing store would be called"));
    }
    Ok(Some((name, root)))
}

fn rm(name: &str) -> Result<()> {
    let Some(mut registry) = Registry::load()? else {
        return Err(Error::NoBook { name: name.to_string(), known: Vec::new() });
    };
    let path = registry.path_of(name)?.clone();
    registry.books.remove(name);
    if registry.default.as_deref() == Some(name) {
        let mut rest = registry.books.keys();
        registry.default = match (rest.next(), rest.next()) {
            (Some(only), None) => Some(only.clone()),
            _ => None,
        };
    }
    registry.save()?;
    println!("book `{name}` forgotten; {} is untouched", path.display());
    if registry.is_empty() {
        println!("no books left: dk is back to one store");
    } else if registry.default.is_none() {
        println!("no current book now — `dk book <name>` picks one");
    }
    Ok(())
}

/// `dk book <name|hash>`: make a book current, so bare `dk` lists it and
/// every command without `-b` works in it.
fn use_book(query: &str) -> Result<()> {
    let Some(mut registry) = Registry::load()? else {
        return Err(Error::NoBook { name: query.to_string(), known: Vec::new() });
    };
    let name = registry.resolve(query)?;
    let path = registry.path_of(&name)?.clone();
    registry.default = Some(name.clone());
    registry.save()?;
    println!(
        "{} {}  {}",
        theme::paint("book:", &[DIM]),
        theme::paint(&name, &[BOLD]),
        theme::paint(&path.display().to_string(), &[DIM])
    );
    if let Some(env) = std::env::var("DOCKET_BOOK").ok().filter(|b| !b.is_empty() && *b != name) {
        eprintln!("note: DOCKET_BOOK={env} is set in this shell and still wins over the current book");
    }
    warn_about_home();
    Ok(())
}

fn check_name(name: &str) -> Result<()> {
    if books::valid_name(name) {
        Ok(())
    } else {
        Err(Error::usage(format!(
            "`{name}` is not a book name — lowercase letters, digits, - and _, starting with a letter or digit"
        )))
    }
}

fn absolute(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|e| Error::io("resolve", path, e))
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}
