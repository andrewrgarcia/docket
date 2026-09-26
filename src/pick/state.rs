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
use crate::checkbox;
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
                    dirty: false,
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

    pub fn toggle_all_folds(&mut self) {
        let all_open = self
            .nodes
            .iter()
            .all(|n| n.expanded && n.open.len() == n.outline.nodes.len());
        for node in &mut self.nodes {
            node.expanded = !all_open;
            node.open = if all_open {
                BTreeSet::new()
            } else {
                (0..node.outline.nodes.len()).collect()
            };
        }
    }

    /// Cards open, their own sections visible, nothing deeper: the overview.
    pub fn expand_cards(&mut self) {
        for node in &mut self.nodes {
            node.expanded = true;
            node.open.clear();
        }
    }

    pub fn collapse_all(&mut self) {
        for node in &mut self.nodes {
            node.expanded = false;
            node.open.clear();
        }
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

    pub fn select_none(&mut self) {
        for node in &mut self.nodes {
            node.selected.clear();
        }
    }

    /// One of the card's own sections, across every card — how you take "just
    /// the `now` of everything" in a keystroke. Only roots match, so a
    /// README's own `## now` is never swept up.
    pub fn select_heading(&mut self, title: &str) {
        let matches = |node: &Node| -> Vec<usize> {
            node.outline
                .roots
                .iter()
                .filter(|at| node.outline.nodes[**at].title().eq_ignore_ascii_case(title))
                .flat_map(|at| node.outline.subtree(*at))
                .collect()
        };
        let any_off = self
            .nodes
            .iter()
            .any(|n| matches(n).iter().any(|at| !n.selected.contains(at)));

        for node in &mut self.nodes {
            for at in matches(node) {
                if any_off {
                    node.selected.insert(at);
                } else {
                    node.selected.remove(&at);
                }
            }
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

    // -- ticking -----------------------------------------------------------

    /// Flip a checkbox, in the view and in the card's text. Returns the card
    /// index when something changed, so the caller can save it.
    pub fn tick(&mut self, row: &Row) -> Option<usize> {
        let Kind::Line { node: at, line } = row.kind else {
            return None;
        };
        let node = &mut self.nodes[row.card];
        let old = node.shown[at][line].clone();
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
        node.shown[at][line] = flipped.clone();
        if let Some(slot) = node.outline.nodes[at].lines.iter_mut().find(|l| **l == old) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card shaped like the one that exposed the bug: a README with its own
    /// title and headings, including a `## now` of its own.
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

    fn row_named(p: &Picker, text: &str) -> Row {
        p.rows().into_iter().find(|r| r.text == text).expect(text)
    }

    #[test]
    fn a_readmes_headings_are_not_sections_of_the_card() {
        let mut p = picker();
        p.expand_cards();
        let visible: Vec<String> = p
            .rows()
            .iter()
            .filter(|r| matches!(r.kind, Kind::Node { .. }))
            .map(|r| r.text.clone())
            .collect();
        assert_eq!(visible, vec!["now", "next", "readme", "now"], "kol's now is last");
        assert!(!visible.contains(&"Install".to_string()), "nested, not top level");
    }

    #[test]
    fn opening_the_readme_reveals_its_own_document() {
        let mut p = picker();
        p.toggle_all_folds();
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
        let mut p = picker();
        p.toggle_all_folds();
        let install = row_named(&p, "Install");
        let readme = row_named(&p, "readme");
        assert!(install.prefix.len() > readme.prefix.len(), "{:?}", install.prefix);
    }

    #[test]
    fn picking_a_heading_takes_everything_under_it() {
        let mut p = picker();
        p.toggle_all_folds();
        p.toggle_select(&row_named(&p, "readme"));

        let (name, chosen) = p.selection().into_iter().next().unwrap();
        assert_eq!(name, "docket-cli");
        // readme + its `# docket` + The loop + Install
        assert_eq!(chosen.len(), 4);
        assert_eq!(row_named(&p, "readme").mark, Mark::All);
        assert_eq!(p.rows()[0].mark, Mark::Partial, "the card is only partly taken");
    }

    #[test]
    fn a_readme_subsection_can_be_taken_alone() {
        let mut p = picker();
        p.toggle_all_folds();
        p.toggle_select(&row_named(&p, "Install"));

        assert_eq!(row_named(&p, "Install").mark, Mark::All);
        assert_eq!(row_named(&p, "readme").mark, Mark::Partial);
        assert_eq!(row_named(&p, "The loop").mark, Mark::None);
        assert_eq!(p.selection()[0].1.len(), 1);
    }

    #[test]
    fn select_heading_matches_the_cards_own_sections_only() {
        let mut p = picker();
        p.select_heading("now");
        let picked: Vec<(String, usize)> = p
            .selection()
            .into_iter()
            .map(|(name, chosen)| (name, chosen.len()))
            .collect();
        assert_eq!(picked, vec![("docket-cli".to_string(), 1), ("kol".to_string(), 1)]);
    }

    #[test]
    fn picking_a_card_takes_its_whole_outline() {
        let mut p = picker();
        let card_row = p.rows()[0].clone();
        p.toggle_select(&card_row);
        assert_eq!(p.rows()[0].mark, Mark::All);
        assert_eq!(p.selection()[0].1.len(), 6, "three sections plus the readme's three");
        p.toggle_select(&card_row);
        assert!(p.selection().is_empty());
    }

    #[test]
    fn ticking_a_box_rewrites_the_card_and_the_view() {
        let mut p = picker();
        p.toggle_all_folds();
        let row = row_named(&p, "[ ] ship it");
        assert_eq!(p.tick(&row), Some(0));
        assert!(p.card(0).body.contains("[x] ship it"));
        assert!(p.rows().iter().any(|r| r.text == "[x] ship it"));
        assert_eq!(p.take_dirty().len(), 1);
        assert!(p.take_dirty().is_empty());
    }

    #[test]
    fn ticking_prose_or_a_heading_does_nothing() {
        let mut p = picker();
        p.toggle_all_folds();
        assert_eq!(p.tick(&row_named(&p, "Intro.")), None);
        assert_eq!(p.tick(&row_named(&p, "readme")), None);
        assert!(p.take_dirty().is_empty());
    }

    #[test]
    fn folding_goes_cards_then_sections_then_lines() {
        let mut p = picker();
        assert_eq!(p.rows().len(), 2, "collapsed");
        p.expand_cards();
        assert!(p.rows().iter().any(|r| matches!(r.kind, Kind::Node { .. })));
        assert!(!p.rows().iter().any(|r| matches!(r.kind, Kind::Line { .. })));
        p.toggle_all_folds();
        assert!(p.rows().iter().any(|r| matches!(r.kind, Kind::Line { .. })));
        p.collapse_all();
        assert_eq!(p.rows().len(), 2);
    }

    #[test]
    fn a_cost_counts_the_header_once_per_card() {
        let mut p = picker();
        assert_eq!(p.cost(), (0, 0, 0));
        p.select_all();
        let (cards, headings, tokens) = p.cost();
        assert_eq!((cards, headings), (2, 7));
        assert!(tokens > 0);
        p.select_all();
        assert_eq!(p.cost().0, 0);
    }

    #[test]
    fn an_empty_card_still_shows_up_and_cannot_be_folded() {
        let p = Picker::new(vec![Card::parse("bare", "# bare\nid: a\n", 0)]);
        let rows = p.rows();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].foldable);
    }
}
