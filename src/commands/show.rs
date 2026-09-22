use crate::error::Result;
use crate::store::Store;
use crate::ui;

/// Reading a card, as opposed to editing it. Colour when a terminal is
/// watching, the file verbatim when it is piped — `dk show moxi > x.md` has
/// to produce the card, not a screenshot of it.
pub fn run(store: &Store, query: &str) -> Result<()> {
    let card = store.get(query)?;
    print!("{}", ui::render_card(&card));
    Ok(())
}
