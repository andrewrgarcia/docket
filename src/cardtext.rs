//! Small, exact edits to a card's text, for the commands that write without an
//! editor (`dk set`, `dk todo`, `dk tick`, `dk note`, `dk save`).
//!
//! Every function takes the whole card and returns the whole card. Each one
//! changes the fewest lines it can and leaves every other byte alone: a card is
//! mostly prose a person wrote, and a tool that reflows it has lost their trust.

use crate::card::README_HEADING;
use crate::checkbox;
use crate::error::{Error, Result};

/// Header keys that are not free to set. `id` is fixed for the life of a card.
const FIXED: &[&str] = &["id"];

/// `key: value` in the header. Replaces the first line for that key, or adds
/// one after the last header line. `place` is keyed by its label, so a second
/// place adds a line and a repeated label replaces its own.
pub fn set_field(body: &str, key: &str, value: &str) -> Result<String> {
    let key = key.trim().to_ascii_lowercase();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(Error::usage(format!("`{key}` is not a header key — a key is one word, like `status`")));
    }
    if FIXED.contains(&key.as_str()) {
        return Err(Error::usage(format!("`{key}` is fixed for the life of a card")));
    }
    let value = one_line(value);
    let mut lines = split(body);
    let header_end = lines.iter().position(|l| l.starts_with("## ")).unwrap_or(lines.len());
    let prefix = format!("{key}:");
    let new_line = format!("{key}: {value}");

    let same = |line: &str| -> bool {
        let Some(rest) = line.strip_prefix(&prefix) else { return false };
        if key != "place" {
            return true;
        }
        let label = value.split_whitespace().next().unwrap_or("");
        rest.split_whitespace().next() == Some(label)
    };

    if let Some(at) = lines[..header_end].iter().position(|l| same(l)) {
        lines[at] = new_line;
    } else {
        // After the last non-blank header line, so the blank line that
        // separates the header from `## now` stays where it was.
        let at = lines[..header_end]
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .map_or(0, |i| i + 1);
        lines.insert(at, new_line);
    }
    Ok(join(lines))
}

/// The `state:` line at the top of `## now`: the first line in that section
/// starting with `state:` is replaced; with none, one becomes the section's
/// first line. A card with no `## now` gets one, before every other section.
pub fn set_state(body: &str, state: &str) -> String {
    let line = format!("state: {}", one_line(state));
    let mut lines = split(body);
    match section(&lines, "now") {
        Some((start, end)) => {
            if let Some(at) = (start + 1..end).find(|&i| lines[i].starts_with("state:")) {
                lines[at] = line;
            } else {
                lines.insert(start + 1, line);
            }
        }
        None => {
            let at = lines.iter().position(|l| l.starts_with("## ")).unwrap_or(lines.len());
            let block = vec!["## now".to_string(), line, String::new()];
            if at == lines.len() && lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            for (k, l) in block.into_iter().enumerate() {
                lines.insert(at + k, l);
            }
        }
    }
    join(lines)
}

/// Add `text` to the end of the section called `heading` (without the `## `).
/// A missing section is created just above `## readme`, or at the end. List
/// items join a list without a blank line; anything else is a new paragraph.
pub fn append(body: &str, heading: &str, text: &str) -> String {
    let heading = heading.trim().trim_start_matches('#').trim();
    let text = text.trim_matches('\n');
    let mut lines = split(body);

    let (start, end) = match section(&lines, heading) {
        Some(found) => found,
        None => {
            let at = lines
                .iter()
                .position(|l| l.trim_end().eq_ignore_ascii_case(README_HEADING))
                .unwrap_or(lines.len());
            let mut at = at;
            if at > 0 && !lines[at - 1].trim().is_empty() {
                lines.insert(at, String::new());
                at += 1;
            }
            lines.insert(at, format!("## {heading}"));
            // A new, empty section: the text goes straight under the heading.
            (at, at + 1)
        }
    };

    let last = (start + 1..end).rev().find(|&i| !lines[i].trim().is_empty());
    let new: Vec<String> = text.lines().map(String::from).collect();
    let item = |l: &str| {
        let t = l.trim_start();
        t.starts_with("- ") || t.starts_with("* ") || checkbox::find(l).is_some()
    };
    let at = match last {
        Some(i) => {
            let joins_list = item(&lines[i]) && new.first().is_some_and(|l| item(l));
            if joins_list {
                i + 1
            } else {
                lines.insert(i + 1, String::new());
                i + 2
            }
        }
        None => start + 1,
    };
    let count = new.len();
    for (k, l) in new.into_iter().enumerate() {
        lines.insert(at + k, l);
    }
    // Keep one blank line between this section and the next heading.
    let after = at + count;
    if after < lines.len() && lines[after].starts_with("## ") {
        lines.insert(after, String::new());
    }
    join(lines)
}

