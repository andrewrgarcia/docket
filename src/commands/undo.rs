use crate::error::Result;
use crate::store::Store;

pub fn run(store: &Store) -> Result<()> {
    match store.restore()? {
        0 => println!("nothing to undo"),
        1 => println!("1 card restored"),
        n => println!("{n} cards restored"),
    }
    Ok(())
}
