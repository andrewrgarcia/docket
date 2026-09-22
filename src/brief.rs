//! The file that goes to the model.
//!
//! One markdown document: an index of every card, then the full text of the
//! ones chosen. The index is always complete, even when only two cards are
//! written out, so the model knows the shape of the whole workload.

use crate::card::Card;

pub const DEFAULT_FILE: &str = "DOCKET.md";

/// `chosen` empty means every card.
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
    fn tokens_are_estimated_from_length() {
        assert_eq!(tokens(&"a".repeat(400)), 100);
    }
}