/// A new open box at the end of `## next`, written the way the boxes already
/// there are: `[ ] text` if the card's own boxes carry no bullet, `- [ ] text`
/// otherwise (and when there are none yet).
pub fn todo(body: &str, text: &str) -> String {
    let lines = split(body);
    let bare = section(&lines, "next")
        .and_then(|(start, end)| (start + 1..end).rev().find(|&i| checkbox::find(&lines[i]).is_some()))
        .is_some_and(|i| lines[i].trim_start().starts_with('['));
    let bullet = if bare { "" } else { "- " };
    append(body, "next", &format!("{bullet}[ ] {}", one_line(text)))
}

/// Tick the one open box whose text contains `needle` (case-insensitive), in
/// the card's own sections. An exact match wins over partial ones; two or more
/// candidates is an error that lists them, never a guess.
pub fn tick(body: &str, needle: &str) -> Result<String> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Err(Error::usage("tick needs the text of the box"));
    }
    let mut lines = split(body);
    let own_end = lines
        .iter()
        .position(|l| l.trim_end().eq_ignore_ascii_case(README_HEADING))
        .unwrap_or(lines.len());

    let text_of = |line: &str| -> Option<String> {
        let cb = checkbox::find(line)?;
        if cb.done {
            return None;
        }
        Some(line[cb.mark + 2..].trim().to_string())
    };
    let lower = needle.to_lowercase();
    let open: Vec<(usize, String)> = (0..own_end)
        .filter_map(|i| text_of(&lines[i]).map(|t| (i, t)))
        .collect();
    let exact: Vec<&(usize, String)> = open.iter().filter(|(_, t)| t.to_lowercase() == lower).collect();
    let partial: Vec<&(usize, String)> =
        open.iter().filter(|(_, t)| t.to_lowercase().contains(&lower)).collect();
    let hits = if exact.len() == 1 { exact } else { partial };

    match hits.as_slice() {
        [(at, _)] => {
            let at = *at;
            if let Some(flipped) = checkbox::toggle(&lines[at]) {
                lines[at] = flipped;
            }
            Ok(join(lines))
        }
        [] => Err(Error::other(format!("no open box contains `{needle}`"))),
        many => Err(Error::other(format!(
            "{} open boxes contain `{needle}` — use more of the text:\n{}",
            many.len(),
            many.iter().map(|(_, t)| format!("  [ ] {t}")).collect::<Vec<_>>().join("\n")
        ))),
    }
}

/// The body of `## <name>` in a session entry or card, trimmed, as one line.
pub fn section_text(text: &str, name: &str) -> String {
    let lines = split(text);
    match section(&lines, name) {
        Some((start, end)) => one_line(&lines[start + 1..end].join(" ")),
        None => String::new(),
    }
}

/// `(heading line, first line after the section)` for `## <name>`, compared
/// without case. Only level-two headings delimit sections.
fn section(lines: &[String], name: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| {
        l.strip_prefix("## ").is_some_and(|h| h.trim().eq_ignore_ascii_case(name))
    })?;
    let end = (start + 1..lines.len()).find(|&i| lines[i].starts_with("## ")).unwrap_or(lines.len());
    Some((start, end))
}

/// Collapse whitespace runs, newlines included: header fields and the state
/// line are one line each.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split(body: &str) -> Vec<String> {
    body.lines().map(String::from).collect()
}

