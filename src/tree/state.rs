//! What the tree shows, and what folding and ticking do to it.
//!
//! No terminal here. `ui.rs` turns these rows into pixels and key presses into
//! calls on this type; keeping that seam clean is what lets the behaviour be
//! tested without a tty.

use crate::card::Card;
use crate::checkbox;

/// One line of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    /// Index into `cards`.
    pub card: usize,
    /// Box-drawing stems including the connector, as in `ygg`.
    pub prefix: String,
    pub text: String,
    /// Folds only: whether this row is open.
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A card. Folds to show its sections.
    Card,
    /// A `## heading`. Folds to show its lines.
    Section { index: usize },
    /// A line of a section's body.
    Line { section: usize, line: usize },
}

/// A card as the tree holds it: headings, their lines, and what is open.
#[derive(Debug, Clone)]
struct Node {
    card: Card,
    sections: Vec<Section>,
    expanded: bool,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct Section {
    heading: String,
    lines: Vec<String>,
    expanded: bool,
}

#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Node>,
    pub cursor: usize,
    pub scroll: usize,
}

impl Tree {
    pub fn new(cards: Vec<Card>) -> Tree {
        let nodes = cards
            .into_iter()
            .map(|card| {
                let sections = card
                    .sections()
                    .into_iter()
                    .map(|(heading, lines)| Section {
                        heading,
                        // Blank lines are structure in the file and noise in a
                        // tree; the card keeps them, the view drops them.
                        lines: lines.into_iter().filter(|l| !l.trim().is_empty()).collect(),
                        expanded: false,
                    })
                    .collect();
                Node { card, sections, expanded: false, dirty: false }
            })
            .collect();
        Tree { nodes, cursor: 0, scroll: 0 }
    }


    pub fn card(&self, index: usize) -> &Card {
        &self.nodes[index].card
    }

