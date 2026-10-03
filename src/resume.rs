//! `dk resume` — the one file that lets a fresh session pick a project back up.
//!
//! The contract is `docs/resume-contract.md`; this module is its reader. It
//! assembles three things, in the order of how hard each is to get back:
//!
//! 1. **the card** — your own sections, README excluded;
//! 2. **the sessions** — the newest three entries whole, older ones as one
//!    index line each, and every long-form document as one index line. These
//!    hold the *why*, which no other source does;
//! 3. **the code** — the index `ygg` makes of the project's `WHITE.md`: which
//!    files, how big. The files themselves are opened when a task needs them.
//!
//! It only reads. Session entries are written by whoever ends a session —
//! `dk save` (see `ledger.rs`), or a Cowork run or you by hand — into a fur
//! archive under `<store>/sessions/`. docket never calls fur.
//!
//! Nothing here fails quietly. A missing manifest, a missing `ygg`, a session
//! file that is gone — each becomes a bracketed line in the output, the same
//! way a moved README becomes `[no README at …]`. A brief that silently lost a
//! section is worse than one that says it did.

use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use crate::brief;
use crate::card::{Card, Place, MAIN_PLACE, README_HEADING};
use crate::error::{Error, Result};

pub const DEFAULT_FILE: &str = "RESUME.md";

/// Session entries written whole. Older ones shrink to an index line, so a long
/// history costs a line per session rather than a page.
const WHOLE: usize = 3;

const NO_SESSIONS: &str = "[no sessions yet]";

/// The finished file and what each part of it costs, in the same rough tokens
/// `dk out` reports.
pub struct Built {
    pub text: String,
    pub card: usize,
    pub sessions: usize,
    pub code: usize,
    pub total: usize,
}

/// `place` limits the code part to one of the card's places, by label.
pub fn build(card: &Card, store_root: &Path, place: Option<&str>) -> Result<Built> {
    let card_part = own_text(card);
    let sessions_part = sessions_section(card, store_root)?;
    let code_part = code_section(card, place)?;

    let text = format!(
        "<!-- dk:resume v1 -->\n# RESUME · {}\n\n## card\n\n{card_part}\n\n## sessions\n\n{sessions_part}\n\n## code\n\n{code_part}\n",
        card.name
    );
    Ok(Built {
        card: brief::tokens(&card_part),
        sessions: brief::tokens(&sessions_part),
        code: brief::tokens(&code_part),
        total: brief::tokens(&text),
        text,
    })
}

// ---------------------------------------------------------------------------
// the card
// ---------------------------------------------------------------------------

