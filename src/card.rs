use std::fmt;
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
    pub agents: String,
    /// The file as written, including the header.
    pub body: String,
    /// Days since the file was last modified.
    pub age_days: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Active,
    Idea,
    Paused,
    Done,
    Dead,
    /// Anything else the user typed; kept verbatim by the store.
    Other,
}

impl Status {
    pub fn parse(s: &str) -> Status {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Status::Active,
            "idea" => Status::Idea,
            "paused" => Status::Paused,
            "done" => Status::Done,
            "dead" => Status::Dead,
            _ => Status::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Idea => "idea",
            Status::Paused => "paused",
            Status::Done => "done",
            Status::Dead => "dead",
            Status::Other => "-",
        }
    }

    /// Done and dead cards sort last and print dim.
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
            agents: field("agents:"),
            body: body.to_string(),
            age_days,
        }
    }

    /// Replace the card's README section, or add one when it has none. The
    /// hand-written sections above it are untouched — the README is the only
    /// part of a card docket ever rewrites, because it is the only part
    /// docket wrote in the first place.
    pub fn with_readme(&self, readme: &str) -> String {
        let kept: String = self
            .body
            .lines()
            .take_while(|l| !l.trim_end().eq_ignore_ascii_case(README_HEADING))
            .collect::<Vec<_>>()
            .join("\n");
        let kept = kept.trim_end();

        if readme.trim().is_empty() {
            return format!("{kept}\n");
        }
        format!("{kept}\n\n{README_HEADING}\n\n{}\n", readme.trim_end())
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

    /// A rough token count for the picker's budget. Four characters per token
    /// is the usual English approximation and is close enough to choose with.
    pub fn tokens(&self) -> usize {
        self.body.chars().count() / 4
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

    /// True when the card carries a README.
    pub fn has_readme(&self) -> bool {
        self.body
            .lines()
            .any(|l| l.trim_end().eq_ignore_ascii_case(README_HEADING))
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
        if !agents.is_empty() {
            s.push_str(&format!("agents: {agents}\n"));
        }
        s.push_str("\n## now\n\n## next\n\n## open questions\n\n## notes\n");
        if !readme.is_empty() {
            s.push_str(&format!("\n{README_HEADING}\n\n{}\n", readme.trim_end()));
        }
        s
    }
}

/// Days since the project was last touched.
///
/// For a project with a git repository, that is the time of the last commit
/// or staged change — `.git/index` is rewritten by `add`, `commit` and
/// `checkout`, which is exactly "when did I last work on this" without
/// spawning git. For anything else it is the card file's own mtime. Docket's
/// own rewrites (`sync`, id backfill) preserve the card's mtime so they never
/// make a forgotten project look fresh.
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
        assert_eq!(c.age_days, 3);
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
    fn the_readme_is_the_last_section() {
        let card = Card::template("a43b21c0", "moxi", Status::Active, "a language", "/p", "", "# moxi\n\nA tool.");
        let sections: Vec<&str> = card.lines().filter(|l| l.starts_with("## ")).collect();
        assert_eq!(sections.last(), Some(&README_HEADING));
        assert!(card.contains("A tool."));
        assert!(card.find("## now").unwrap() < card.find(README_HEADING).unwrap());
    }

    #[test]
    fn a_project_without_a_readme_gets_no_readme_section() {
        let card = Card::template("a43b21c0", "moxi", Status::Idea, "x", "", "", "");
        assert!(!card.contains(README_HEADING));
        assert!(!Card::parse("moxi", &card, 0).has_readme());
    }

    #[test]
    fn has_readme_sees_the_section() {
        let card = Card::template("a43b21c0", "moxi", Status::Active, "x", "/p", "", "hello");
        assert!(Card::parse("moxi", &card, 0).has_readme());
    }

    #[test]
    fn with_readme_replaces_only_the_readme_section() {
        let card = Card::parse(
            "moxi",
            "# moxi\nid: a43b21c0\nstatus: active\n\n## now\nparser\n\n## readme\nold text\n",
            0,
        );
        let updated = card.with_readme("new text");
        assert!(updated.contains("## now\nparser"));
        assert!(updated.contains("new text"));
        assert!(!updated.contains("old text"));
        assert_eq!(updated.matches(README_HEADING).count(), 1);
    }

    #[test]
    fn with_readme_appends_when_there_was_none() {
        let card = Card::parse("moxi", "# moxi\nid: a43b21c0\nstatus: active\n\n## now\n", 0);
        let updated = card.with_readme("fresh");
        assert!(updated.contains(README_HEADING));
        assert!(updated.trim_end().ends_with("fresh"));
    }

    #[test]
    fn with_readme_of_nothing_removes_the_section() {
        let card = Card::parse("moxi", "# moxi\nid: a\n\n## now\nx\n\n## readme\nold\n", 0);
        let updated = card.with_readme("");
        assert!(!updated.contains(README_HEADING));
        assert!(updated.contains("## now\nx"));
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
    fn tokens_are_estimated_from_length() {
        let c = Card::parse("x", &"a".repeat(400), 0);
        assert_eq!(c.tokens(), 100);
    }
}
