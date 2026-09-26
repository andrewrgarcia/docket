//! docket — every project and idea you have, in one list.
//!
//! A card is a markdown file in one folder. There is no database, no daemon
//! and no index; every command is a read or a write of plain text. `main` does
//! nothing but turn arguments into a command and an error into an exit code.

mod brief;
mod card;
mod cli;
mod commands;
mod derive;
mod editor;
mod error;
mod id;
mod outline;
mod pick;
mod store;
mod checkbox;
mod theme;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args().skip(1);

    let outcome = cli::parse(args).and_then(commands::dispatch);

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("docket: {error}");
            ExitCode::from(error.code() as u8)
        }
    }
}
