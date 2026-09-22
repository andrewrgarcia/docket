use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::card::{age_days, Card};
use crate::id;
use crate::error::{Error, Result};

/// A directory of markdown files. Nothing else lives here, nothing is
/// cached, and no state is kept beside the cards themselves.
#[derive(Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// `$DOCKET_HOME`, else the platform data directory. Created on first use,
    /// which is why there is no `init` command.
    pub fn open() -> Result<Store> {
        let root = match env::var_os("DOCKET_HOME") {
            Some(p) => PathBuf::from(p),
            None => dirs::data_dir()
                .ok_or_else(|| Error::other("no data directory on this system — set DOCKET_HOME"))?
                .join("docket"),
        };
        fs::create_dir_all(&root).map_err(|e| Error::io("create", &root, e))?;
        Ok(Store { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn card_path(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.md"))
    }

    /// Every card, newest first, with cold ones (done, dead) pushed to the end.
    ///
    /// A card written before ids existed, or by hand, is given one here and
    /// the file is rewritten. It happens once per card and is the only time
    /// docket edits a card you did not ask it to.
    pub fn cards(&self) -> Result<Vec<Card>> {
        let mut cards = Vec::new();
        let entries = fs::read_dir(&self.root).map_err(|e| Error::io("read", &self.root, e))?;
        for entry in entries {
            let path = entry.map_err(|e| Error::io("read", &self.root, e))?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let body = fs::read_to_string(&path).map_err(|e| Error::io("read", &path, e))?;
            let mut card = Card::parse(name, &body, 0);
            card.age_days = age_days(&path, &card.path);
            cards.push(card);
        }
        self.backfill_ids(&mut cards)?;
        cards.sort_by(|a, b| {
            a.status
                .is_cold()
                .cmp(&b.status.is_cold())
                .then(a.age_days.cmp(&b.age_days))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(cards)
    }

    /// Give every card without an id one, and write it into the file.
    fn backfill_ids(&self, cards: &mut [Card]) -> Result<()> {
        let mut taken: BTreeSet<String> = cards
            .iter()
            .filter(|c| !c.id.is_empty())
            .map(|c| c.id.clone())
            .collect();

        for card in cards.iter_mut().filter(|c| c.id.is_empty()) {
            let fresh = id::generate(&card.name, &taken);
            taken.insert(fresh.clone());
            card.body = insert_id(&card.body, &fresh);
            card.id = fresh;
            self.write_quietly(&card.name, &card.body)?;
        }
        Ok(())
    }

    /// Every id currently in use, for handing out a new one.
    pub fn ids(&self) -> Result<BTreeSet<String>> {
        Ok(self.cards()?.into_iter().map(|c| c.id).collect())
    }

    /// Exact name first, then a unique prefix. Ambiguity is an error, never a
    /// guess — picking the wrong card silently is the one unforgivable bug.
    /// Resolution order: exact name, exact id, then a unique prefix of
    /// either. Exact matches come first so a card named like hex — `abed`,
    /// `face` — is still reachable by its name.
    pub fn get(&self, query: &str) -> Result<Card> {
        let cards = self.cards()?;
        if let Some(card) = cards.iter().find(|c| c.name == query) {
            return Ok(card.clone());
        }
        if let Some(card) = cards.iter().find(|c| c.id == query) {
            return Ok(card.clone());
        }

        let by_id = id::looks_like(query);
        let hits: Vec<&Card> = cards
            .iter()
            .filter(|c| c.name.starts_with(query) || (by_id && c.id.starts_with(query)))
            .collect();
        match hits.as_slice() {
            [only] => Ok((*only).clone()),
            [] => Err(Error::NoCard(query.to_string())),
            many => Err(Error::Ambiguous {
                query: query.to_string(),
                hits: many.iter().map(|c| c.name.clone()).collect(),
            }),
        }
    }

    pub fn exists(&self, name: &str) -> bool {
        self.card_path(name).exists()
    }

    pub fn write(&self, name: &str, body: &str) -> Result<()> {
        let path = self.card_path(name);
        fs::write(&path, body).map_err(|e| Error::io("write", &path, e))
    }

    /// A write that is docket's doing, not the user's: the file's mtime is put
    /// back afterwards so the age column still reports the user's last touch.
    pub fn write_quietly(&self, name: &str, body: &str) -> Result<()> {
        let path = self.card_path(name);
        let before = fs::metadata(&path).and_then(|m| m.modified()).ok();
        fs::write(&path, body).map_err(|e| Error::io("write", &path, e))?;
        if let Some(stamp) = before {
            let file = fs::File::options()
                .write(true)
                .open(&path)
                .map_err(|e| Error::io("reopen", &path, e))?;
            // Best effort: a filesystem that refuses is not worth an error.
            let _ = file.set_modified(stamp);
        }
        Ok(())
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.card_path(name);
        fs::remove_file(&path).map_err(|e| Error::io("delete", &path, e))
    }

}

/// Put `id:` directly under the `#` heading, or at the top when there is none.
fn insert_id(body: &str, id: &str) -> String {
    let mut lines: Vec<String> = body.lines().map(String::from).collect();
    let at = match lines.first() {
        Some(first) if first.starts_with("# ") => 1,
        _ => 0,
    };
    lines.insert(at, format!("id: {id}"));
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway store under the OS temp dir. No dev-dependency needed.
    fn scratch(tag: &str) -> Store {
        let root = env::temp_dir().join(format!("docket-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Store { root }
    }

    fn put(store: &Store, name: &str, status: &str) {
        store
            .write(name, &format!("# {name}\nstatus: {status}\nwhat: x\n"))
            .unwrap();
    }

    fn put_with_id(store: &Store, name: &str, id: &str) {
        store
            .write(name, &format!("# {name}\nid: {id}\nstatus: active\nwhat: x\n"))
            .unwrap();
    }

    #[test]
    fn a_card_without_an_id_is_given_one_and_rewritten() {
        let s = scratch("backfill");
        put(&s, "moxi", "active");

        let card = s.cards().unwrap().remove(0);
        assert_eq!(card.id.len(), crate::id::LENGTH);

        // Written to the file, and stable across reads.
        let again = s.cards().unwrap().remove(0);
        assert_eq!(again.id, card.id);
        assert!(s.card_path("moxi").exists());
        let body = std::fs::read_to_string(s.card_path("moxi")).unwrap();
        assert!(body.starts_with(&format!("# moxi\nid: {}", card.id)), "{body}");
    }

    #[test]
    fn backfilled_ids_do_not_collide() {
        let s = scratch("backfill-many");
        for name in ["a", "b", "c", "d"] {
            put(&s, name, "active");
        }
        let ids: std::collections::BTreeSet<String> =
            s.cards().unwrap().into_iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn cards_resolve_by_id_and_by_id_prefix() {
        let s = scratch("by-id");
        put_with_id(&s, "moxi", "a43b21c0");
        assert_eq!(s.get("a43b21c0").unwrap().name, "moxi");
        assert_eq!(s.get("a43b").unwrap().name, "moxi");
        assert!(matches!(s.get("ffff"), Err(Error::NoCard(_))));
    }

    #[test]
    fn an_exact_name_beats_an_id_prefix() {
        let s = scratch("name-wins");
        put_with_id(&s, "abed", "ffffffff");
        put_with_id(&s, "other", "abed0000");
        assert_eq!(s.get("abed").unwrap().name, "abed");
    }

    #[test]
    fn an_ambiguous_id_prefix_is_refused() {
        let s = scratch("id-clash");
        put_with_id(&s, "one", "a43b0000");
        put_with_id(&s, "two", "a43b1111");
        assert!(matches!(s.get("a43b"), Err(Error::Ambiguous { .. })));
    }

    #[test]
    fn resolves_exact_before_prefix() {
        let s = scratch("exact");
        put(&s, "mo", "idea");
        put(&s, "moxi", "active");
        assert_eq!(s.get("mo").unwrap().name, "mo");
        assert_eq!(s.get("moxi").unwrap().name, "moxi");
    }

    #[test]
    fn ambiguous_prefix_is_an_error() {
        let s = scratch("ambiguous");
        put(&s, "moxi", "active");
        put(&s, "morse", "idea");
        match s.get("mo") {
            Err(Error::Ambiguous { hits, .. }) => assert_eq!(hits.len(), 2),
            other => panic!("expected ambiguity, got {other:?}"),
        }
    }

    #[test]
    fn cold_cards_sort_last() {
        let s = scratch("sort");
        put(&s, "buried", "dead");
        put(&s, "live", "active");
        let names: Vec<String> = s.cards().unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["live", "buried"]);
    }


}
