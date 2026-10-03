//! `dk save` — write a session entry (and any documents) into the card's fur
//! conversation, the way the contract says, so whoever ends a session does not
//! have to: find the conversation by its `dk-<id>` tag or create it, name the
//! files, append one marker per new file, and check it all from disk.
//!
//! The format is the one `resume.rs` reads; the two stay in this crate so they
//! cannot drift. fur itself is never called: its archive is plain markdown, and
//! `.fur/` is only an index fur rebuilds on its own.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::card::{slug, Card};
use crate::error::{Error, Result};
use crate::resume::{conversations_tagged, marker_attrs};

pub const ENTRY_MARK: &str = "<!-- dk:session v1 -->";
pub const DOC_MARK: &str = "<!-- dk:doc v1 -->";

/// The seven `##` sections every entry carries, in this order. With the `#`
/// title line they are the contract's "eight headings".
pub const SECTIONS: [&str; 7] = ["done", "decided", "rejected", "state", "blockers", "next", "files"];

/// A UTC instant, to the second. Enough calendar to name files and write
/// RFC 3339 without a date crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub y: i64,
    pub mo: u32,
    pub d: u32,
    pub h: u32,
    pub mi: u32,
    pub s: u32,
}

impl Stamp {
    pub fn now() -> Stamp {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        Stamp::from_unix(secs as i64)
    }

    pub fn from_unix(secs: i64) -> Stamp {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = yoe + era * 400 + i64::from(mo <= 2);
        Stamp { y, mo, d, h: (rem / 3600) as u32, mi: (rem % 3600 / 60) as u32, s: (rem % 60) as u32 }
    }

    fn plus_seconds(self, n: i64) -> Stamp {
        Stamp::from_unix(self.unix() + n)
    }

    fn unix(self) -> i64 {
        // days_from_civil, the inverse of the above.
        let y = self.y - i64::from(self.mo <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.mo);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.d) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        days * 86_400 + i64::from(self.h) * 3600 + i64::from(self.mi) * 60 + i64::from(self.s)
    }

    pub fn date(self) -> String {
        format!("{:04}{:02}{:02}", self.y, self.mo, self.d)
    }

    pub fn file_stamp(self) -> String {
        format!("{}-{:02}{:02}{:02}", self.date(), self.h, self.mi, self.s)
    }

    pub fn rfc3339(self) -> String {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", self.y, self.mo, self.d, self.h, self.mi, self.s)
    }
}

