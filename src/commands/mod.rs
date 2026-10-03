mod add;
mod book;
mod edit;
mod here;
mod list;
mod code;
mod out;
mod remove;
mod rename;
mod resume;
mod save;
mod show;
pub(crate) mod write;

use crate::books;
use crate::cli::{Command, HELP};
use crate::error::{Error, Result};
use crate::store::Store;

/// One place where a verb becomes an effect. Each command opens the store
/// itself, so `help` and `version` never depend on one.
///
/// `book` is the book named with `-b`. A card named as `book/card` names its
/// book too; the two may be given together only if they agree.
pub fn dispatch(command: Command, book: Option<String>) -> Result<()> {
    let flag = book.as_deref();
    match command {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Version => {
            println!("dk {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Book(cmd) => {
            if flag.is_some() {
                return Err(Error::usage("`dk book` works on the registry, so it takes no -b"));
            }
            book::run(cmd)
        }
        Command::List => {
            if flag.is_none() {
                if let Some(registry) = books::Registry::load()? {
                    let book_env = std::env::var("DOCKET_BOOK").ok();
                    let home = std::env::var_os("DOCKET_HOME").map(std::path::PathBuf::from);
                    if books::index_wanted(Some(&registry), None, book_env.as_deref(), home.as_deref()) {
                        return index(&registry);
                    }
                }
            }
            list::run(&Store::open(flag)?)
        }
        Command::Show(name) => {
            let (b, name) = scope(flag, &name)?;
            show::run(&Store::open(b)?, name)
        }
        Command::Edit(name) => {
            let (b, name) = scope(flag, &name)?;
            edit::run(&Store::open(b)?, name)
        }
        // `dk where` is what the README's git setup pipes into `mv`, so it
        // prints the path alone, with no decoration.
        Command::Where => {
            println!("{}", Store::open(flag)?.root().display());
            Ok(())
        }
        Command::Code(Some(name)) => {
            let (b, name) = scope(flag, &name)?;
            code::card(&Store::open(b)?, name)
        }
        Command::Code(None) => code::store_dir(&Store::open(flag)?),
        Command::Add(path) => add::run(&Store::open(flag)?, path.as_deref()),
        Command::Pick { out } => out::picked(&Store::open(flag)?, out.as_deref()),
        Command::Out { out } => out::all(&Store::open(flag)?, out.as_deref()),
        Command::Rename { from, to } => {
            let (b, from) = scope(flag, &from)?;
            // A rename stays inside one book: `to` may repeat the book, never change it.
            let (to_book, to) = books::split_qualified(&to);
            if to_book.is_some() && to_book != b {
                return Err(Error::usage("a card cannot be renamed into another book"));
            }
            rename::run(&Store::open(b)?, from, to)
        }
        Command::Remove(name) => {
            let (b, name) = scope(flag, &name)?;
            remove::run(&Store::open(b)?, name)
        }
        Command::Resume { card, out, place } => {
            let (b, card) = scope(flag, &card)?;
            resume::run(&Store::open(b)?, card, out.as_deref(), place.as_deref())
        }
        Command::Here => here::run(&Store::open(flag)?),
        Command::Set { card, key, value } => {
            let (b, card) = scope(flag, &card)?;
            write::set(&Store::open(b)?, card, &key, &value)
        }
        Command::Todo { card, text } => {
            let (b, card) = scope(flag, &card)?;
            write::todo(&Store::open(b)?, card, &text)
        }
        Command::Tick { card, text } => {
            let (b, card) = scope(flag, &card)?;
            write::tick(&Store::open(b)?, card, &text)
        }
        Command::Note { card, section, text } => {
            let (b, card) = scope(flag, &card)?;
            write::note(&Store::open(b)?, card, section.as_deref(), &text)
        }
        Command::Write { card, source } => {
            let (b, card) = scope(flag, &card)?;
            write::write(&Store::open(b)?, card, &source)
        }
        Command::Undo(card) => {
            let (b, card) = scope(flag, &card)?;
            write::undo(&Store::open(b)?, card)
        }
        Command::Save { card, entry, docs, ticks, nexts, dry_run } => {
            let (b, card) = scope(flag, &card)?;
            save::run(
                &Store::open(b)?,
                &save::Request { card, entry: &entry, docs: &docs, ticks: &ticks, nexts: &nexts, dry_run },
            )
        }
    }
}

/// Bare `dk` with several books: the index in a terminal, the plain book list
/// anywhere else (a pipe has no one to press Enter).
fn index(registry: &books::Registry) -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        book::print_list(registry);
        return Ok(());
    }
    match crate::shelf::run(&books::summaries(registry))? {
        crate::shelf::Choice::List(name) => list::run(&Store::open(Some(&name))?),
        crate::shelf::Choice::Pick(name) => out::picked(&Store::open(Some(&name))?, None),
        crate::shelf::Choice::Quit => Ok(()),
    }
}

/// The book and the bare card name for a command that takes a card. A name
/// like `bcrp/moxi` carries its book; a `-b` that says otherwise is a mistake
/// to report, not a conflict to settle silently.
fn scope<'a>(flag: Option<&'a str>, name: &'a str) -> Result<(Option<&'a str>, &'a str)> {
    let (qualified, bare) = books::split_qualified(name);
    match (flag, qualified) {
        (Some(f), Some(q)) if f != q => Err(Error::usage(format!(
            "-b {f} and `{q}/{bare}` name different books — use one"
        ))),
        (f, q) => Ok((q.or(f), bare)),
    }
}
