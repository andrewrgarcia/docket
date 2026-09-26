//! What the picker shows, what is selected, and what folding or ticking does.
//!
//! No terminal here. `ui.rs` turns these rows into pixels and key presses into
//! calls on this type; keeping that seam clean is what lets the behaviour be
//! tested without a tty.
//!
//! The tree is a card's `Outline`, so a README's own headings hang under
//! `## readme` rather than posing as sections of the card.

use std::collections::BTreeSet;

use crate::card::Card;
use crate::outline::Outline;

/// One line of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    pub card: usize,
    /// Box-drawing stems including the connector.
    pub prefix: String,
    pub text: String,
    pub expanded: bool,
    /// Whether the row can be folded at all.
    pub foldable: bool,
    /// 0 for a card's own section; deeper for anything a README brought.
    /// Carried on the row so drawing never reparses the card.
    pub depth: usize,
    pub mark: Mark,
    pub tokens: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Card,
    /// A heading — the card's own, or one nested inside it.
    Node { index: usize },
    /// A body line of a heading.
    Line { node: usize, line: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Partial,
    All,
}

#[derive(Debug, Clone)]
struct Node {
    card: Card,
    outline: Outline,
    /// Non-blank body lines per outline node, which is what the tree shows.
    shown: Vec<Vec<String>>,
    /// Tokens for a node's own heading and lines.
    tokens: Vec<usize>,
    selected: BTreeSet<usize>,
    open: BTreeSet<usize>,
    expanded: bool,
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
                let outline = card.outline();
                let shown = outline
                    .nodes
                    .iter()
                    .map(|n| n.lines.iter().filter(|l| !l.trim().is_empty()).cloned().collect())
                    .collect();
                let tokens = (0..outline.nodes.len())
                    .map(|i| tokens_of(&outline.text_of(i)))
                    .collect();
                Node {
                    card,
                    outline,
                    shown,
                    tokens,
                    selected: BTreeSet::new(),
                    open: BTreeSet::new(),
                    expanded: false,
                }
            })
            .collect();
        Picker { nodes, cursor: 0, scroll: 0 }
    }

    pub fn card(&self, index: usize) -> &Card {
        &self.nodes[index].card
    }

    /// Tokens for a node and everything under it.
    fn subtree_tokens(&self, card: usize, index: usize) -> usize {
        let node = &self.nodes[card];
        node.outline.subtree(index).iter().map(|i| node.tokens[*i]).sum()
    }

    fn mark_of(&self, card: usize, index: usize) -> Mark {
        let node = &self.nodes[card];
        let subtree = node.outline.subtree(index);
        let picked = subtree.iter().filter(|i| node.selected.contains(i)).count();
        match picked {
            0 => Mark::None,
            n if n == subtree.len() => Mark::All,
            _ => Mark::Partial,
        }
    }

    fn card_mark(&self, card: usize) -> Mark {
        let node = &self.nodes[card];
        match node.selected.len() {
            0 => Mark::None,
            n if n == node.outline.nodes.len() => Mark::All,
            _ => Mark::Partial,
        }
    }

    /// Every visible row, top to bottom.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (card, node) in self.nodes.iter().enumerate() {
            rows.push(Row {
                kind: Kind::Card,
                card,
                prefix: String::new(),
                text: node.card.name.clone(),
                expanded: node.expanded,
                foldable: !node.outline.nodes.is_empty(),
                depth: 0,
                mark: self.card_mark(card),
                tokens: node.selected.iter().map(|i| node.tokens[*i]).sum(),
            });
            if node.expanded {
                let roots = node.outline.roots.clone();
                self.walk(card, &roots, "", &mut rows);
            }
        }
        rows
    }

    /// Depth-first, drawing the stems as it goes.
    fn walk(&self, card: usize, indices: &[usize], stem: &str, rows: &mut Vec<Row>) {
        let node = &self.nodes[card];
        let last = indices.len().saturating_sub(1);
        for (position, index) in indices.iter().enumerate() {
            let is_last = position == last;
            let connector = if is_last { "└── " } else { "├── " };
            let children = &node.outline.nodes[*index].children;
            let lines = &node.shown[*index];

            rows.push(Row {
                kind: Kind::Node { index: *index },
                card,
                prefix: format!("{stem}{connector}"),
                text: node.outline.nodes[*index].title().to_string(),
                expanded: node.open.contains(index),
                foldable: !children.is_empty() || !lines.is_empty(),
                depth: node.outline.nodes[*index].depth,
                mark: self.mark_of(card, *index),
                tokens: self.subtree_tokens(card, *index),
            });

            if !node.open.contains(index) {
                continue;
            }
            let deeper = format!("{stem}{}", if is_last { "    " } else { "│   " });
            let last_line = lines.len().saturating_sub(1);
            for (l, line) in lines.iter().enumerate() {
                let tail = l == last_line && children.is_empty();
                rows.push(Row {
                    kind: Kind::Line { node: *index, line: l },
                    card,
                    prefix: format!("{deeper}{}", if tail { "└── " } else { "├── " }),
                    text: line.clone(),
                    expanded: false,
                    foldable: false,
                    depth: node.outline.nodes[*index].depth + 1,
                    mark: if node.selected.contains(index) { Mark::All } else { Mark::None },
                    tokens: tokens_of(line),
                });
            }
            self.walk(card, children, &deeper, rows);
        }
    }

    // -- folding -----------------------------------------------------------

    pub fn fold(&mut self, row: &Row, open: bool) {
        match row.kind {
            Kind::Card => self.nodes[row.card].expanded = open,
            Kind::Node { index } => {
                if open {
                    self.nodes[row.card].open.insert(index);
                } else {
                    self.nodes[row.card].open.remove(&index);
                }
            }
            Kind::Line { node, .. } => {
                if !open {
                    self.nodes[row.card].open.remove(&node);
                }
            }
        }
    }

    pub fn toggle_fold(&mut self, row: &Row) {
        let open = match row.kind {
            Kind::Card => !self.nodes[row.card].expanded,
            Kind::Node { index } => !self.nodes[row.card].open.contains(&index),
            Kind::Line { .. } => false,
        };
        self.fold(row, open);
    }




    // -- selecting ---------------------------------------------------------

    /// Space takes a whole subtree: a heading brings everything under it,
    /// because sending `## Install` without its `### From source` would be a
    /// quietly truncated document.
    pub fn toggle_select(&mut self, row: &Row) {
        match row.kind {
            Kind::Card => {
                let want = self.card_mark(row.card) != Mark::All;
                let node = &mut self.nodes[row.card];
                node.selected = if want { (0..node.outline.nodes.len()).collect() } else { BTreeSet::new() };
            }
            Kind::Node { index } => self.toggle_subtree(row.card, index),
            Kind::Line { node, .. } => self.toggle_subtree(row.card, node),
        }
    }

    fn toggle_subtree(&mut self, card: usize, index: usize) {
        let want = self.mark_of(card, index) != Mark::All;
        let subtree = self.nodes[card].outline.subtree(index);
        let node = &mut self.nodes[card];
        for at in subtree {
            if want {
                node.selected.insert(at);
            } else {
                node.selected.remove(&at);
            }
        }
    }

    pub fn select_all(&mut self) {
        let everything = self
            .nodes
            .iter()
            .all(|n| n.selected.len() == n.outline.nodes.len());
        for node in &mut self.nodes {
            node.selected = if everything {
                BTreeSet::new()
            } else {
                (0..node.outline.nodes.len()).collect()
            };
        }
    }


    /// `(card name, chosen node indices)` for every card with a selection.
    pub fn selection(&self) -> Vec<(String, Vec<usize>)> {
        self.nodes
            .iter()
            .filter(|node| !node.selected.is_empty())
            .map(|node| (node.card.name.clone(), node.selected.iter().copied().collect()))
            .collect()
    }

    /// `(cards, headings, tokens)` currently chosen.
    pub fn cost(&self) -> (usize, usize, usize) {
        let mut cards = 0;
        let mut headings = 0;
        let mut tokens = 0;
        for node in &self.nodes {
            if node.selected.is_empty() {
                continue;
            }
            cards += 1;
            headings += node.selected.len();
            tokens += tokens_of(&node.card.header())
                + node.selected.iter().map(|i| node.tokens[*i]).sum::<usize>();
        }
        (cards, headings, tokens)
    }

    /// `(cards, headings, ticked, boxes)` in the whole store.
    pub fn totals(&self) -> (usize, usize, usize, usize) {
        let headings = self.nodes.iter().map(|n| n.outline.nodes.len()).sum();
        let (done, total) = self
            .nodes
            .iter()
            .map(|n| n.card.progress())
            .fold((0, 0), |(d, t), (nd, nt)| (d + nd, t + nt));
        (self.nodes.len(), headings, done, total)
    }

    /// Open every fold. Used by the tests to reach the rows a key press
    /// would reach; the picker itself opens one row at a time.
    #[cfg(test)]
    fn open_everything(&mut self) {
        for node in &mut self.nodes {
            node.expanded = true;
            node.open = (0..node.outline.nodes.len()).collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card shaped like the one that exposed the outline bug: a README with
    /// its own title and headings, including a `## now` of its own.
    const DOCKET: &str = "\
# docket-cli
id: 133f
status: active

## now
[ ] ship it

## next

## readme

# docket

Intro.

## The loop

dk add .

## Install

cargo install
";

    fn picker() -> Picker {
        Picker::new(vec![
            Card::parse("docket-cli", DOCKET, 0),
            Card::parse("kol", "# kol\nid: bbbb\n\n## now\nnothing\n", 0),
        ])
    }

    /// Every row, with all folds open — the state a few `→` presses reach.
    fn opened() -> Picker {
        let mut p = picker();
        p.open_everything();
        p
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
    fn a_readmes_headings_are_not_sections_of_the_card() {
        let p = opened();
        let roots: Vec<String> = p
            .rows()
            .iter()
            .filter(|r| matches!(r.kind, Kind::Node { .. }) && r.depth == 0)
            .map(|r| r.text.clone())
            .collect();
        assert_eq!(roots, vec!["now", "next", "readme", "now"], "kol's now is last");
    }

    #[test]
    fn opening_the_readme_reveals_its_own_document() {
        let p = opened();
        let titles: Vec<String> = p
            .rows()
            .iter()
            .filter(|r| matches!(r.kind, Kind::Node { .. }))
            .map(|r| r.text.clone())
            .collect();
        assert!(titles.contains(&"docket".to_string()));
        assert!(titles.contains(&"The loop".to_string()));
        assert!(titles.contains(&"Install".to_string()));
    }

    #[test]
    fn stems_deepen_with_the_tree() {
        let p = opened();
        assert!(row_named(&p, "Install").prefix.len() > row_named(&p, "readme").prefix.len());
    }

    #[test]
    fn folds_open_and_shut_one_row_at_a_time() {
        let mut p = picker();
        let card_row = p.rows()[0].clone();
        p.toggle_fold(&card_row);
        assert!(p.rows().iter().any(|r| r.text == "readme"));
        assert!(!p.rows().iter().any(|r| r.text == "Install"), "not two levels at once");

        let readme = row_named(&p, "readme");
        p.fold(&readme, true);
        assert!(p.rows().iter().any(|r| r.text == "docket"));

        p.fold(&card_row, false);
        assert_eq!(p.rows().len(), 2);
    }

    #[test]
    fn picking_a_heading_takes_everything_under_it() {
        let mut p = opened();
        p.toggle_select(&row_named(&p, "readme"));

        let (name, chosen) = p.selection().into_iter().next().unwrap();
        assert_eq!(name, "docket-cli");
        assert_eq!(chosen.len(), 4, "readme, its title, The loop, Install");
        assert_eq!(row_named(&p, "readme").mark, Mark::All);
        assert_eq!(p.rows()[0].mark, Mark::Partial, "the card is only partly taken");
    }

    #[test]
    fn a_readme_subsection_can_be_taken_alone() {
        let mut p = opened();
        p.toggle_select(&row_named(&p, "Install"));
        assert_eq!(row_named(&p, "Install").mark, Mark::All);
        assert_eq!(row_named(&p, "readme").mark, Mark::Partial);
        assert_eq!(row_named(&p, "The loop").mark, Mark::None);
        assert_eq!(p.selection()[0].1.len(), 1);
    }

    #[test]
    fn a_line_takes_the_heading_it_belongs_to() {
        let mut p = opened();
        p.toggle_select(&row_named(&p, "[ ] ship it"));
        assert_eq!(row_named(&p, "now").mark, Mark::All);
    }

    #[test]
    fn picking_a_card_takes_its_whole_outline() {
        let mut p = picker();
        let card_row = p.rows()[0].clone();
        p.toggle_select(&card_row);
        assert_eq!(p.rows()[0].mark, Mark::All);
        assert_eq!(p.selection()[0].1.len(), 6);
        p.toggle_select(&card_row);
        assert!(p.selection().is_empty());
    }

    #[test]
    fn select_all_toggles_both_ways() {
        let mut p = picker();
        p.select_all();
        assert_eq!(p.cost().0, 2);
        p.select_all();
        assert_eq!(p.cost().0, 0);
    }


    #[test]
    fn cost_counts_the_header_once_per_card() {
        let mut p = picker();
        assert_eq!(p.cost(), (0, 0, 0));
        p.select_all();
        let (cards, headings, tokens) = p.cost();
        assert_eq!((cards, headings), (2, 7));
        assert!(tokens > 0);
    }

    #[test]
    fn blank_lines_do_not_become_rows() {
        let mut p = Picker::new(vec![Card::parse("x", "# x\nid: a\n\n## now\n\n\nreal\n\n", 0)]);
        p.open_everything();
        let lines = p.rows().into_iter().filter(|r| matches!(r.kind, Kind::Line { .. })).count();
        assert_eq!(lines, 1);
    }

    #[test]
    fn an_empty_card_still_shows_up_and_cannot_be_folded() {
        let p = Picker::new(vec![Card::parse("bare", "# bare\nid: a\n", 0)]);
        let rows = p.rows();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].foldable);
    }
}
