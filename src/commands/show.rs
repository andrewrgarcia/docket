use crate::error::Result;
use crate::store::Store;
use crate::ui;

/// Reading a card, as opposed to editing it.
///
/// The card file holds a `readme:` path rather than the README itself, so
/// `show` reads that file and prints it under a `## readme` heading. What you
/// see is the whole card as an AI would receive it, which is the point of
/// looking at it.
///
/// In a terminal it opens the fold view: the same text in the same colours,
/// with every heading collapsible. Piped, it is the card verbatim — `dk show
/// moxi > card.md` has to produce the card, not a screenshot of it.
pub fn run(store: &Store, query: &str) -> Result<()> {
    let card = store.get(query)?;
    if crate::view::wanted() {
        return crate::view::run(store, &card.name);
    }
    print!("{}", ui::render_card(&card));
    Ok(())
}
