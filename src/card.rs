use std::fmt;

use crate::outline::{self, Outline};
use std::path::Path;
use std::time::SystemTime;

/// The heading the full README is filed under, always last in a card so the
/// short, hand-written sections stay at the top where they are read.
pub const README_HEADING: &str = "## readme";

/// A card is a markdown file. Everything above the first `##` heading is a
/// block of `key: value` lines; everything below belongs to the user and is
/// passed through untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Eight hex characters, fixed for the life of the card. See `id.rs`.
    pub id: String,
    pub name: String,
    pub status: Status,
    pub what: String,
    pub path: String,
    /// Further folders the project lives in, from `place: <label> <path>`
    /// lines. `path:` stays the primary place; see `all_places`.
    pub places: Vec<Place>,
    pub agents: String,
    /// Absolute path of the project's README, if it has one. The card links
    /// to it rather than holding a copy: a copy goes stale, and an AI asked
    /// to revise the card would happily rewrite the README along with it.
    pub readme: String,
    /// The file as written, including the header.
    pub body: String,
    /// Days since the file was last modified.
    pub age_days: u64,
}

/// The label `path:` goes by when a card has several places.
pub const MAIN_PLACE: &str = "main";

/// One folder a project lives in: the repo, its issue archive, an eval harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub label: String,
    pub path: String,
}

/// `place: <label> <path>`: the label is one word, the path is the rest of the
/// line, with a leading `~/` meaning the home folder. A line with no path is
/// not a place.
fn parse_place(value: &str) -> Option<Place> {
    let value = value.trim();
    let (label, path) = value.split_once(char::is_whitespace)?;
    let path = path.trim();
    if label.is_empty() || path.is_empty() {
        return None;
    }
    Some(Place { label: label.to_string(), path: expand_home(path) })
}

fn expand_home(path: &str) -> String {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest).display().to_string(),
        _ => path.to_string(),
    }
}

/// A card's status. The words docket knows about get a colour and a place in
/// the sort order; any other word is kept and shown as written.
///
/// There is no fixed vocabulary here on purpose. `stable`, `shipped`,
/// `blocked`, `abandoned` are all things a project can be, and a tool that
/// prints `-` because it has not heard of your word is telling you its
/// opinion matters more than your note.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Active,
    Idea,
    Paused,
    Done,
    Dead,
    /// Whatever the user wrote, verbatim.
    Other(String),
    /// No `status:` line at all.
    Unset,
}