/// The header and the sections you wrote. The README is the project's text, not
/// yours, and it travels by being in `WHITE.md` if you want it.
fn own_text(card: &Card) -> String {
    card.body
        .lines()
        .take_while(|l| !l.trim_end().eq_ignore_ascii_case(README_HEADING))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// `white:` in the card's header, if it sets one.
fn white_field(card: &Card) -> Option<String> {
    card.header()
        .lines()
        .find_map(|l| l.strip_prefix("white:"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------------------
// the sessions
// ---------------------------------------------------------------------------

/// One conversation in the sessions archive, read whole.
pub(crate) struct Conversation {
    pub(crate) folder: String,
    pub(crate) dir: PathBuf,
    pub(crate) text: String,
}

/// One session entry: the file's stem and its text.
struct Entry {
    stem: String,
    text: String,
}

/// One long-form document: listed by name, never inlined. A plan or an option
/// analysis can be thousands of tokens, and the three-entry window would lose
/// its other two entries to it.
struct Doc {
    stem: String,
    title: String,
    status: String,
    tokens: usize,
}

/// What a linked file in the conversation turned out to be.
enum Linked {
    Session(Entry),
    Doc(Doc),
}

fn sessions_section(card: &Card, store_root: &Path) -> Result<String> {
    if card.id.is_empty() {
        return Ok(NO_SESSIONS.to_string());
    }
    let tag = format!("dk-{}", card.id);
    let found = conversations_tagged(&store_root.join("sessions").join("chats"), &tag)?;

    let conversation = match found.as_slice() {
        [] => return Ok(NO_SESSIONS.to_string()),
        [only] => only,
        many => {
            let names: Vec<&str> = many.iter().map(|c| c.folder.as_str()).collect();
            return Err(Error::other(format!(
                "{} conversations are tagged {tag}: {} — a card has exactly one, remove the tag from the others",
                many.len(),
                names.join(", ")
            )));
        }
    };

    let (linked, notes) = read_linked(conversation);
    let mut entries = Vec::new();
    let mut docs = Vec::new();
    for item in linked {
        match item {
            Linked::Session(entry) => entries.push(entry),
            Linked::Doc(doc) => docs.push(doc),
        }
    }
    let (older, whole) = entries.split_at(entries.len().saturating_sub(WHOLE));

    let mut out = String::new();
    if !notes.is_empty() {
        out.push_str(&notes.join("\n"));
        out.push_str("\n\n");
    }
    if whole.is_empty() {
        out.push_str(NO_SESSIONS);
    } else {
        let newest_first: Vec<&str> = whole.iter().rev().map(|e| e.text.trim_end()).collect();
        out.push_str(&newest_first.join("\n\n---\n\n"));
    }
    if !docs.is_empty() {
        out.push_str("\n\n### documents\n\n");
        let lines: Vec<String> = docs.iter().rev().map(doc_line).collect();
        out.push_str(&lines.join("\n"));
    }
    if !older.is_empty() {
        out.push_str("\n\n### earlier\n\n");
        let lines: Vec<String> = older.iter().rev().map(index_line).collect();
        out.push_str(&lines.join("\n"));
    }
    Ok(out)
}

/// Every conversation under `chats/` whose front matter carries `tag`, in folder
/// order. A missing `chats/` is simply no conversations.
pub(crate) fn conversations_tagged(chats: &Path, tag: &str) -> Result<Vec<Conversation>> {
    let listing = match fs::read_dir(chats) {
        Ok(listing) => listing,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::io("read", chats, e)),
    };
    let mut dirs: Vec<PathBuf> = listing
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.join("convo.md").is_file())
        .collect();
    dirs.sort();

    let mut found = Vec::new();
    for dir in dirs {
        let spine = dir.join("convo.md");
        let text = match fs::read_to_string(&spine) {
            Ok(text) => text,
            // Not UTF-8 is what an encrypted archive looks like. Reporting "no
            // sessions" would be a lie, so say what it probably is.
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                return Err(Error::other(format!(
                    "cannot read {} as text — if the sessions archive is locked, run `fur unlock` there",
                    spine.display()
                )));
            }
            Err(e) => return Err(Error::io("read", &spine, e)),
        };
        if front_matter_tags(&text).iter().any(|t| t == tag) {
            let folder = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            found.push(Conversation { folder, dir, text });
        }
    }
    Ok(found)
}

/// Session entries and documents in order (oldest first, as fur keeps them),
/// plus a note for every linked file that could not be used.
///
/// The first line of a linked file says what it is: `<!-- dk:session` or
/// `<!-- dk:doc`. Anything else — ordinary `fur jot` chatter in the same
/// conversation — is neither, and is passed over without comment.
fn read_linked(conversation: &Conversation) -> (Vec<Linked>, Vec<String>) {
    let mut entries = Vec::new();
    let mut notes = Vec::new();

    for line in conversation.text.lines() {
        let Some(attrs) = marker_attrs(line) else {
            continue;
        };
        let Some(link) = attrs.iter().find(|(k, _)| k == "link").map(|(_, v)| v.clone()) else {
            continue;
        };
        if !is_plain_relative(&link) {
            notes.push(format!("[skipped {link}: path leaves the conversation folder]"));
            continue;
        }
        let text = match fs::read_to_string(conversation.dir.join(&link)) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                notes.push(format!("[missing {link}: listed in convo.md but not on disk]"));
                continue;
            }
            Err(e) => {
                notes.push(format!("[unreadable {link}: {e}]"));
                continue;
            }
        };
        let first = text
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(str::trim_start)
            .unwrap_or("");
        let stem = Path::new(&link)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| link.clone());
        if first.starts_with("<!-- dk:session") {
            entries.push(Linked::Session(Entry { stem, text }));
        } else if first.starts_with("<!-- dk:doc") {
            let (title, status) = doc_header(&text);
            entries.push(Linked::Doc(Doc { stem, title, status, tokens: brief::tokens(&text) }));
        }
    }
    (entries, notes)
}

/// A document's `# title` and `status:` line, both from above its first `##`.
fn doc_header(text: &str) -> (String, String) {
    let mut title = String::new();
    let mut status = String::new();
    for line in text.lines().take_while(|l| !l.starts_with("## ")) {
        if title.is_empty() {
            if let Some(t) = line.strip_prefix("# ") {
                title = t.trim().to_string();
            }
        }
        if status.is_empty() {
            if let Some(v) = line.strip_prefix("status:") {
                status = v.trim().to_string();
            }
        }
    }
    if status.is_empty() {
        status = "-".to_string();
    }
    (title, status)
}

