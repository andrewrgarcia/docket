//! The file that goes to the model.
//!
//! One markdown document: an index of every card, then the full text of the
//! ones chosen. The index is always complete, even when only two cards are
//! written out, so the model knows the shape of the whole workload.

use crate::card::Card;

pub const DEFAULT_FILE: &str = "DOCKET.md";

/// Section-level selection: `(card name, chosen section indices)`, indices
/// into `Card::all_sections`. Each chosen card is written out header-first,
/// because a `## now` with no card around it is unreadable.
pub fn build_selection(cards: &[Card], selection: &[(String, Vec<usize>)]) -> String {
    let mut out = String::from("# DOCKET\n\n## INDEX\n\n");
    for card in cards {
        let picked = selection.iter().find(|(name, _)| name == &card.name);
        let mark = if picked.is_some() { '*' } else { '-' };
        let what = if card.what.is_empty() { String::new() } else { format!(" {}", card.what) };
        let partial = match picked {
            Some((_, chosen)) if chosen.len() < card.all_sections().len() => {
                format!(" ({} of {} sections)", chosen.len(), card.all_sections().len())
            }
            _ => String::new(),
        };
        out.push_str(&format!(
            "{mark} {} [{}, {}d]{what}{partial}\n",
            card.name, card.status, card.age_days
        ));
    }

    out.push_str("\n## CARDS\n\n");
    for card in cards {
        let Some((_, chosen)) = selection.iter().find(|(name, _)| name == &card.name) else {
            continue;
        };
        out.push_str(card.header().trim_end());
        out.push_str("\n\n");
        for (index, (heading, lines)) in card.all_sections().into_iter().enumerate() {
            if !chosen.contains(&index) {
                continue;
            }
            out.push_str(&heading);
            out.push('\n');
            let body = lines.join("\n");
            let body = body.trim_end();
            if !body.is_empty() {
                out.push_str(body);
                out.push('\n');
            }
            out.push('\n');
        }
        out.push_str("---\n\n");
    }
    out
}

/// `chosen` empty means every card, whole.
pub fn build(cards: &[Card], chosen: &[String]) -> String {
    let picked: Vec<&Card> = if chosen.is_empty() {
        cards.iter().collect()
    } else {
        cards.iter().filter(|c| chosen.contains(&c.name)).collect()
    };

    let mut out = String::from("# DOCKET\n\n## INDEX\n\n");
    for card in cards {
        let mark = if picked.iter().any(|p| p.name == card.name) { '*' } else { '-' };
        let what = if card.what.is_empty() {
            String::new()
        } else {
            format!(" {}", card.what)
        };
        out.push_str(&format!(
            "{mark} {} [{}, {}d]{}\n",
            card.name, card.status, card.age_days, what
        ));
    }

    out.push_str("\n## CARDS\n\n");
    for card in &picked {
        out.push_str(card.body.trim_end());
        out.push_str("\n\n---\n\n");
    }
    out
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
