use crate::error::Result;
use crate::store::Store;
use crate::ui;

pub fn run(store: &Store) -> Result<()> {
    print!("{}", ui::table(&store.cards()?));
    Ok(())
}
