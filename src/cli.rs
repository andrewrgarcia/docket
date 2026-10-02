use crate::error::{Error, Result};

pub const HELP: &str = "\
dk — every project and idea you have, in one list.

  dk                    the list
  dk show <name>        read a card
  dk edit <name>        edit a card here in the terminal
  dk code [name]        open a card in VS Code (no name: the whole store)
  dk add [path]         new card; no path means an idea
  dk pick               browse and choose what to send, write DOCKET.md
                        (alias: dk tree)
  dk out                write every card to DOCKET.md
  dk resume <card>      write RESUME.md: the card, its latest sessions, its code
  dk rename <old> <new> rename a card
  dk rm <name>          delete a card
  dk where              print the store path

  dk book               list your books (separate collections of cards)
  dk book new <name> [path]    make a book and register it
  dk book add <path> [name]    register a folder of cards as a book
  dk book rm <name>     forget a book (its folder is left alone)
  dk book use <name>    make a book the default

  --out <file>           with pick, out or resume: write somewhere else
  -b, --book <name>      use this book for one command (or write `name/card`)

Cards are addressed by name or by the first few characters of their hash.
Cards are plain markdown. Set DOCKET_HOME to move the store.
With no books registered there is one store, as always. DOCKET_BOOK picks a
book for a whole shell.
";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    List,
    Show(String),
    Edit(String),
    Code(Option<String>),
    Where,
    Add(Option<String>),
    Pick { out: Option<String> },
    Out { out: Option<String> },
    Rename { from: String, to: String },
    Remove(String),
    Resume { card: String, out: Option<String> },
    Book(BookCmd),
    Help,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BookCmd {
    List,
    New { name: String, path: Option<String> },
    Add { path: String, name: Option<String> },
    Rm(String),
    Use(String),
}

/// The arguments as `main` sees them: the command, and the book named with
/// `-b` if there was one. The flag may sit anywhere, like `--out`.
pub fn parse_args<I>(args: I) -> Result<(Command, Option<String>)>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let (rest, book) = take_book(&args)?;
    Ok((parse(rest)?, book))
}

/// No parser crate: eight verbs, one positional each. The payoff is that an
/// unrecognised word is a card name, so `dk moxi` shows that card.
pub fn parse<I>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let (rest, out) = take_out(&args)?;

    let Some((verb, operands)) = rest.split_first() else {
        return Ok(Command::List);
    };

    let one = |usage: &str| -> Result<String> {
        operands
            .first()
            .cloned()
            .ok_or_else(|| Error::usage(format!("missing name — try `dk {usage}`")))
    };

    match verb.as_str() {
        "help" | "-h" | "--help" => Ok(Command::Help),
        "-V" | "--version" => Ok(Command::Version),
        "show" => Ok(Command::Show(one("show <name>")?)),
        "edit" => Ok(Command::Edit(one("edit <name>")?)),
        "code" => Ok(Command::Code(operands.first().cloned())),
        "where" => Ok(Command::Where),
        "book" | "books" => parse_book(operands),
        "add" => Ok(Command::Add(operands.first().cloned())),
        "pick" | "p" | "tree" | "t" => Ok(Command::Pick { out }),
        "out" => Ok(Command::Out { out }),
        "resume" => Ok(Command::Resume { card: one("resume <card>")?, out }),
        "rm" | "remove" => Ok(Command::Remove(one("rm <name>")?)),
        "rename" | "mv" => match operands {
            [from, to] => Ok(Command::Rename {
                from: from.clone(),
                to: to.clone(),
            }),
            _ => Err(Error::usage(
                "rename takes two names — try `dk rename <old> <new>`",
            )),
        },
        other if other.starts_with('-') => {
            Err(Error::usage(format!("unknown flag `{other}` — try `dk help`")))
        }
        name => Ok(Command::Show(name.to_string())),
    }
}

fn parse_book(operands: &[String]) -> Result<Command> {
    let usage = || Error::usage("try `dk help` for the book commands");
    let cmd = match operands.split_first() {
        None => BookCmd::List,
        Some((verb, rest)) => match (verb.as_str(), rest) {
            ("list" | "ls", []) => BookCmd::List,
            ("new", [name]) => BookCmd::New { name: name.clone(), path: None },
            ("new", [name, path]) => BookCmd::New { name: name.clone(), path: Some(path.clone()) },
            ("add", [path]) => BookCmd::Add { path: path.clone(), name: None },
            ("add", [path, name]) => BookCmd::Add { path: path.clone(), name: Some(name.clone()) },
            ("rm" | "remove", [name]) => BookCmd::Rm(name.clone()),
            ("use", [name]) => BookCmd::Use(name.clone()),
            ("new", _) => return Err(Error::usage("try `dk book new <name> [path]`")),
            ("add", _) => return Err(Error::usage("try `dk book add <path> [name]`")),
            ("rm" | "remove", _) => return Err(Error::usage("try `dk book rm <name>`")),
            ("use", _) => return Err(Error::usage("try `dk book use <name>`")),
            _ => return Err(usage()),
        },
    };
    Ok(Command::Book(cmd))
}

/// Pull `-b <name>` / `--book <name>` out of the arguments wherever it sits.
fn take_book(args: &[String]) -> Result<(Vec<String>, Option<String>)> {
    let mut rest = Vec::with_capacity(args.len());
    let mut book = None;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--book" | "-b" => {
                let value = iter
                    .next()
                    .ok_or_else(|| Error::usage("--book needs a book name — `dk book` lists them"))?;
                book = Some(value.clone());
            }
            other => rest.push(other.to_string()),
        }
    }
    Ok((rest, book))
}

