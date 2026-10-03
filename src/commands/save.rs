use crate::cardtext;
use crate::commands::write::text_arg;
use crate::error::{Error, Result};
use crate::ledger::{self, Stamp};
use crate::store::Store;

/// What `dk save` was asked to do.
pub struct Request<'a> {
    pub card: &'a str,
    pub entry: &'a str,
    pub docs: &'a [String],
    pub ticks: &'a [String],
    pub nexts: &'a [String],
    pub dry_run: bool,
}

/// End a session in one command: the entry (and documents) into the card's
/// sessions conversation, then the card write-back — its `state:` line from
/// the entry's `## state`, the boxes named with `--tick`, the items added with
/// `--next`. The card is checked before anything is written, so a bad
/// `--tick` stops the save instead of leaving half of it on disk.
///
/// stdout lists the files written, one per line; the rest goes to stderr.
pub fn run(store: &Store, req: &Request) -> Result<()> {
    let card = store.get(req.card)?;
    let entry_text = if req.entry == "-" {
        text_arg("-")?
    } else {
        std::fs::read_to_string(req.entry).map_err(|e| Error::io("read", std::path::Path::new(req.entry), e))?
    };
    let docs: Vec<(String, String)> = req
        .docs
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .map(|text| (path.clone(), text))
                .map_err(|e| Error::io("read", std::path::Path::new(path), e))
        })
        .collect::<Result<_>>()?;

    let entry = ledger::check_entry(&entry_text)?;
    let state = cardtext::section_text(&entry, "state");
    if state.is_empty() {
        return Err(Error::usage("the entry's `## state` is empty — say what is in flight, or `no task in flight`"));
    }

    // The card change, worked out in full before anything is written.
    let mut body = cardtext::set_state(&card.body, &state);
    for t in req.ticks {
        body = cardtext::tick(&body, t)?;
    }
    for n in req.nexts {
        body = cardtext::todo(&body, n);
    }

    let saved = ledger::save(store.root(), &card, &entry, &docs, Stamp::now(), req.dry_run)?;

    let verb = if req.dry_run { "would write" } else { "wrote" };
    if saved.created {
        eprintln!("{verb} a new sessions conversation: {}", saved.folder.display());
    }
    for (path, revised) in &saved.docs {
        eprintln!("{verb} {} {}", if *revised { "revised document" } else { "document" }, path.display());
    }
    eprintln!("{verb} entry {}", saved.entry.display());
    eprintln!(
        "{} card {}: state set{}{}",
        if req.dry_run { "would update" } else { "updated" },
        card.name,
        if req.ticks.is_empty() { String::new() } else { format!(", {} ticked", req.ticks.len()) },
        if req.nexts.is_empty() { String::new() } else { format!(", {} added to ## next", req.nexts.len()) },
    );

    if req.dry_run {
        return Ok(());
    }
    if body != card.body {
        store.snapshot(&card.name)?;
        store.write(&card.name, &body)?;
        let back = std::fs::read_to_string(store.card_path(&card.name))
            .map_err(|e| Error::io("read back", &store.card_path(&card.name), e))?;
        if back != body {
            return Err(Error::other(format!("{} did not read back as written", card.name)));
        }
    }
    for (path, _) in &saved.docs {
        println!("{}", path.display());
    }
    println!("{}", saved.entry.display());
    Ok(())
}