    /// Every visible row, top to bottom.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            rows.push(Row {
                kind: Kind::Card,
                card: index,
                prefix: String::new(),
                text: node.card.name.clone(),
                expanded: node.expanded,
            });
            if !node.expanded {
                continue;
            }
            let last_section = node.sections.len().saturating_sub(1);
            for (s, section) in node.sections.iter().enumerate() {
                let last = s == last_section;
                rows.push(Row {
                    kind: Kind::Section { index: s },
                    card: index,
                    prefix: if last { "└── ".into() } else { "├── ".into() },
                    text: section.heading.trim_start_matches("## ").to_string(),
                    expanded: section.expanded,
                });
                if !section.expanded {
                    continue;
                }
                let stem = if last { "    " } else { "│   " };
                let last_line = section.lines.len().saturating_sub(1);
                for (l, line) in section.lines.iter().enumerate() {
                    rows.push(Row {
                        kind: Kind::Line { section: s, line: l },
                        card: index,
                        prefix: format!("{stem}{}", if l == last_line { "└── " } else { "├── " }),
                        text: line.clone(),
                        expanded: false,
                    });
                }
            }
        }
        rows
    }

    /// Open or close the row, or its parent when the row is a leaf.
    pub fn fold(&mut self, row: &Row, open: bool) {
        match row.kind {
            Kind::Card => self.nodes[row.card].expanded = open,
            Kind::Section { index } => self.nodes[row.card].sections[index].expanded = open,
            Kind::Line { section, .. } => {
                if !open {
                    self.nodes[row.card].sections[section].expanded = false;
                }
            }
        }
    }

    pub fn toggle_fold(&mut self, row: &Row) {
        let open = match row.kind {
            Kind::Card => !self.nodes[row.card].expanded,
            Kind::Section { index } => !self.nodes[row.card].sections[index].expanded,
            Kind::Line { .. } => false,
        };
        self.fold(row, open);
    }

    /// Everything open, or — when everything already is — everything shut.
    pub fn toggle_all(&mut self) {
        let all_open = self
            .nodes
            .iter()
            .all(|n| n.expanded && n.sections.iter().all(|s| s.expanded));
        for node in &mut self.nodes {
            node.expanded = !all_open;
            for section in &mut node.sections {
                section.expanded = !all_open;
            }
        }
    }

    /// Open every card but leave the sections shut: the overview.
    pub fn expand_cards(&mut self) {
        for node in &mut self.nodes {
            node.expanded = true;
        }
    }

    pub fn collapse_all(&mut self) {
        for node in &mut self.nodes {
            node.expanded = false;
            for section in &mut node.sections {
                section.expanded = false;
            }
        }
    }

    /// Tick or untick a checkbox line, in the view and in the card's text.
    /// Returns the card index when something changed, so the caller can save.
    pub fn tick(&mut self, row: &Row) -> Option<usize> {
        let Kind::Line { section, line } = row.kind else {
            return None;
        };
        let node = &mut self.nodes[row.card];
        let old = node.sections[section].lines[line].clone();
        let flipped = checkbox::toggle(&old)?;

        // The card's text is the truth; the view is a copy. Rewrite the one
        // matching line in the body, not the whole section, so nothing else
        // in the file can shift.
        let mut replaced = false;
        let body: Vec<String> = node
            .card
            .body
            .lines()
            .map(|l| {
                if !replaced && l == old {
                    replaced = true;
                    flipped.clone()
                } else {
                    l.to_string()
                }
            })
            .collect();
        if !replaced {
            return None;
        }
        node.card.body = format!("{}\n", body.join("\n"));
        node.sections[section].lines[line] = flipped;
        node.dirty = true;
        Some(row.card)
    }

    /// `(name, body)` for every card changed since the last call, and clears
    /// the flags.
    pub fn take_dirty(&mut self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for node in &mut self.nodes {
            if node.dirty {
                node.dirty = false;
                out.push((node.card.name.clone(), node.card.body.clone()));
            }
        }
        out
    }

    /// `(cards, sections shown, boxes ticked, boxes total)`.
    pub fn summary(&self) -> (usize, usize, usize, usize) {
        let sections = self.nodes.iter().map(|n| n.sections.len()).sum();
        let (done, total) = self
            .nodes
            .iter()
            .map(|n| n.card.progress())
            .fold((0, 0), |(d, t), (nd, nt)| (d + nd, t + nt));
        (self.nodes.len(), sections, done, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, body: &str) -> Card {
        Card::parse(name, body, 0)
    }

    fn tree() -> Tree {
        Tree::new(vec![
            card(
                "moxi",
                "# moxi\nid: aaaa\n\n## now\nparser\n\n## next\n- [ ] spans\n- [x] lexer\n",
            ),
            card("kol", "# kol\nid: bbbb\n\n## now\n"),
        ])
    }

    #[test]
    fn collapsed_by_default_one_row_per_card() {
        let rows = tree().rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "moxi");
        assert!(matches!(rows[0].kind, Kind::Card));
    }

    #[test]
    fn expanding_a_card_shows_its_sections_only() {
        let mut t = tree();
        let row = t.rows()[0].clone();
        t.toggle_fold(&row);
        let rows = t.rows();
        assert_eq!(rows.len(), 4, "card + two sections + the other card");
        assert_eq!(rows[1].text, "now");
        assert_eq!(rows[2].text, "next");
        assert_eq!(rows[2].prefix, "└── ");
    }

    #[test]
    fn expanding_a_section_shows_its_lines() {
        let mut t = tree();
        let card_row = t.rows()[0].clone();
        t.toggle_fold(&card_row);
        let section = t.rows()[2].clone();
        t.toggle_fold(&section);
        let texts: Vec<String> = t.rows().iter().map(|r| r.text.clone()).collect();
        assert!(texts.contains(&"- [ ] spans".to_string()));
        assert!(texts.contains(&"- [x] lexer".to_string()));
    }

    #[test]
    fn blank_lines_do_not_become_rows() {
        let mut t = Tree::new(vec![card("x", "# x\nid: a\n\n## now\n\n\nreal\n\n")]);
        t.toggle_all();
        let lines = t
            .rows()
            .into_iter()
            .filter(|r| matches!(r.kind, Kind::Line { .. }))
            .count();
        assert_eq!(lines, 1);
    }

    #[test]
    fn toggle_all_opens_then_shuts_everything() {
        let mut t = tree();
        t.toggle_all();
        assert!(t.rows().iter().any(|r| matches!(r.kind, Kind::Line { .. })));
        t.toggle_all();
        assert_eq!(t.rows().len(), 2);
    }

    #[test]
    fn expand_cards_stops_at_the_headings() {
        let mut t = tree();
        t.expand_cards();
        assert!(t.rows().iter().any(|r| matches!(r.kind, Kind::Section { .. })));
        assert!(!t.rows().iter().any(|r| matches!(r.kind, Kind::Line { .. })));
    }

    #[test]
    fn ticking_rewrites_the_card_body() {
        let mut t = tree();
        t.toggle_all();
        let row = t
            .rows()
            .into_iter()
            .find(|r| r.text.contains("[ ] spans"))
            .expect("the open box");
        assert_eq!(t.tick(&row), Some(0));

        let body = &t.card(0).body;
        assert!(body.contains("- [x] spans"), "{body}");
        assert!(body.contains("- [x] lexer"), "the other box is untouched");
        assert!(body.contains("## now\nparser"), "the rest of the card survives");

        let dirty = t.take_dirty();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0].0, "moxi");
        assert!(t.take_dirty().is_empty(), "flags clear once taken");
    }

    #[test]
    fn ticking_a_line_without_a_box_does_nothing() {
        let mut t = tree();
        t.toggle_all();
        let prose = t
            .rows()
            .into_iter()
            .find(|r| r.text == "parser")
            .expect("prose line");
        assert_eq!(t.tick(&prose), None);
        assert!(t.take_dirty().is_empty());
    }

    #[test]
    fn a_card_row_cannot_be_ticked() {
        let mut t = tree();
        let row = t.rows()[0].clone();
        assert_eq!(t.tick(&row), None);
    }

    #[test]
    fn summary_counts_cards_sections_and_boxes() {
        assert_eq!(tree().summary(), (2, 3, 1, 2));
    }
}