impl Status {
    pub fn parse(s: &str) -> Status {
        let word = s.trim();
        match word.to_ascii_lowercase().as_str() {
            "" => Status::Unset,
            "active" => Status::Active,
            "idea" => Status::Idea,
            "paused" => Status::Paused,
            "done" => Status::Done,
            "dead" => Status::Dead,
            _ => Status::Other(word.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Status::Active => "active",
            Status::Idea => "idea",
            Status::Paused => "paused",
            Status::Done => "done",
            Status::Dead => "dead",
            Status::Other(word) => word,
            Status::Unset => "-",
        }
    }

    /// Finished work sorts last and prints dim.
    pub fn is_cold(&self) -> bool {
        matches!(self, Status::Done | Status::Dead)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Card {
    /// `name` comes from the filename, not the `#` heading, so a mangled
    /// heading can never move a card.
    pub fn parse(name: &str, body: &str, age_days: u64) -> Card {
        let header: Vec<&str> = body.lines().take_while(|l| !l.starts_with("## ")).collect();
        let field = |key: &str| -> String {
            header
                .iter()
                .find_map(|l| l.strip_prefix(key))
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        Card {
            id: field("id:"),
            name: name.to_string(),
            status: Status::parse(&field("status:")),
            what: field("what:"),
            path: field("path:"),
            places: header
                .iter()
                .filter_map(|l| l.strip_prefix("place:"))
                .filter_map(parse_place)
                .collect(),
            agents: field("agents:"),
            readme: field("readme:"),
            body: body.to_string(),
            age_days,
        }
    }

    /// Every folder the project lives in, `path:` first as `main`.
    pub fn all_places(&self) -> Vec<Place> {
        let mut all = Vec::new();
        if !self.path.is_empty() {
            all.push(Place { label: MAIN_PLACE.to_string(), path: self.path.clone() });
        }
        all.extend(self.places.iter().cloned());
        all
    }

    /// The card with any embedded `## readme` section removed and a
    /// `readme:` field added. Used once per card, to migrate stores written
    /// before the README became a link.
    pub fn link_readme(&self, path: &str) -> String {
        let kept: String = self
            .body
            .lines()
            .take_while(|l| !l.trim_end().eq_ignore_ascii_case(README_HEADING))
            .collect::<Vec<_>>()
            .join("\n");

        let mut out = String::new();
        let mut placed = false;
        for line in kept.trim_end().lines() {
            out.push_str(line);
            out.push('\n');
            // Straight after `path:`, where it reads as the pair it is.
            if !placed && line.starts_with("path:") {
                out.push_str(&format!("readme: {path}\n"));
                placed = true;
            }
        }
        if !placed {
            out.insert_str(0, &format!("readme: {path}\n"));
        }
        out
    }

    /// The shortest prefix of this id that no other id in `others` shares.
    /// Four characters unless the store is unusually crowded.
    pub fn short_id(&self, others: &[String]) -> String {
        for length in 4..=self.id.len() {
            let prefix = &self.id[..length];
            let clashes = others
                .iter()
                .any(|other| other != &self.id && other.starts_with(prefix));
            if !clashes {
                return prefix.to_string();
            }
        }
        self.id.clone()
    }


    /// Everything above the first `##` heading: the title and the field
    /// lines. Written out with any selection, because a section with no card
    /// around it is unreadable.
    pub fn header(&self) -> String {
        self.body
            .lines()
            .take_while(|l| !l.trim_start().starts_with("## "))
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string()
    }

    /// The card's own headings, as written in the card file.
    pub fn outline(&self) -> Outline {
        outline::parse(&self.body)
    }

    /// The card's headings with the linked README grafted on as a final
    /// `## readme` section. This is what the picker shows and what `out`,
    /// `pick` and `show` write — the README is read at that moment, so it is
    /// never stale and never duplicated into the card.
    ///
    /// A README that has moved or been deleted becomes a one-line note rather
    /// than a silent omission: a brief that quietly lost a document is worse
    /// than one that says the document is missing.
    pub fn full_outline(&self) -> Outline {
        let mut tree = self.outline();
        if self.readme.is_empty() {
            return tree;
        }
        match self.readme_text() {
            Some(text) => {
                tree.graft(README_HEADING, Vec::new(), outline::parse_document(&text));
            }
            None => {
                tree.graft(
                    README_HEADING,
                    vec![format!("[no README at {}]", self.readme)],
                    Outline::default(),
                );
            }
        }
        tree
    }

    /// The linked README's text, read now.
    pub fn readme_text(&self) -> Option<String> {
        if self.readme.is_empty() {
            return None;
        }
        std::fs::read_to_string(&self.readme).ok()
    }

    /// The card's own sections, README excluded — it is the project's text,
    /// not yours, and completion is about what you wrote.
    pub fn sections(&self) -> Vec<(String, Vec<String>)> {
        let outline = self.outline();
        outline
            .roots
            .iter()
            .map(|at| &outline.nodes[*at])
            .take_while(|node| !node.heading.eq_ignore_ascii_case(README_HEADING))
            .map(|node| (node.heading.clone(), node.lines.clone()))
            .collect()
    }

    /// `(filled, total)` sections. A section counts as filled when it holds
    /// anything but blank lines — the question a card answers is "have I
    /// written this down yet", and an empty `## next` means no.
    pub fn completion(&self) -> (usize, usize) {
        let sections = self.sections();
        let filled = sections
            .iter()
            .filter(|(_, lines)| lines.iter().any(|l| !l.trim().is_empty()))
            .count();
        (filled, sections.len())
    }

    /// `(done, total)` checkboxes in the hand-written sections. The README is
    /// excluded: a project's own checklist is not yours.
    pub fn progress(&self) -> (usize, usize) {
        let own: String = self
            .body
            .lines()
            .take_while(|l| !l.trim_end().eq_ignore_ascii_case(README_HEADING))
            .collect::<Vec<_>>()
            .join("\n");
        crate::checkbox::tally(&own)
    }


    /// The starting text of a new card. The README goes last, under its own
    /// heading, because a card is read top-down and the hand-written notes are
    /// what you came for.
    #[allow(clippy::too_many_arguments)]
    pub fn template(
        id: &str,
        name: &str,
        status: Status,
        what: &str,
        path: &str,
        agents: &str,
        readme: &str,
    ) -> String {
        let mut s = format!("# {name}\nid: {id}\nstatus: {status}\nwhat: {what}\n");
        if !path.is_empty() {
            s.push_str(&format!("path: {path}\n"));
        }
        if !readme.is_empty() {
            s.push_str(&format!("readme: {readme}\n"));
        }
        if !agents.is_empty() {
            s.push_str(&format!("agents: {agents}\n"));
        }
        s.push_str("\n## now\n\n## next\n\n## open questions\n\n## notes\n");
        s
    }
}

/// Days since the project was last touched.
///
/// For a project with a git repository, that is the time of the last commit
/// or staged change — `.git/index` is rewritten by `add`, `commit` and
/// `checkout`, which is exactly "when did I last work on this" without
/// spawning git. For anything else it is the card file's own mtime. Docket's
/// own rewrites (id backfill, readme linking) preserve the card's mtime so
/// they never make a forgotten project look fresh.
///
/// Unreadable metadata means zero rather than an error: an age column is
/// never worth failing a command over.
pub fn age_days(card_file: &Path, project: &str) -> u64 {
    let project_touch = if project.is_empty() {
        None
    } else {
        let git_index = Path::new(project).join(".git").join("index");
        modified(&git_index)
    };
    let stamp = project_touch.or_else(|| modified(card_file));
    stamp
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Lowercase ASCII, runs of anything else collapsed to a single dash.
pub fn slug(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_safe_filenames() {
        assert_eq!(slug("Kingdom of Loathing clone"), "kingdom-of-loathing-clone");
        assert_eq!(slug("  MoXi!!  "), "moxi");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("???"), "");
    }

    #[test]
    fn parses_header_fields_and_stops_at_headings() {
        let body = "# moxi\nid: a43b21c0\nstatus: Active\nwhat: a language\npath: /home/a/moxi\n\n## now\nstatus: not a field\n";
        let c = Card::parse("moxi", body, 3);
        assert_eq!(c.id, "a43b21c0");
        assert_eq!(c.status, Status::Active);
        assert_eq!(c.what, "a language");
        assert_eq!(c.path, "/home/a/moxi");
        assert!(c.places.is_empty());
        assert_eq!(c.age_days, 3);
    }

    #[test]
    fn an_unknown_status_is_kept_as_written() {
        let c = Card::parse("flashcall", "# flashcall\nid: a\nstatus: stable\n", 0);
        assert_eq!(c.status, Status::Other("stable".into()));
        assert_eq!(c.status.as_str(), "stable");
        assert!(!c.status.is_cold());
    }

    #[test]
    fn a_missing_status_reads_as_unset() {
        let c = Card::parse("x", "# x\nid: a\n", 0);
        assert_eq!(c.status, Status::Unset);
        assert_eq!(c.status.as_str(), "-");
    }

    #[test]
    fn known_words_are_recognised_whatever_their_case() {
        assert_eq!(Status::parse("Active"), Status::Active);
        assert_eq!(Status::parse("  DEAD "), Status::Dead);
    }

    #[test]
    fn filename_wins_over_heading() {
        assert_eq!(Card::parse("real", "# other\n", 0).name, "real");
    }

    #[test]
    fn short_ids_grow_only_when_they_must() {
        let card = Card::parse("moxi", "# moxi\nid: a43b21c0\n", 0);
        assert_eq!(card.short_id(&["a43b21c0".into(), "ffffffff".into()]), "a43b");
        assert_eq!(card.short_id(&["a43b21c0".into(), "a43b9999".into()]), "a43b2");
    }










    #[test]
    fn completion_counts_sections_with_something_in_them() {
        let c = Card::parse(
            "x",
            "# x\nid: a\n\n## now\nparser\n\n## next\n\n## open questions\n   \n\n## notes\nmine\n",
            0,
        );
        assert_eq!(c.completion(), (2, 4));
        assert_eq!(Card::parse("y", "# y\nid: b\n", 0).completion(), (0, 0));
    }

    #[test]
    fn progress_counts_only_the_users_own_boxes() {
        let c = Card::parse(
            "x",
            "# x\nid: a\n\n## now\n[ ] a\n[x] b\n\n## readme\n[ ] theirs\n[ ] also theirs\n",
            0,
        );
        assert_eq!(c.progress(), (1, 2));
    }


    #[test]
    fn places_come_from_the_header_and_follow_path() {
        let body = "# m\nid: a\npath: /p/main\nplace: issues /p/issues archive\nplace: eval /p/eval\nplace: broken\n\n## now\nplace: nope /x\n";
        let c = Card::parse("m", body, 0);
        let labels: Vec<_> = c.all_places().iter().map(|p| (p.label.clone(), p.path.clone())).collect();
        assert_eq!(
            labels,
            vec![
                ("main".into(), "/p/main".into()),
                ("issues".into(), "/p/issues archive".into()),
                ("eval".into(), "/p/eval".into()),
            ]
        );
    }

    #[test]
    fn a_card_without_path_can_still_have_places() {
        let c = Card::parse("m", "# m\nid: a\nplace: eval /p/eval\n", 0);
        assert_eq!(c.all_places().len(), 1);
        assert_eq!(c.all_places()[0].label, "eval");
    }
}
