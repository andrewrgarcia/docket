use crate::error::Result;
use crate::store::Store;
use crate::theme::{self, DIM};
use crate::ui;

pub fn run(store: &Store) -> Result<()> {
    // Only a registered book names itself: with no books the list is the
    // list it always was.
    if let Some(book) = store.book() {
        println!("{}", theme::paint(&format!("book: {book}"), &[DIM]));
    }
    print!("{}", ui::table(&store.cards()?));
    Ok(())
}
