mod add;
mod edit;
mod list;
mod code;
mod out;
mod remove;
mod rename;
mod resume;
mod show;

use crate::cli::{Command, HELP};
use crate::error::Result;
use crate::store::Store;

/// One place where a verb becomes an effect. Each command opens the store
/// itself, so `help` and `version` never depend on one.
pub fn dispatch(command: Command) -> Result<()> {
    match command {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Version => {
            println!("dk {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::List => list::run(&Store::open()?),
        Command::Show(name) => show::run(&Store::open()?, &name),
        Command::Edit(name) => edit::run(&Store::open()?, &name),
        // `dk where` is what the README's git setup pipes into `mv`, so it
        // prints the path alone, with no decoration.
        Command::Where => {
            println!("{}", Store::open()?.root().display());
            Ok(())
        }
        Command::Code(Some(name)) => code::card(&Store::open()?, &name),
        Command::Code(None) => code::store_dir(&Store::open()?),
        Command::Add(path) => add::run(&Store::open()?, path.as_deref()),
        Command::Pick { out } => out::picked(&Store::open()?, out.as_deref()),
        Command::Out { out } => out::all(&Store::open()?, out.as_deref()),
        Command::Rename { from, to } => rename::run(&Store::open()?, &from, &to),
        Command::Remove(name) => remove::run(&Store::open()?, &name),
        Command::Resume { card, out } => resume::run(&Store::open()?, &card, out.as_deref()),
    }
}
