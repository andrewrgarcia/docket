//! What the picker shows, what is selected, and what folding or ticking does.
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
    /// Index into the cards.
    pub card: usize,
    /// Box-drawing stems including the connector.
    pub prefix: String,
    pub text: String,
    pub expanded: bool,
    pub mark: Mark,
    pub tokens: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Card,
    Section { index: usize },
    Line { section: usize, line: usize },
}

/// How much of a row is going into the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    /// Some sections of this card, not all.
    Partial,
    All,
}

#[derive(Debug, Clone)]
struct Section {
    heading: String,
    /// Body lines as written, blanks included — this is what gets emitted.
    raw: Vec<String>,
    /// Non-blank lines, which is what the tree shows.
    shown: Vec<String>,
    expanded: bool,
    selected: bool,
    tokens: usize,
}

#[derive(Debug, Clone)]
struct Node {
    card: Card,
    sections: Vec<Section>,
    expanded: bool,
    dirty: bool,
}

#[derive(Debug)]
pub struct Picker {
    nodes: Vec<Node>,
    pub cursor: usize,
    pub scroll: usize,
}

/// Four characters per token, the usual English approximation.
fn tokens_of(text: &str) -> usize {
    text.chars().count() / 4
}

impl Picker {
    pub fn new(cards: Vec<Card>) -> Picker {
        let nodes = cards
            .into_iter()
            .map(|card| {
                let sections = card
                    .all_sections()
                    .into_iter()
                    .map(|(heading, raw)| Section {
                        tokens: tokens_of(&heading) + tokens_of(&raw.join("\n")),
                        shown: raw.iter().filter(|l| !l.trim().is_empty()).cloned().collect(),
                        heading,
                        raw,
                        expanded: false,
                        selected: false,
                    })
                    .collect();
                Node { card, sections, expanded: false, dirty: false }
            })
            .collect();
        Picker { nodes, cursor: 0, scroll: 0 }
    }

    pub fn card(&self, index: usize) -> &Card {
        &self.nodes[index].card
    }