/// `- DOC-20261001-workflow-design · Unified workflow · adopted · 3.8k tok · 2026-10-01`
fn doc_line(doc: &Doc) -> String {
    let cost = if doc.tokens < 1_000 {
        format!("{} tok", doc.tokens)
    } else {
        format!("{:.1}k tok", doc.tokens as f64 / 1_000.0)
    };
    let date = date_of(&doc.stem);
    let parts: Vec<&str> = [doc.stem.as_str(), doc.title.as_str(), doc.status.as_str(), cost.as_str(), date.as_str()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    format!("- {}", parts.join(" · "))
}

/// `- SES-20260901-110233 · M1 parser spans · 2026-09-01`
fn index_line(entry: &Entry) -> String {
    let next = section_first_line(&entry.text, "next");
    let date = date_of(&entry.stem);
    let parts: Vec<&str> = [entry.stem.as_str(), next.as_str(), date.as_str()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    format!("- {}", parts.join(" · "))
}

/// First non-empty line under `## <name>`, without a list dash.
fn section_first_line(text: &str, name: &str) -> String {
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            if inside {
                break;
            }
            inside = heading.trim().eq_ignore_ascii_case(name);
            continue;
        }
        if inside && !trimmed.is_empty() {
            return trimmed.trim_start_matches("- ").to_string();
        }
    }
    String::new()
}

/// `SES-20260901-110233` / `DOC-20261001-slug` → the date; anything else → empty.
fn date_of(stem: &str) -> String {
    let digits: Vec<char> = stem
        .strip_prefix("SES-")
        .or_else(|| stem.strip_prefix("DOC-"))
        .unwrap_or("")
        .chars()
        .take(8)
        .collect();
    if digits.len() != 8 || !digits.iter().all(|c| c.is_ascii_digit()) {
        return String::new();
    }
    let part = |from: usize, to: usize| -> String {
        digits.get(from..to).map(|d| d.iter().collect()).unwrap_or_default()
    };
    format!("{}-{}-{}", part(0, 4), part(4, 6), part(6, 8))
}

/// A link may only name a file inside the conversation's own folder.
fn is_plain_relative(link: &str) -> bool {
    !link.is_empty()
        && Path::new(link)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

// ---------------------------------------------------------------------------
// fur's file format, as much of it as this needs
// ---------------------------------------------------------------------------

/// The `tags:` of a document's front matter. Reads both shapes fur writes —
/// a block sequence and the inline `[a, b]` / `[]` — and nothing else.
fn front_matter_tags(text: &str) -> Vec<String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Vec::new();
    }
    let mut tags = Vec::new();
    let mut in_tags = false;

    for line in lines {
        if line.trim_end() == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix("tags:") {
            let rest = rest.trim();
            in_tags = rest.is_empty();
            if !in_tags {
                tags.extend(
                    rest.trim_start_matches('[')
                        .trim_end_matches(']')
                        .split(',')
                        .map(|t| unquote(t.trim()))
                        .filter(|t| !t.is_empty()),
                );
            }
            continue;
        }
        if in_tags {
            match line.trim_start().strip_prefix('-') {
                Some(item) => tags.push(unquote(item.trim())),
                None => in_tags = false,
            }
        }
    }
    tags
}

fn unquote(raw: &str) -> String {
    for quote in ['"', '\''] {
        if raw.len() >= 2 && raw.starts_with(quote) && raw.ends_with(quote) {
            let inner = raw.get(1..raw.len() - 1).unwrap_or("");
            return inner.replace("\\\"", "\"").replace("\\\\", "\\");
        }
    }
    raw.to_string()
}

/// The `key=value` pairs of a `<!-- fur:msg … -->` line, or `None` if the line
/// is not one. Values are bare or double-quoted with backslash escapes, which is
/// how fur's own writer quotes them.
pub(crate) fn marker_attrs(line: &str) -> Option<Vec<(String, String)>> {
    let rest = line.trim().strip_prefix("<!-- fur:msg")?;
    let rest = rest.trim_end();
    let rest = rest.strip_suffix("-->").unwrap_or(rest);

    let mut chars = rest.chars().peekable();
    let mut attrs = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        if key.is_empty() {
            break;
        }
        if chars.peek() != Some(&'=') {
            continue;
        }
        chars.next();

        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(escaped) = chars.next() {
                            value.push(escaped);
                        }
                    }
                    '"' => break,
                    other => value.push(other),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        attrs.push((key, value));
    }
    Some(attrs)
}