/// Pull `--out <file>` out of the arguments wherever it sits. The only flag
/// there is, because a file written somewhere other than `DOCKET.md` is the
/// one thing that cannot be expressed another way.
fn take_out(args: &[String]) -> Result<(Vec<String>, Option<String>)> {
    let mut rest = Vec::with_capacity(args.len());
    let mut out = None;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--out" | "-o" => {
                let value = iter
                    .next()
                    .ok_or_else(|| Error::usage("--out needs a filename"))?;
                out = Some(value.clone());
            }
            other => rest.push(other.to_string()),
        }
    }
    Ok((rest, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(line: &str) -> Result<Command> {
        parse(line.split_whitespace().map(String::from))
    }

    #[test]
    fn bare_invocation_lists() {
        assert_eq!(parse_words("").unwrap(), Command::List);
    }

    #[test]
    fn an_unknown_word_is_a_card_name() {
        assert_eq!(parse_words("moxi").unwrap(), Command::Show("moxi".into()));
        assert_eq!(parse_words("show moxi").unwrap(), Command::Show("moxi".into()));
    }

    #[test]
    fn tree_is_an_alias_for_pick() {
        for word in ["pick", "p", "tree", "t"] {
            assert_eq!(parse_words(word).unwrap(), Command::Pick { out: None }, "{word}");
        }
    }


    #[test]
    fn where_is_its_own_verb_not_a_card_name() {
        assert_eq!(parse_words("where").unwrap(), Command::Where);
    }

    #[test]
    fn code_takes_an_optional_name() {
        assert_eq!(parse_words("code").unwrap(), Command::Code(None));
        assert_eq!(parse_words("code moxi").unwrap(), Command::Code(Some("moxi".into())));
    }

    #[test]
    fn pick_and_out_take_an_optional_file() {
        assert_eq!(parse_words("pick").unwrap(), Command::Pick { out: None });
        assert_eq!(
            parse_words("out --out brief.md").unwrap(),
            Command::Out { out: Some("brief.md".into()) }
        );
        assert_eq!(
            parse_words("--out brief.md pick").unwrap(),
            Command::Pick { out: Some("brief.md".into()) }
        );
    }

    #[test]
    fn resume_takes_a_card_and_an_optional_file() {
        assert_eq!(
            parse_words("resume moxi").unwrap(),
            Command::Resume { card: "moxi".into(), out: None }
        );
        assert_eq!(
            parse_words("resume moxi --out handoff.md").unwrap(),
            Command::Resume { card: "moxi".into(), out: Some("handoff.md".into()) }
        );
        assert!(matches!(parse_words("resume"), Err(Error::Usage(_))));
    }

    #[test]
    fn rename_needs_exactly_two_names() {
        assert_eq!(
            parse_words("rename cli fur-cli").unwrap(),
            Command::Rename { from: "cli".into(), to: "fur-cli".into() }
        );
        assert!(matches!(parse_words("rename cli"), Err(Error::Usage(_))));
        assert!(matches!(parse_words("rename a b c"), Err(Error::Usage(_))));
    }

    fn parse_all(line: &str) -> Result<(Command, Option<String>)> {
        parse_args(line.split_whitespace().map(String::from))
    }

    #[test]
    fn the_book_flag_goes_anywhere_and_leaves_the_command_alone() {
        assert_eq!(parse_all("-b bcrp").unwrap(), (Command::List, Some("bcrp".into())));
        assert_eq!(
            parse_all("show moxi --book bcrp").unwrap(),
            (Command::Show("moxi".into()), Some("bcrp".into()))
        );
        assert_eq!(parse_all("where").unwrap(), (Command::Where, None));
        assert!(matches!(parse_all("show moxi -b"), Err(Error::Usage(_))));
    }

    #[test]
    fn book_verbs_parse() {
        let book = |line: &str| match parse_words(line).unwrap() {
            Command::Book(b) => b,
            other => panic!("{other:?}"),
        };
        assert_eq!(book("book"), BookCmd::List);
        assert_eq!(book("book new bcrp"), BookCmd::New { name: "bcrp".into(), path: None });
        assert_eq!(
            book("book new bcrp /x"),
            BookCmd::New { name: "bcrp".into(), path: Some("/x".into()) }
        );
        assert_eq!(book("book add /x"), BookCmd::Add { path: "/x".into(), name: None });
        assert_eq!(
            book("book add /x bcrp"),
            BookCmd::Add { path: "/x".into(), name: Some("bcrp".into()) }
        );
        assert_eq!(book("book rm bcrp"), BookCmd::Rm("bcrp".into()));
        assert_eq!(book("book use bcrp"), BookCmd::Use("bcrp".into()));
        assert!(matches!(parse_words("book new"), Err(Error::Usage(_))));
        assert!(matches!(parse_words("book rm"), Err(Error::Usage(_))));
        assert!(matches!(parse_words("book frobnicate"), Err(Error::Usage(_))));
    }

    #[test]
    fn missing_operands_and_stray_flags_are_usage_errors() {
        assert!(matches!(parse_words("edit"), Err(Error::Usage(_))));
        assert!(matches!(parse_words("--nope"), Err(Error::Usage(_))));
        assert!(matches!(parse_words("out --out"), Err(Error::Usage(_))));
    }
}
