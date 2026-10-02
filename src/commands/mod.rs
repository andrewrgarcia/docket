mod add;
mod book;
mod edit;
mod list;
mod code;
mod out;
mod remove;
mod rename;
mod resume;
mod show;

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
        Command::List => list::run(&Store::open(flag)?),
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
        Command::Resume { card, out } => {
            let (b, card) = scope(flag, &card)?;
            resume::run(&Store::open(b)?, card, out.as_deref())
        }
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
