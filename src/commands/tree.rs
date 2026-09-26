use crate::error::Result;
use crate::store::Store;

pub fn run(store: &Store) -> Result<()> {
    crate::tree::run(store)
}