// ---------------------------------------------------------------------------
// the code
// ---------------------------------------------------------------------------

/// The `## code` section: ygg's index of each place's manifest files (not their
/// contents), or a bracketed reason there isn't one.
///
/// A card with at most one place reads exactly as it always did. With several,
/// each place gets a `### <label> · <path>` heading so a reader can tell the
/// repo from its issue archive; `only` keeps just the one asked for.
fn code_section(card: &Card, only: Option<&str>) -> Result<String> {
    let all = card.all_places();
    if all.is_empty() {
        return Ok("[no project path — this card is an idea]".to_string());
    }
    let chosen: Vec<&Place> = match only {
        None => all.iter().collect(),
        Some(label) => {
            let hit: Vec<&Place> = all.iter().filter(|p| p.label == label).collect();
            if hit.is_empty() {
                let known: Vec<&str> = all.iter().map(|p| p.label.as_str()).collect();
                return Err(Error::usage(format!(
                    "`{}` has no place `{label}` — its places are {}",
                    card.name,
                    known.join(", ")
                )));
            }
            hit
        }
    };
    if all.len() == 1 {
        return Ok(code_of(&all[0].path, white_field(card)));
    }
    let parts: Vec<String> = chosen
        .iter()
        .map(|p| {
            // `white:` names the main place's manifest; the others use their own WHITE.md.
            let white = if p.label == MAIN_PLACE { white_field(card) } else { None };
            format!("### {} · {}\n\n{}", p.label, p.path, code_of(&p.path, white))
        })
        .collect();
    Ok(parts.join("\n\n"))
}

fn code_of(path: &str, white: Option<String>) -> String {
    let project = Path::new(path);
    if !project.is_dir() {
        return format!("[project path is gone: {path}]");
    }
    let manifest = match white {
        Some(custom) => {
            let custom = PathBuf::from(custom);
            if custom.is_absolute() {
                custom
            } else {
                project.join(custom)
            }
        }
        None => project.join("WHITE.md"),
    };
    if !manifest.is_file() {
        return format!("[no WHITE.md at {}]", manifest.display());
    }
    run_ygg(project, &manifest)
}

