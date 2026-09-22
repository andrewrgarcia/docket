use crate::error::Result;
use crate::store::Store;
use crate::theme::{self, RED};
use crate::ui;

/// Typing the name is the confirmation. A `y/N` prompt is too easy to answer
/// on reflex for something that deletes writing, and there is no undo.
pub fn run(store: &Store, query: &str) -> Result<()> {
    let card = store.get(query)?;
    let answer = ui::prompt(&format!("delete `{}`? type the name: ", card.name))?;
    if answer.trim() != card.name {
        println!("kept");
        return Ok(());
    }

    store.delete(&card.name)?;
    println!("{}", theme::paint(&format!("deleted {}", card.name), &[RED]));
    Ok(())
}
