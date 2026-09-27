//! The file that goes to the model.
//!
//! One markdown document: an index of every card, then the full text of the
//! ones chosen. The index is always complete, even when only two cards are
//! written out, so the model knows the shape of the whole workload.

use crate::card::Card;

pub const DEFAULT_FILE: &str = "DOCKET.md";
pub const DEFAULT_ZIP: &str = "DOCKET.zip";

/// Outline-level selection: `(card name, chosen node indices)`, indices into
/// `Card::outline().nodes`. Each chosen card is written header-first, because
/// a `## now` with no card around it is unreadable, and chosen nodes are
/// emitted in document order so a README's structure survives the trip.
pub fn build_selection(cards: &[Card], selection: &[(String, Vec<usize>)]) -> String {
    let mut out = index_only(cards, selection);
    out.push_str("\n## CARDS\n\n");
    for card in cards {
        let Some((_, chosen)) = selection.iter().find(|(name, _)| name == &card.name) else {
            continue;
        };
        out.push_str(card.header().trim_end());
        out.push_str("\n\n");

        let outline = card.full_outline();
        for index in 0..outline.nodes.len() {
            if chosen.contains(&index) {
                out.push_str(&outline.text_of(index));
                out.push('\n');
            }
        }
        out.push_str("---\n\n");
    }
    out
}

/// Just the index: every card, with the chosen ones starred. This is the
/// whole of a zip's `INDEX.md`, and the top of a printed brief.
pub fn index_only(cards: &[Card], selection: &[(String, Vec<usize>)]) -> String {
    let mut out = String::from("# DOCKET\n\n## INDEX\n\n");
    for card in cards {
        let picked = selection.iter().find(|(name, _)| name == &card.name);
        let mark = if picked.is_some() { '*' } else { '-' };
        let what = if card.what.is_empty() { String::new() } else { format!(" {}", card.what) };
        let partial = match picked {
            Some((_, chosen)) => {
                let total = card.full_outline().nodes.len();
                if chosen.len() < total {
                    format!(" ({} of {total} sections)", chosen.len())
                } else {
                    String::new()
                }
            }
            None => String::new(),
        };
        out.push_str(&format!(
            "{mark} {} [{}, {}d]{what}{partial}\n",
            card.name, card.status, card.age_days
        ));
    }
    out
}

/// Every card, whole — `dk out`.
///
/// Expressed as a selection of everything so there is exactly one code path
/// that turns cards into a brief. Writing `card.body` directly here is what
/// made `out` the one command that missed a linked README.
pub fn build(cards: &[Card], chosen: &[String]) -> String {
    let selection: Vec<(String, Vec<usize>)> = cards
        .iter()
        .filter(|card| chosen.is_empty() || chosen.contains(&card.name))
        .map(|card| (card.name.clone(), (0..card.full_outline().nodes.len()).collect()))
        .collect();
    build_selection(cards, &selection)
}

/// A rough token count for what was written.
pub fn tokens(text: &str) -> usize {
    text.chars().count() / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards() -> Vec<Card> {
        vec![
            Card::parse("moxi", "# moxi\nstatus: active\nwhat: a language\n\n## now\nparser\n", 3),
            Card::parse("kol", "# kol\nstatus: idea\nwhat: a game\n\n## now\nnothing\n", 142),
        ]
    }

    #[test]
    fn no_selection_writes_everything() {
        let text = build(&cards(), &[]);
        assert!(text.contains("* moxi"));
        assert!(text.contains("* kol"));
        assert!(text.contains("parser"));
        assert!(text.contains("nothing"));
        assert!(!text.contains("of 1 sections"), "whole cards are not partial");
    }

    #[test]
    fn out_carries_a_linked_readme_like_pick_does() {
        let dir = std::env::temp_dir().join(format!("dk-brief-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let readme = dir.join("README.md");
        std::fs::write(&readme, "# moxi\n\nThe linked text.\n").unwrap();

        let card = Card::parse(
            "moxi",
            &format!("# moxi\nid: a\nstatus: active\nreadme: {}\n\n## now\nparser\n", readme.display()),
            0,
        );
        let text = build(&[card], &[]);
        assert!(text.contains("The linked text."), "{text}");
        let _ = std::fs::remove_file(&readme);
    }

    #[test]
    fn a_selection_still_indexes_every_card() {
        let text = build(&cards(), &["moxi".to_string()]);
        assert!(text.contains("* moxi [active, 3d] a language"));
        assert!(text.contains("- kol [idea, 142d] a game"));
        let bodies = text.split("## CARDS").nth(1).unwrap();
        assert!(bodies.contains("parser"));
        assert!(!bodies.contains("nothing"));
    }

    #[test]
    fn cards_are_separated_by_a_rule() {
        assert_eq!(build(&cards(), &[]).matches("\n---\n").count(), 2);
    }

    #[test]
    fn a_section_selection_writes_the_header_and_those_sections_only() {
        let cards = vec![Card::parse(
            "moxi",
            "# moxi\nid: a43b\nstatus: active\nwhat: a language\n\n## now\nparser\n\n## next\nspans\n\n## readme\ntheirs\n",
            3,
        )];
        let text = build_selection(&cards, &[("moxi".to_string(), vec![0, 2])]);

        assert!(text.contains("# moxi\nid: a43b"), "header travels with it");
        assert!(text.contains("## now\nparser"));
        assert!(text.contains("## readme\ntheirs"));
        assert!(!text.contains("spans"), "unchosen sections stay out");
        assert!(text.contains("(2 of 3 sections)"), "the index says it is partial");
    }

    #[test]
    fn a_readme_subsection_can_be_sent_without_the_whole_readme() {
        let card = Card::parse(
            "docket",
            "# docket\nid: a\n\n## now\nx\n\n## readme\n\n# docket\n\n## Install\ncargo install\n\n## Commands\na table\n",
            0,
        );
        let outline = card.outline();
        let install = outline.nodes.iter().position(|n| n.title() == "Install").unwrap();
        let text = build_selection(&[card], &[("docket".to_string(), vec![install])]);

        assert!(text.contains("## Install\ncargo install"));
        assert!(!text.contains("a table"), "its sibling stays behind");
        assert!(!text.contains("## now"), "and so does the rest of the card");
    }

    #[test]
    fn a_whole_card_selection_is_not_marked_partial() {
        let cards = vec![Card::parse("x", "# x\nid: a\n\n## now\nq\n", 0)];
        let text = build_selection(&cards, &[("x".to_string(), vec![0])]);
        assert!(!text.contains("of 1 sections"));
        assert!(text.contains("* x ["));
    }

    #[test]
    fn unpicked_cards_still_appear_in_the_index() {
        let cards = vec![
            Card::parse("a", "# a\nid: a\nwhat: one\n\n## now\nx\n", 0),
            Card::parse("b", "# b\nid: b\nwhat: two\n\n## now\ny\n", 0),
        ];
        let text = build_selection(&cards, &[("a".to_string(), vec![0])]);
        assert!(text.contains("* a ["));
        assert!(text.contains("- b ["));
        assert!(!text.split("## CARDS").nth(1).unwrap().contains("# b"));
    }

    #[test]
    fn tokens_are_estimated_from_length() {
        assert_eq!(tokens(&"a".repeat(400)), 100);
    }
}