/// `ygg --white <manifest> --out <temp>.md`, run from the project so
/// its `.gitignore` and relative paths behave as they do when you run it. stdin
/// is closed so an unexpected prompt can never hang a resume.
fn run_ygg(project: &Path, manifest: &Path) -> String {
    let temp = env::temp_dir().join(format!("dk-resume-{}.md", std::process::id()));

    let outcome = Command::new("ygg")
        .current_dir(project)
        .arg("--white")
        .arg(manifest)
        .arg("--out")
        .arg(&temp)
        .stdin(Stdio::null())
        .output();

    let text = match outcome {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            "[ygg not found — install yggdrasil-cli]".to_string()
        }
        Err(e) => format!("[ygg could not start: {e}]"),
        Ok(out) if !out.status.success() => {
            let said = first_line(&out.stderr)
                .or_else(|| first_line(&out.stdout))
                .unwrap_or_else(|| format!("exit status {}", out.status));
            format!("[ygg failed: {said}]")
        }
        Ok(_) => match fs::read_to_string(&temp) {
            Ok(codex) => codex.trim_end().to_string(),
            Err(e) => format!("[ygg ran but wrote nothing readable: {e}]"),
        },
    };
    let _ = fs::remove_file(&temp);
    text
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_read_from_a_block_sequence() {
        let doc = "---\nfur_schema: 1\ntitle: x\ntags:\n  - dk-a43b21c0\n  - session\nparents: []\n---\n\ntags:\n  - not-me\n";
        assert_eq!(front_matter_tags(doc), vec!["dk-a43b21c0", "session"]);
    }

    #[test]
    fn tags_are_read_inline_empty_and_quoted() {
        assert_eq!(front_matter_tags("---\ntags: [a, \"b c\"]\n---\n"), vec!["a", "b c"]);
        assert!(front_matter_tags("---\ntags: []\n---\n").is_empty());
        assert_eq!(front_matter_tags("---\ntags:\n  - \"dk-1\"\n---\n"), vec!["dk-1"]);
    }

    #[test]
    fn a_document_without_front_matter_has_no_tags() {
        assert!(front_matter_tags("tags:\n  - dk-a43b21c0\n").is_empty());
        assert!(front_matter_tags("").is_empty());
    }

    #[test]
    fn markers_yield_their_attributes() {
        let line = "<!-- fur:msg id=b27 avatar=claude ts=2026-09-30T22:14:00Z link=SES-20260930-221400.md sha256=ab12 -->";
        let attrs = marker_attrs(line).expect("a marker");
        let get = |k: &str| attrs.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
        assert_eq!(get("link"), Some("SES-20260930-221400.md"));
        assert_eq!(get("avatar"), Some("claude"));
        assert_eq!(get("sha256"), Some("ab12"));
    }

    #[test]
    fn quoted_attribute_values_keep_spaces_and_escapes() {
        let attrs = marker_attrs(r#"<!-- fur:msg id=1 link="my notes \"v2\".md" avatar=a -->"#).expect("a marker");
        let link = attrs.iter().find(|(k, _)| k == "link").map(|(_, v)| v.as_str());
        assert_eq!(link, Some(r#"my notes "v2".md"#));
        assert!(attrs.iter().any(|(k, v)| k == "avatar" && v == "a"));
    }

    #[test]
    fn only_markers_are_markers() {
        assert!(marker_attrs("hello").is_none());
        assert!(marker_attrs("\\<!-- fur:msg id=1 -->").is_none(), "an escaped body line");
        assert!(marker_attrs("<!-- dk:session v1 -->").is_none());
    }

    #[test]
    fn the_index_quotes_the_first_line_of_next() {
        let text = "<!-- dk:session v1 -->\n# x\n\n## next\n\n- M2 finish · Sonnet 5\nmore\n\n## files\nnone\n";
        assert_eq!(section_first_line(text, "next"), "M2 finish · Sonnet 5");
        assert_eq!(section_first_line(text, "files"), "none");
        assert_eq!(section_first_line(text, "absent"), "");
    }

    #[test]
    fn an_empty_section_does_not_borrow_the_next_ones_text() {
        let text = "## next\n\n## files\nnone\n";
        assert_eq!(section_first_line(text, "next"), "");
    }

    #[test]
    fn dates_come_from_the_file_name() {
        assert_eq!(date_of("SES-20260901-110233"), "2026-09-01");
        assert_eq!(date_of("NOTE"), "");
        assert_eq!(date_of("SES-2026"), "");
    }

    #[test]
    fn documents_are_dated_too() {
        assert_eq!(date_of("DOC-20261001-workflow-design"), "2026-10-01");
    }

    #[test]
    fn a_document_header_gives_title_and_status() {
        let text = "<!-- dk:doc v1 -->\n# Unified workflow\nstatus: adopted\n\n## body\nstatus: not this\n";
        assert_eq!(doc_header(text), ("Unified workflow".to_string(), "adopted".to_string()));
        let bare = "<!-- dk:doc v1 -->\n# Plan\n\n## body\nstatus: not this\n";
        assert_eq!(doc_header(bare), ("Plan".to_string(), "-".to_string()), "status is never borrowed from the body");
    }

    #[test]
    fn links_may_not_leave_the_folder() {
        assert!(is_plain_relative("SES-1.md"));
        assert!(is_plain_relative("sub/SES-1.md"));
        assert!(!is_plain_relative("../x.md"));
        assert!(!is_plain_relative("a/../../x.md"));
        assert!(!is_plain_relative("/etc/passwd"));
        assert!(!is_plain_relative(""));
    }

    #[test]
    fn the_card_part_stops_before_the_readme() {
        let card = Card::parse(
            "x",
            "# x\nid: a\n\n## now\nmine\n\n## readme\n\ntheirs\n",
            0,
        );
        let own = own_text(&card);
        assert!(own.contains("mine"));
        assert!(!own.contains("theirs"));
        assert!(!own.contains("## readme"));
    }

    #[test]
    fn the_white_field_is_read_from_the_header_only() {
        let card = Card::parse("x", "# x\nid: a\nwhite: alt.txt\n\n## now\nwhite: nope\n", 0);
        assert_eq!(white_field(&card).as_deref(), Some("alt.txt"));
        let card = Card::parse("x", "# x\nid: a\n\n## now\nwhite: nope\n", 0);
        assert_eq!(white_field(&card), None);
    }
}
