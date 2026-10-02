use std::fs;
use std::path::{Path, PathBuf};

use crate::books::{self, Registry};
use crate::card::{age_days, Card};
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

    let rows: Vec<Row> = registry
        .books
        .iter()
        .map(|(name, path)| Row::read(name, path, registry.default.as_deref() == Some(name)))
        .collect();
    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);

    println!("{}", theme::paint(&format!("  {:<name_w$}  {:>5}  {:>6}  {:>6}  PATH", "NAME", "CARDS", "ACTIVE", "NEWEST"), &[DIM]));
    for r in &rows {
        let mark = if r.default { theme::paint("*", &[GREEN]) } else { " ".into() };
        let name = if r.default { theme::paint(&format!("{:<name_w$}", r.name), &[BOLD]) } else { format!("{:<name_w$}", r.name) };
        match &r.counts {
            Some(c) => println!(
                "{mark} {name}  {:>5}  {:>6}  {:>6}  {}",
                c.cards,
                c.active,
                c.newest.map(age).unwrap_or_else(|| "-".into()),
                theme::paint(&r.path.display().to_string(), &[DIM])
            ),
            None => println!(
                "{mark} {name}  {:>5}  {:>6}  {:>6}  {} {}",
                "-",
                "-",
                "-",
                r.path.display(),
                theme::paint("(missing folder)", &[RED])
            ),
        }
    }
    if registry.default.is_none() && registry.books.len() > 1 {
        println!("no default: name a book with -b, or `dk book use <name>`");
    }
    Ok(())
}

struct Row {
    name: String,
    path: PathBuf,
    default: bool,
    counts: Option<Counts>,
}

struct Counts {
    cards: usize,
    active: usize,
    newest: Option<u64>,
}

impl Row {
    /// Read-only: listing books must never rewrite a card, so this parses the
    /// files itself instead of going through `Store::cards`, which backfills
    /// ids.
    fn read(name: &str, path: &Path, default: bool) -> Row {
        Row { name: name.to_string(), path: path.to_path_buf(), default, counts: counts(path) }
    }
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
        let card = Card::parse(stem, &body, 0);
        cards += 1;
        if !card.status.is_cold() {
            active += 1;
        }
        let days = age_days(&file, &card.path);
        newest = Some(newest.map_or(days, |n| n.min(days)));
    }
    Some(Counts { cards, active, newest })
}

fn age(days: u64) -> String {
    if days == 0 { "today".into() } else { format!("{days}d") }
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
        println!("no default now — `dk book use <name>` picks one");
    }
    Ok(())
}

fn use_book(name: &str) -> Result<()> {
    let Some(mut registry) = Registry::load()? else {
        return Err(Error::NoBook { name: name.to_string(), known: Vec::new() });
    };
    registry.path_of(name)?;
    registry.default = Some(name.to_string());
    registry.save()?;
    println!("default book: `{name}`");
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
