//! Which colour each line of a card is drawn in.
//!
//! Not a markdown parser: a card has five kinds of line and telling them apart
//! is what makes the structure readable at a glance. Pure, so it is tested
//! without a terminal.

use crate::theme::{BOLD, CYAN, DIM, GREEN, GREY, MAGENTA, YELLOW};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The `# name` line at the top.
    Title,
    /// A `## section` heading.
    Section,
    /// A `key: value` line in the header block.
    Field,
    /// Anything inside the README section, which is reference material rather
    /// than something you are writing.
    Readme,
    /// `[ ] something` — still to do.
    Open,
    /// `[x] something` — done, and shown receding.
    Ticked,
    Body,
}

impl Kind {
    pub fn codes(&self) -> &'static [&'static str] {
        match self {
            Kind::Title => &[BOLD, MAGENTA],
            Kind::Section => &[BOLD, CYAN],
            Kind::Field => &[YELLOW],
            Kind::Readme => &[GREY],
            Kind::Open => &[BOLD, GREEN],
            Kind::Ticked => &[DIM],
            Kind::Body => &[],
        }
    }
}

/// Classify every line. Done in one pass over the whole card because a line's
/// kind depends on what came before it: the same text is a field in the header
/// and prose in the body.
pub fn classify(lines: &[String]) -> Vec<Kind> {
    let mut kinds = Vec::with_capacity(lines.len());
    let mut in_header = true;
    let mut in_readme = false;

    for line in lines {
        let trimmed = line.trim_end();
        let kind = if trimmed.starts_with("## ") {
            in_header = false;
            in_readme = trimmed.eq_ignore_ascii_case(crate::card::README_HEADING);
            Kind::Section
        } else if trimmed.starts_with("# ") && in_header {
            Kind::Title
        } else if in_readme {
            Kind::Readme
        } else if in_header && is_field(trimmed) {
            Kind::Field
        } else if let Some(cb) = crate::checkbox::find(line) {
            if cb.done {
                Kind::Ticked
            } else {
                Kind::Open
            }
        } else {
            Kind::Body
        };
        kinds.push(kind);
    }
    kinds
}

/// `key: value`, where the key is a bare word. Prose with a colon in it is not
/// a field, so the key may not contain spaces.
fn is_field(line: &str) -> bool {
    match line.split_once(':') {
        Some((key, _)) => {
            !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(card: &str) -> Vec<Kind> {
        let lines: Vec<String> = card.lines().map(String::from).collect();
        classify(&lines)
    }

    #[test]
    fn a_card_is_title_fields_sections_and_body() {
        let got = kinds("# moxi\nstatus: active\n\n## now\nparser work\n");
        assert_eq!(
            got,
            vec![Kind::Title, Kind::Field, Kind::Body, Kind::Section, Kind::Body]
        );
    }

    #[test]
    fn everything_under_the_readme_heading_is_readme() {
        let got = kinds("# x\n\n## now\na\n\n## readme\n# x\nprose\n");
        assert_eq!(got[2], Kind::Section);
        assert_eq!(got[3], Kind::Body);
        assert_eq!(got[5], Kind::Section);
        assert_eq!(got[6], Kind::Readme, "a heading inside the readme is readme");
        assert_eq!(got[7], Kind::Readme);
    }

    #[test]
    fn a_section_after_the_readme_leaves_readme_colouring() {
        let got = kinds("# x\n\n## readme\nprose\n\n## notes\nmine\n");
        assert_eq!(got[3], Kind::Readme);
        assert_eq!(got[6], Kind::Body);
    }

    #[test]
    fn checkboxes_are_their_own_kinds_outside_the_readme() {
        let got = kinds("# x\n\n## now\n[ ] open\n- [x] done\n\n## readme\n[ ] not ours\n");
        assert_eq!(got[3], Kind::Open);
        assert_eq!(got[4], Kind::Ticked);
        assert_eq!(got[7], Kind::Readme, "boxes in a README are just README");
    }

    #[test]
    fn prose_with_a_colon_is_not_a_field() {
        let got = kinds("# x\nwhat: a thing\nthen this: happened\n");
        assert_eq!(got[1], Kind::Field);
        assert_eq!(got[2], Kind::Body);
    }

    #[test]
    fn a_field_line_below_the_header_is_body() {
        let got = kinds("# x\nstatus: active\n\n## now\nstatus: not a field\n");
        assert_eq!(got[4], Kind::Body);
    }
}
