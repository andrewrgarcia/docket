use crate::error::Result;
use crate::store::Store;

use super::list;

/// Resolves every name before writing anything, so a typo in the third name
/// does not leave the first two half-applied.
pub fn run(store: &Store, queries: &[String], on: bool) -> Result<()> {
    let names: Vec<String> = queries
        .iter()
        .map(|q| store.get(q).map(|c| c.name))
        .collect::<Result<Vec<_>>>()?;

    let mut selection = store.selection()?;
    for name in names {
        if on {
            selection.insert(name);
        } else {
            selection.remove(&name);
        }
    }
    store.save_selection(&selection)?;
    list::run(store)
}
