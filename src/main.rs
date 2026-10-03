//! docket — every project and idea you have, in one list.
//!
//! A card is a markdown file in one folder. There is no database, no daemon
//! and no index; every command is a read or a write of plain text. `main` does
//! nothing but turn arguments into a command and an error into an exit code.

mod books;
mod brief;
mod card;
mod checkbox;
mod cli;
mod clipboard;
mod commands;
mod derive;
mod editor;
mod error;
mod id;
mod outline;
mod pack;
mod pick;
mod resume;
mod shelf;
mod store;
mod theme;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args().skip(1);

    let outcome = cli::parse_args(args).and_then(|(command, book)| commands::dispatch(command, book));

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("docket: {error}");
            ExitCode::from(error.code() as u8)
        }
    }
}