/// A random UUID v4. Not cryptographic, and does not need to be: it only has
/// to be unique among one archive's markers. `RandomState` is seeded from the
/// OS once per process, so no dependency is needed for it.
pub fn uuid_v4() -> String {
    let state = RandomState::new();
    let mut a = state.build_hasher();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    a.write_u128(nanos);
    a.write_u32(std::process::id());
    let x = a.finish();
    let mut b = state.build_hasher();
    b.write_u64(x);
    b.write_u8(0x5a);
    let y = b.finish();

    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&x.to_be_bytes());
    bytes[8..].copy_from_slice(&y.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The entry as it will be written: the marker line first (added if the writer
/// left it off), a `# ` title, and the seven sections in order. Anything
/// missing is an error naming what to add — a half-shaped entry would be read
/// back wrong by every later resume.
pub fn check_entry(text: &str) -> Result<String> {
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return Err(Error::usage("the entry is empty"));
    }
    let body = text.strip_prefix(ENTRY_MARK).map(str::trim_start).unwrap_or(text);
    let mut lines = body.lines();
    if !lines.next().is_some_and(|l| l.starts_with("# ")) {
        return Err(Error::usage(
            "an entry starts with its title: `# <card> · <YYYY-MM-DD HH:MM> · <harness> · <model> <effort>`",
        ));
    }
    let headings: Vec<String> = body
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let missing: Vec<&str> = SECTIONS.iter().copied().filter(|s| !headings.iter().any(|h| h == s)).collect();
    if !missing.is_empty() {
        return Err(Error::usage(format!(
            "the entry is missing `## {}` — every entry has all of: {} (an empty one says `none`)",
            missing.join("`, `## "),
            SECTIONS.join(", ")
        )));
    }
    let order: Vec<usize> = SECTIONS
        .iter()
        .filter_map(|s| headings.iter().position(|h| h == s))
        .collect();
    if order.windows(2).any(|w| w[0] > w[1]) {
        return Err(Error::usage(format!("the entry's sections must come in this order: {}", SECTIONS.join(", "))));
    }
    Ok(format!("{ENTRY_MARK}\n{}\n", body.trim_end()))
}

/// A document's file name and text. A name already shaped `DOC-YYYYMMDD-slug.md`
/// is kept (that is how a document is revised in place); otherwise the name is
/// made from today's date and the `# ` title.
pub fn check_doc(hint: &str, text: &str, today: &str) -> Result<(String, String)> {
    let text = text.trim_start_matches('\u{feff}').trim();
    let body = text.strip_prefix(DOC_MARK).map(str::trim_start).unwrap_or(text);
    let title = body
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("# "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| Error::usage(format!("{hint}: a document starts with `# <title>`")))?;
    if !body.lines().any(|l| l.trim_start().starts_with("status:")) {
        return Err(Error::usage(format!(
            "{hint}: a document needs a `status: draft | adopted | superseded` line under its title"
        )));
    }
    let base = Path::new(hint).file_name().and_then(|n| n.to_str()).unwrap_or("");
    let name = if is_doc_name(base) {
        base.to_string()
    } else {
        let s = slug(title);
        let s: String = s.chars().take(48).collect::<String>().trim_end_matches('-').to_string();
        format!("DOC-{today}-{}.md", if s.is_empty() { "doc".into() } else { s })
    };
    Ok((name, format!("{DOC_MARK}\n{}\n", body.trim_end())))
}

fn is_doc_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("DOC-").and_then(|r| r.strip_suffix(".md")) else {
        return false;
    };
    let (date, slug) = rest.split_at(rest.len().min(8));
    date.len() == 8
        && date.chars().all(|c| c.is_ascii_digit())
        && slug.starts_with('-')
        && slug.len() > 1
        && slug[1..].chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// What `save` did, for the report and for tests.
#[derive(Debug)]
pub struct Saved {
    pub folder: PathBuf,
    pub created: bool,
    pub entry: PathBuf,
    /// Each document and whether it replaced one already in the conversation.
    pub docs: Vec<(PathBuf, bool)>,
}

/// Write `docs` then `entry` into the conversation tagged `dk-<card id>`,
/// creating it if there is none, and append a marker for every new file.
/// Every file is read back afterwards; a mismatch is an error, not a warning.
pub fn save(store_root: &Path, card: &Card, entry: &str, docs: &[(String, String)], now: Stamp, dry_run: bool) -> Result<Saved> {
    if card.id.is_empty() {
        return Err(Error::other(format!("`{}` has no id yet — run `dk show {}` once and try again", card.name, card.name)));
    }
    let entry = check_entry(entry)?;
    let docs: Vec<(String, String)> = docs
        .iter()
        .map(|(hint, text)| check_doc(hint, text, &now.date()))
        .collect::<Result<_>>()?;

    let chats = store_root.join("sessions").join("chats");
    let tag = format!("dk-{}", card.id);
    let found = conversations_tagged(&chats, &tag)?;
    let (folder, mut spine, created) = match found.as_slice() {
        [] => {
            let id = uuid_v4();
            let folder = chats.join(format!("{}-sessions-{}", slug(&card.name), &id[..8]));
            let spine = format!(
                "---\nfur_schema: 1\nconversation_id: {id}\ntitle: {} sessions\ncreated_at: {}\ntags:\n  - {tag}\n  - session\n---\n",
                card.name,
                now.rfc3339()
            );
            (folder, spine, true)
        }
        [only] => (only.dir.clone(), only.text.clone(), false),
        many => {
            let names: Vec<&str> = many.iter().map(|c| c.folder.as_str()).collect();
            return Err(Error::other(format!(
                "{} conversations are tagged {tag}: {} — a card has exactly one; remove the tag from the others",
                many.len(),
                names.join(", ")
            )));
        }
    };

    let linked: Vec<String> = spine
        .lines()
        .filter_map(marker_attrs)
        .filter_map(|attrs| attrs.into_iter().find(|(k, _)| k == "link").map(|(_, v)| v))
        .collect();

    // A second save in the same second must not overwrite the first.
    let mut stamp = now;
    let entry_name = loop {
        let name = format!("SES-{}.md", stamp.file_stamp());
        if !folder.join(&name).exists() && !linked.contains(&name) {
            break name;
        }
        stamp = stamp.plus_seconds(1);
    };

    let mut writes: Vec<(PathBuf, String)> = Vec::new();
    let mut new_links: Vec<String> = Vec::new();
    let mut doc_report = Vec::new();
    for (name, text) in &docs {
        let path = folder.join(name);
        let revised = linked.contains(name);
        if !revised {
            new_links.push(name.clone());
        }
        writes.push((path.clone(), text.clone()));
        doc_report.push((path, revised));
    }
    let entry_path = folder.join(&entry_name);
    writes.push((entry_path.clone(), entry));
    new_links.push(entry_name);

    for link in &new_links {
        if !spine.ends_with('\n') {
            spine.push('\n');
        }
        spine.push_str(&format!(
            "\n<!-- fur:msg id={} avatar=claude ts={} link={link} -->\n",
            uuid_v4(),
            now.rfc3339()
        ));
    }

    let saved = Saved { folder: folder.clone(), created, entry: entry_path, docs: doc_report };
    if dry_run {
        return Ok(saved);
    }

    fs::create_dir_all(&folder).map_err(|e| Error::io("create", &folder, e))?;
    for (path, text) in &writes {
        fs::write(path, text).map_err(|e| Error::io("write", path, e))?;
    }
    let spine_path = folder.join("convo.md");
    fs::write(&spine_path, &spine).map_err(|e| Error::io("write", &spine_path, e))?;

    // Read everything back. The tool saying "written" is not the same as the
    // bytes being there, and the contract asks for the check.
    for (path, text) in writes.iter().chain(std::iter::once(&(spine_path.clone(), spine.clone()))) {
        let back = fs::read_to_string(path).map_err(|e| Error::io("read back", path, e))?;
        if &back != text {
            return Err(Error::other(format!("{} did not read back as written", path.display())));
        }
    }
    let tail: Vec<String> = spine
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(new_links.len())
        .filter_map(marker_attrs)
        .filter_map(|a| a.into_iter().find(|(k, _)| k == "link").map(|(_, v)| v))
        .collect();
    if tail.iter().rev().cloned().collect::<Vec<_>>() != new_links {
        return Err(Error::other(format!("the new markers are not the last lines of {}", spine_path.display())));
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "# moxi · 2026-10-03 17:49 · Cowork · opus high\n\n## done\n- a\n\n## decided\nnone\n\n## rejected\nnone\n\n## state\nP0 · done\n\n## blockers\nnone\n\n## next\nP1 · sonnet medium · tests\n\n## files\nnone\n";

    #[test]
    fn calendar_round_trips() {
        let s = Stamp::from_unix(1_791_049_759); // 2026-10-03T17:49:19Z
        assert_eq!(s.rfc3339(), "2026-10-03T17:49:19Z");
        assert_eq!(s.file_stamp(), "20261003-174919");
        assert_eq!(s.unix(), 1_791_049_759);
        assert_eq!(Stamp::from_unix(951_782_400).rfc3339(), "2000-02-29T00:00:00Z");
        assert_eq!(Stamp::from_unix(0).plus_seconds(86_399).rfc3339(), "1970-01-01T23:59:59Z");
    }

    #[test]
    fn uuids_are_v4_shaped_and_distinct() {
        let a = uuid_v4();
        let b = uuid_v4();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
        assert!(matches!(a.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }

    #[test]
    fn an_entry_gets_its_marker_and_must_have_every_section_in_order() {
        let ok = check_entry(ENTRY).unwrap();
        assert!(ok.starts_with("<!-- dk:session v1 -->\n# moxi"));
        assert_eq!(check_entry(&ok).unwrap(), ok, "idempotent");
        assert!(check_entry(&ENTRY.replace("## blockers\nnone\n\n", "")).is_err());
        assert!(check_entry(&ENTRY.replace("# moxi", "moxi")).is_err());
        let swapped = ENTRY.replace("## done", "## TMP").replace("## files", "## done").replace("## TMP", "## files");
        assert!(check_entry(&swapped).is_err());
    }

    #[test]
    fn documents_keep_a_doc_name_or_get_one_from_the_title() {
        let doc = "# Asset-driven plan: v2!\nstatus: draft\n\ntext";
        let (name, text) = check_doc("/tmp/plan.md", doc, "20261003").unwrap();
        assert_eq!(name, "DOC-20261003-asset-driven-plan-v2.md");
        assert!(text.starts_with("<!-- dk:doc v1 -->\n# Asset"));
        let (kept, _) = check_doc("x/DOC-20260101-old-plan.md", doc, "20261003").unwrap();
        assert_eq!(kept, "DOC-20260101-old-plan.md");
        assert!(check_doc("p.md", "# t\nno status", "20261003").is_err());
    }
}
