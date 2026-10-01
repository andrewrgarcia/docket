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

  --out <file>           with pick, out or resume: write somewhere else

Cards are addressed by name or by the first few characters of their hash.
Cards are plain markdown. Set DOCKET_HOME to move the store.
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
    Help,
    Version,
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

    #[test]
    fn missing_operands_and_stray_flags_are_usage_errors() {
        assert!(matches!(parse_words("edit"), Err(Error::Usage(_))));
        assert!(matches!(parse_words("--nope"), Err(Error::Usage(_))));
        assert!(matches!(parse_words("out --out"), Err(Error::Usage(_))));
    }
}