fn join(lines: Vec<String>) -> String {
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "# moxi\nid: 0003cc59\nstatus: active\nwhat: a language\npath: /x\n\n## now\nstate: old\n\nprose stays\n\n## next\n\n- [ ] write the paper\n- [x] run the eval\n\n## notes\nkeep me\n\n## readme\n[ ] not mine\n";

    #[test]
    fn set_field_replaces_or_adds_and_moves_nothing_else() {
        let out = set_field(CARD, "status", "paused").unwrap();
        assert!(out.contains("status: paused\n"));
        assert_eq!(out.lines().count(), CARD.lines().count());

        let out = set_field(CARD, "white", "/x/WHITE.md").unwrap();
        assert!(out.contains("path: /x\nwhite: /x/WHITE.md\n\n## now"));
    }

    #[test]
    fn places_are_keyed_by_label() {
        let one = set_field(CARD, "place", "eval ~/e").unwrap();
        let two = set_field(&one, "place", "paper ~/p").unwrap();
        let again = set_field(&two, "place", "eval ~/e2").unwrap();
        assert!(again.contains("place: eval ~/e2\nplace: paper ~/p\n"));
        assert!(!again.contains("place: eval ~/e\n"));
    }

    #[test]
    fn the_id_cannot_be_set() {
        assert!(set_field(CARD, "id", "deadbeef").is_err());
        assert!(set_field(CARD, "two words", "x").is_err());
    }

    #[test]
    fn state_replaces_the_line_in_now_only() {
        let out = set_state(CARD, "new\nstate");
        assert!(out.contains("## now\nstate: new state\n\nprose stays"));
        let fresh = set_state("# a\nid: 1\n\n## next\n- [ ] x\n", "s");
        assert!(fresh.contains("id: 1\n\n## now\nstate: s\n\n## next"));
        let no_line = set_state("# a\n\n## now\nprose\n", "s");
        assert!(no_line.contains("## now\nstate: s\nprose"));
    }

    #[test]
    fn todo_joins_the_list_at_the_end_of_next() {
        let out = todo(CARD, "draft the intro");
        assert!(out.contains("- [x] run the eval\n- [ ] draft the intro\n\n## notes"));
    }

    #[test]
    fn todo_copies_the_cards_own_box_style() {
        let bare = "# a\n\n## next\nprose\n\n[ ] one\n[x] two\n";
        assert!(todo(bare, "three").ends_with("[x] two\n[ ] three\n"));
        let none = "# a\n\n## next\n";
        assert!(todo(none, "first").ends_with("## next\n- [ ] first\n"));
    }

    #[test]
    fn note_appends_a_paragraph_or_makes_the_section() {
        let out = append(CARD, "notes", "a new fact");
        assert!(out.contains("## notes\nkeep me\n\na new fact\n\n## readme"));
        let made = append(CARD, "open questions", "which model?");
        assert!(made.contains("keep me\n\n## open questions\nwhich model?\n\n## readme"));
        let at_end = append("# a\n\n## now\nx\n", "notes", "y");
        assert!(at_end.ends_with("x\n\n## notes\ny\n"));
    }

    #[test]
    fn tick_needs_one_open_box_and_never_touches_the_readme() {
        let out = tick(CARD, "paper").unwrap();
        assert!(out.contains("- [x] write the paper"));
        assert!(tick(CARD, "not mine").is_err());
        assert!(tick(CARD, "eval").is_err(), "a ticked box is not open");
        let two = todo(CARD, "write the paper intro");
        assert!(tick(&two, "write").is_err(), "two candidates");
        let exact = tick(&two, "write the paper").unwrap();
        assert!(exact.contains("- [x] write the paper\n"));
        assert!(exact.contains("- [ ] write the paper intro"));
    }

    #[test]
    fn section_text_reads_one_section_as_one_line() {
        let entry = "# t\n\n## state\nP0 done ·\ncriteria met\n\n## blockers\nnone\n";
        assert_eq!(section_text(entry, "state"), "P0 done · criteria met");
        assert_eq!(section_text(entry, "missing"), "");
    }
}