    fn mark_of(&self, index: usize) -> Mark {
        let node = &self.nodes[index];
        let picked = node.sections.iter().filter(|s| s.selected).count();
        match picked {
            0 => Mark::None,
            n if n == node.sections.len() => Mark::All,
            _ => Mark::Partial,
        }
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
                mark: self.mark_of(index),
                tokens: node.sections.iter().filter(|s| s.selected).map(|s| s.tokens).sum(),
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
                    mark: if section.selected { Mark::All } else { Mark::None },
                    tokens: section.tokens,
                });
                if !section.expanded {
                    continue;
                }
                let stem = if last { "    " } else { "│   " };
                let last_line = section.shown.len().saturating_sub(1);
                for (l, line) in section.shown.iter().enumerate() {
                    rows.push(Row {
                        kind: Kind::Line { section: s, line: l },
                        card: index,
                        prefix: format!("{stem}{}", if l == last_line { "└── " } else { "├── " }),
                        text: line.clone(),
                        expanded: false,
                        mark: if section.selected { Mark::All } else { Mark::None },
                        tokens: tokens_of(line),
                    });
                }
            }
        }
        rows
    }

    // -- folding -----------------------------------------------------------

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

    pub fn toggle_all_folds(&mut self) {
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

    // -- selecting ---------------------------------------------------------

    /// Space on a card takes or drops the whole card; on a section or one of
    /// its lines, just that section. A partly-picked card fills up rather than
    /// emptying, which is what you mean when you press space on it.
    pub fn toggle_select(&mut self, row: &Row) {
        match row.kind {
            Kind::Card => {
                let want = self.mark_of(row.card) != Mark::All;
                for section in &mut self.nodes[row.card].sections {
                    section.selected = want;
                }
            }
            Kind::Section { index } => {
                let section = &mut self.nodes[row.card].sections[index];
                section.selected = !section.selected;
            }
            Kind::Line { section, .. } => {
                let section = &mut self.nodes[row.card].sections[section];
                section.selected = !section.selected;
            }
        }
    }

    pub fn select_all(&mut self) {
        let everything = self.nodes.iter().all(|n| n.sections.iter().all(|s| s.selected));
        for node in &mut self.nodes {
            for section in &mut node.sections {
                section.selected = !everything;
            }
        }
    }

    pub fn select_none(&mut self) {
        for node in &mut self.nodes {
            for section in &mut node.sections {
                section.selected = false;
            }
        }
    }

    /// Every section whose heading matches, across every card — how you take
    /// "just the `now` of everything" in one key.
    pub fn select_heading(&mut self, heading: &str) {
        let any_off = self
            .nodes
            .iter()
            .flat_map(|n| &n.sections)
            .any(|s| s.heading.eq_ignore_ascii_case(heading) && !s.selected);
        for node in &mut self.nodes {
            for section in &mut node.sections {
                if section.heading.eq_ignore_ascii_case(heading) {
                    section.selected = any_off;
                }
            }
        }
    }

    /// `(card name, chosen section indices)` for every card with a selection.
    pub fn selection(&self) -> Vec<(String, Vec<usize>)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                let chosen: Vec<usize> = node
                    .sections
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.selected)
                    .map(|(i, _)| i)
                    .collect();
                (!chosen.is_empty()).then(|| (node.card.name.clone(), chosen))
            })
            .collect()
    }

    /// `(cards, sections, tokens)` currently chosen.
    pub fn cost(&self) -> (usize, usize, usize) {
        let mut cards = 0;
        let mut sections = 0;
        let mut tokens = 0;
        for node in &self.nodes {
            let picked: Vec<&Section> = node.sections.iter().filter(|s| s.selected).collect();
            if picked.is_empty() {
                continue;
            }
            cards += 1;
            sections += picked.len();
            tokens += tokens_of(&node.card.header()) + picked.iter().map(|s| s.tokens).sum::<usize>();
        }
        (cards, sections, tokens)
    }

    // -- ticking -----------------------------------------------------------

    /// Flip a checkbox, in the view and in the card's text. Returns the card
    /// index when something changed, so the caller can save it.
    pub fn tick(&mut self, row: &Row) -> Option<usize> {
        let Kind::Line { section, line } = row.kind else {
            return None;
        };
        let node = &mut self.nodes[row.card];
        let old = node.sections[section].shown[line].clone();
        let flipped = checkbox::toggle(&old)?;

        // The card's text is the truth; the view is a copy. Rewrite the one
        // matching line, not the section, so nothing else can shift.
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
        node.sections[section].shown[line] = flipped.clone();
        if let Some(slot) = node.sections[section].raw.iter_mut().find(|l| **l == old) {
            *slot = flipped;
        }
        node.dirty = true;
        Some(row.card)
    }

    /// `(name, body)` for every card changed since the last call.
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

    /// `(cards, sections, ticked, boxes)` in the whole store.
    pub fn totals(&self) -> (usize, usize, usize, usize) {
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

    fn picker() -> Picker {
        Picker::new(vec![
            card(
                "moxi",
                "# moxi\nid: aaaa\n\n## now\nparser\n\n## next\n- [ ] spans\n- [x] lexer\n\n## readme\ntheirs\n",
            ),
            card("kol", "# kol\nid: bbbb\n\n## now\nnothing\n"),
        ])
    }

    fn row_named(p: &Picker, text: &str) -> Row {
        p.rows().into_iter().find(|r| r.text == text).expect(text)
    }

    #[test]
    fn collapsed_by_default_one_row_per_card() {
        let rows = picker().rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].mark, Mark::None);
    }

    #[test]
    fn the_readme_is_a_section_you_can_pick() {
        let mut p = picker();
        p.expand_cards();
        let readme = row_named(&p, "readme");
        p.toggle_select(&readme);
        assert_eq!(p.selection(), vec![("moxi".to_string(), vec![2])]);
    }

    #[test]
    fn picking_a_card_picks_every_section_and_unpicking_clears_it() {
        let mut p = picker();
        let card_row = p.rows()[0].clone();
        p.toggle_select(&card_row);
        assert_eq!(p.rows()[0].mark, Mark::All);
        assert_eq!(p.selection(), vec![("moxi".to_string(), vec![0, 1, 2])]);
        p.toggle_select(&card_row);
        assert!(p.selection().is_empty());
    }

    #[test]
    fn a_part_picked_card_shows_partial_and_fills_up_on_space() {
        let mut p = picker();
        p.expand_cards();
        let now = row_named(&p, "now");
        p.toggle_select(&now);
        assert_eq!(p.rows()[0].mark, Mark::Partial);

        let card_row = p.rows()[0].clone();
        p.toggle_select(&card_row);
        assert_eq!(p.rows()[0].mark, Mark::All, "space on a partial card fills it");
    }

    #[test]
    fn a_line_selects_the_section_it_belongs_to() {
        let mut p = picker();
        p.toggle_all_folds();
        let line = row_named(&p, "- [ ] spans");
        p.toggle_select(&line);
        assert_eq!(p.selection(), vec![("moxi".to_string(), vec![1])]);
    }

    #[test]
    fn select_heading_takes_that_section_everywhere() {
        let mut p = picker();
        p.select_heading("## now");
        assert_eq!(
            p.selection(),
            vec![("moxi".to_string(), vec![0]), ("kol".to_string(), vec![0])]
        );
        p.select_heading("## now");
        assert!(p.selection().is_empty(), "pressing it again clears them");
    }

    #[test]
    fn cost_counts_the_header_once_per_card() {
        let mut p = picker();
        assert_eq!(p.cost(), (0, 0, 0));
        let card_row = p.rows()[0].clone();
        p.toggle_select(&card_row);
        let (cards, sections, tokens) = p.cost();
        assert_eq!((cards, sections), (1, 3));
        assert!(tokens > 0);
    }

    #[test]
    fn ticking_rewrites_the_card_body_and_the_view() {
        let mut p = picker();
        p.toggle_all_folds();
        let row = row_named(&p, "- [ ] spans");
        assert_eq!(p.tick(&row), Some(0));

        let body = &p.card(0).body;
        assert!(body.contains("- [x] spans"), "{body}");
        assert!(body.contains("- [x] lexer"), "the other box is untouched");
        assert!(body.contains("## now\nparser"), "the rest survives");
        assert!(p.rows().iter().any(|r| r.text == "- [x] spans"), "view updated");

        let dirty = p.take_dirty();
        assert_eq!(dirty.len(), 1);
        assert!(p.take_dirty().is_empty(), "flags clear once taken");
    }

    #[test]
    fn ticking_something_without_a_box_does_nothing() {
        let mut p = picker();
        p.toggle_all_folds();
        assert_eq!(p.tick(&row_named(&p, "parser")), None);
        assert_eq!(p.tick(&p.rows()[0].clone()), None);
        assert!(p.take_dirty().is_empty());
    }

    #[test]
    fn folding_opens_cards_then_sections_then_lines() {
        let mut p = picker();
        p.expand_cards();
        assert!(p.rows().iter().any(|r| matches!(r.kind, Kind::Section { .. })));
        assert!(!p.rows().iter().any(|r| matches!(r.kind, Kind::Line { .. })));
        p.toggle_all_folds();
        assert!(p.rows().iter().any(|r| matches!(r.kind, Kind::Line { .. })));
        p.collapse_all();
        assert_eq!(p.rows().len(), 2);
    }

    #[test]
    fn blank_lines_do_not_become_rows() {
        let mut p = Picker::new(vec![card("x", "# x\nid: a\n\n## now\n\n\nreal\n\n")]);
        p.toggle_all_folds();
        let lines = p.rows().into_iter().filter(|r| matches!(r.kind, Kind::Line { .. })).count();
        assert_eq!(lines, 1);
    }

    #[test]
    fn select_all_toggles_both_ways() {
        let mut p = picker();
        p.select_all();
        assert_eq!(p.cost().0, 2);
        p.select_all();
        assert_eq!(p.cost().0, 0);
    }
}
