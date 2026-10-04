//! A card's heading structure.
//!
//! A card has five sections of its own — `## now`, `## next`, and so on — but
//! its `## readme` holds someone else's document, with its own `#` title and
//! its own `##` headings. Splitting on every `## ` makes those look like
//! sections of the card, which is wrong and, for a README with fifteen
//! headings, unreadable.
//!
//! The rule, in order:
//!
//! 1. The card's own sections are level-2 headings, starting at the first one.
//! 2. The run must be unbroken. The first heading that is not a fresh level-2
//!    heading ends it — a `#` title, a `###`, or a repeat of a heading already
//!    used. This is what stops a README's `# docket` from being a section.
//! 3. A heading already seen at the top is a duplicate and is never a second
//!    top-level section. A README that contains `## now` nests it under
//!    `## readme`, where it belongs.
//! 4. Everything after the run nests by heading depth, `ygg`-style but over
//!    text rather than files.

/// One heading and the lines directly under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The heading line as written, `## now` or `### Install`.
    pub heading: String,
    /// Number of leading `#`.
    pub level: usize,
    /// Body lines belonging to this heading, before any deeper heading.
    pub lines: Vec<String>,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    /// Depth in the tree, roots at 0 — not the same as `level`, because a
    /// README's `#` title sits under a `##` section.
    pub depth: usize,
}

impl Node {
    /// The heading without its `#` marks.
    pub fn title(&self) -> &str {
        self.heading.trim_start_matches('#').trim()
    }
}

/// A card's headings as a tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    pub nodes: Vec<Node>,
    /// Indices of the card's own sections, in order.
    pub roots: Vec<usize>,
}

impl Outline {
    /// Hang another document under a new root heading of this one.
    ///
    /// This is how a linked README joins a card's outline: the card's own
    /// sections are parsed from the card, the README is parsed from its file,
    /// and the two are grafted together so the picker sees one tree. Indices
    /// of the existing nodes do not move, so anything already holding them
    /// stays valid.
    pub fn graft(&mut self, heading: &str, lines: Vec<String>, other: Outline) -> usize {
        let root = self.nodes.len();
        self.nodes.push(Node {
            heading: heading.to_string(),
            level: 2,
            lines,
            children: Vec::new(),
            parent: None,
            depth: 0,
        });
        self.roots.push(root);

        let offset = self.nodes.len();
        for mut node in other.nodes {
            node.depth += 1;
            node.parent = Some(node.parent.map_or(root, |p| p + offset));
            for child in &mut node.children {
                *child += offset;
            }
            self.nodes.push(node);
        }
        for child in other.roots {
            self.nodes[root].children.push(child + offset);
        }
        root
    }

    /// Every descendant of `index`, including itself, in document order.
    pub fn subtree(&self, index: usize) -> Vec<usize> {
        let mut out = vec![index];
        let mut queue = vec![index];
        while let Some(at) = queue.pop() {
            for child in &self.nodes[at].children {
                out.push(*child);
                queue.push(*child);
            }
        }
        out.sort_unstable();
        out
    }

    /// Text of a node: its heading, its own lines, nothing of its children.
    pub fn text_of(&self, index: usize) -> String {
        let node = &self.nodes[index];
        let body = node.lines.join("\n");
        let body = body.trim_end();
        if body.is_empty() {
            format!("{}\n", node.heading)
        } else {
            format!("{}\n{}\n", node.heading, body)
        }
    }
}

/// Split a markdown body into the card's sections and everything nested in
/// them. Lines before the first level-2 heading are the card's header and are
/// not part of the outline.
pub fn parse(body: &str) -> Outline {
    let mut outline = Outline::default();
    let mut used: Vec<String> = Vec::new();
    // (level, index) of the open ancestors, innermost last.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut spine = true;
    let mut current: Option<usize> = None;
    // A `# comment` inside a fenced block is shell, not a heading, and every
    // README has one.
    let mut fence: Option<usize> = None;

    for line in body.lines() {
        let ticks = fence_marker(line);
        match (fence, ticks) {
            (None, Some(n)) => fence = Some(n),
            (Some(open), Some(n)) if n >= open => fence = None,
            _ => {}
        }
        let in_fence = fence.is_some() || ticks.is_some();

        let Some((level, _)) = (if in_fence { None } else { heading(line) }) else {
            if let Some(at) = current {
                outline.nodes[at].lines.push(line.to_string());
            }
            continue;
        };
        let title = line.trim_start_matches('#').trim().to_ascii_lowercase();

        // Rules 1 to 3: a fresh level-2 heading continues the card's own run.
        let is_root = spine && level == 2 && !used.contains(&title);
        if is_root {
            // `readme` is terminal. Everything after it is the project's own
            // document, and a README that opens with HTML rather than a `#`
            // title — a centred logo, a badge block — would otherwise let its
            // first `## Install` look like a fresh section of the card.
            if title == "readme" {
                spine = false;
            }
            used.push(title);
            let index = push(&mut outline, line, level, None, 0);
            outline.roots.push(index);
            stack = vec![(level, index)];
            current = Some(index);
            continue;
        }
        spine = false;

        // Rule 4: nest by depth, never above the section we are inside.
        let Some(&(_, root)) = stack.first() else {
            // Nothing is open yet, so this heading is above the first `##` —
            // the card's own `# name` title. That belongs to the header, not
            // the outline, and treating it as a section would break the run
            // before the real sections start.
            spine = true;
            continue;
        };
        while stack.len() > 1 && stack.last().is_some_and(|(open, _)| *open >= level) {
            stack.pop();
        }
        let parent = stack.last().map(|(_, at)| *at).unwrap_or(root);
        let depth = outline.nodes[parent].depth + 1;
        let index = push(&mut outline, line, level, Some(parent), depth);
        outline.nodes[parent].children.push(index);
        stack.push((level, index));
        current = Some(index);
    }

    outline
}

/// Parse an ordinary markdown document: every heading is part of the tree,
/// nested by depth, with the shallowest ones as roots.
///
/// This is what a linked README gets. `parse` above is for cards, where the
/// rules exist to tell the card's own five sections from whatever document
/// follows them — applied to a README those rules would throw away its `#`
/// title, which is the one heading naming the thing.
pub fn parse_document(text: &str) -> Outline {
    let mut outline = Outline::default();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<usize> = None;
    let mut fence: Option<usize> = None;

    for line in text.lines() {
        let ticks = fence_marker(line);
        match (fence, ticks) {
            (None, Some(n)) => fence = Some(n),
            (Some(open), Some(n)) if n >= open => fence = None,
            _ => {}
        }
        let in_fence = fence.is_some() || ticks.is_some();

        let Some((level, _)) = (if in_fence { None } else { heading(line) }) else {
            if let Some(at) = current {
                outline.nodes[at].lines.push(line.to_string());
            }
            continue;
        };

        while stack.last().is_some_and(|(open, _)| *open >= level) {
            stack.pop();
        }
        let parent = stack.last().map(|(_, at)| *at);
        let depth = parent.map_or(0, |at| outline.nodes[at].depth + 1);
        let index = push(&mut outline, line, level, parent, depth);
        match parent {
            Some(at) => outline.nodes[at].children.push(index),
            None => outline.roots.push(index),
        }
        stack.push((level, index));
        current = Some(index);
    }

    outline
}

fn push(outline: &mut Outline, heading: &str, level: usize, parent: Option<usize>, depth: usize) -> usize {
    outline.nodes.push(Node {
        heading: heading.trim_end().to_string(),
        level,
        lines: Vec::new(),
        children: Vec::new(),
        parent,
        depth,
    });
    outline.nodes.len() - 1
}

/// The length of a fence marker, for a line that is one.
pub(crate) fn fence_marker(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    for mark in ['`', '~'] {
        let run = trimmed.chars().take_while(|c| *c == mark).count();
        if run >= 3 {
            return Some(run);
        }
    }
    None
}

/// `(level, text)` for a heading line, `None` for anything else. A `#` with no
/// space after it is not a heading — `#1 priority` is prose.
pub(crate) fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    let text = rest.strip_prefix(' ')?.trim();
    (!text.is_empty()).then_some((level, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The card that exposed the problem: a README carrying its own `#` title
    /// and a `## now` of its own.
    const DOCKET_CARD: &str = "\
# docket-cli
id: 133f61fc
status: active

## now

## next

## open questions

## notes

## readme

# docket

Intro prose.

## The loop

```
dk add .
```

## Install

cargo install docket-cli

### From source

cargo build

## Commands

a table
";

    fn titles(outline: &Outline, indices: &[usize]) -> Vec<String> {
        indices.iter().map(|i| outline.nodes[*i].title().to_string()).collect()
    }

    #[test]
    fn the_cards_own_sections_are_the_roots() {
        let outline = parse(DOCKET_CARD);
        assert_eq!(
            titles(&outline, &outline.roots),
            vec!["now", "next", "open questions", "notes", "readme"]
        );
    }

    #[test]
    fn a_readmes_headings_nest_under_readme() {
        let outline = parse(DOCKET_CARD);
        let readme = *outline.roots.last().unwrap();
        // The README's `# docket` title is the only direct child; its own
        // `##` headings hang off that.
        assert_eq!(titles(&outline, &outline.nodes[readme].children), vec!["docket"]);

        let docket = outline.nodes[readme].children[0];
        assert_eq!(
            titles(&outline, &outline.nodes[docket].children),
            vec!["The loop", "Install", "Commands"]
        );
    }

    #[test]
    fn deeper_headings_nest_deeper() {
        let outline = parse(DOCKET_CARD);
        let install = outline
            .nodes
            .iter()
            .position(|n| n.title() == "Install")
            .unwrap();
        assert_eq!(titles(&outline, &outline.nodes[install].children), vec!["From source"]);
        assert_eq!(outline.nodes[install].depth + 1, outline.nodes[outline.nodes[install].children[0]].depth);
    }

    #[test]
    fn a_readme_that_opens_with_html_still_ends_the_run() {
        // FUR's README: a centred logo and badges, no `#` title at all, then
        // `## Why FUR exists`. Without the terminal rule that heading becomes
        // a section of the card.
        let outline = parse(
            "## now\nx\n\n## readme\n\n<p align=\"center\">\n<h1>FUR</h1>\n</p>\n\n## Why FUR exists\nchats vanish\n\n## Installation\ncargo install\n",
        );
        assert_eq!(titles(&outline, &outline.roots), vec!["now", "readme"]);
        let readme = outline.roots[1];
        assert_eq!(
            titles(&outline, &outline.nodes[readme].children),
            vec!["Why FUR exists", "Installation"]
        );
    }

    #[test]
    fn sections_after_the_readme_are_never_roots_even_when_fresh() {
        let outline = parse("## now\na\n\n## readme\nb\n\n## Usage\nc\n\n## Language\nd\n");
        assert_eq!(titles(&outline, &outline.roots), vec!["now", "readme"]);
    }

    #[test]
    fn a_duplicate_heading_is_not_a_second_root() {
        let outline = parse("## now\na\n\n## readme\n\n## now\ntheirs\n");
        assert_eq!(titles(&outline, &outline.roots), vec!["now", "readme"]);
        let readme = outline.roots[1];
        assert_eq!(titles(&outline, &outline.nodes[readme].children), vec!["now"]);
    }

    #[test]
    fn the_run_never_resumes_once_broken() {
        // `### deep` breaks the run; `## after` is nested, not a new section.
        let outline = parse("## now\na\n\n### deep\nb\n\n## after\nc\n");
        assert_eq!(titles(&outline, &outline.roots), vec!["now"]);
        assert_eq!(titles(&outline, &outline.nodes[0].children), vec!["deep", "after"]);
    }

    #[test]
    fn lines_belong_to_the_heading_above_them() {
        let outline = parse("## now\nparser\nwork\n\n## next\nspans\n");
        assert_eq!(
            outline.nodes[0].lines.iter().filter(|l| !l.trim().is_empty()).count(),
            2
        );
        assert_eq!(outline.text_of(1).trim_end(), "## next\nspans");
    }

    #[test]
    fn a_title_above_the_sections_does_not_start_the_tree() {
        // The exact shape of a real card: `# name` then field lines, then the
        // sections. The title is header, and the run starts at `## now`.
        let outline = parse("# docket-cli\nid: 133f\nstatus: active\n\n## now\nx\n\n## next\n");
        assert_eq!(titles(&outline, &outline.roots), vec!["now", "next"]);
    }

    #[test]
    fn the_header_is_not_part_of_the_outline() {
        let outline = parse("# moxi\nid: a43b\nstatus: active\n\n## now\nx\n");
        assert_eq!(titles(&outline, &outline.roots), vec!["now"]);
        assert_eq!(outline.nodes.len(), 1);
    }

    #[test]
    fn a_card_with_no_sections_has_no_outline() {
        assert_eq!(parse("# moxi\nid: a43b\n").nodes.len(), 0);
    }

    #[test]
    fn a_document_keeps_its_title_as_the_root() {
        let doc = parse_document("# The Tool\n\nIntro.\n\n## Install\nsteps\n\n## Usage\nrun it\n");
        assert_eq!(titles(&doc, &doc.roots), vec!["The Tool"]);
        assert_eq!(
            titles(&doc, &doc.nodes[0].children),
            vec!["Install", "Usage"]
        );
    }

    #[test]
    fn a_document_with_no_title_has_several_roots() {
        // FUR's README: an HTML masthead, then `##` headings all the way.
        let doc = parse_document("<p>logo</p>\n\n## Why\na\n\n## Install\nb\n");
        assert_eq!(titles(&doc, &doc.roots), vec!["Why", "Install"]);
    }

    #[test]
    fn a_document_nests_by_depth_not_by_order() {
        let doc = parse_document("# T\n\n## A\n\n### A1\n\n## B\n");
        assert_eq!(titles(&doc, &doc.nodes[0].children), vec!["A", "B"]);
        let a = doc.nodes[0].children[0];
        assert_eq!(titles(&doc, &doc.nodes[a].children), vec!["A1"]);
    }

    #[test]
    fn a_document_ignores_headings_inside_fences() {
        let doc = parse_document("# T\n\n```\n# not a heading\n```\n\n## Real\n");
        assert_eq!(titles(&doc, &doc.nodes[0].children), vec!["Real"]);
    }

    #[test]
    fn grafting_hangs_a_document_under_a_new_root() {
        let mut card = parse("## now\nparser\n\n## next\nspans\n");
        let readme = parse_document("# The Tool\n\nIntro.\n\n## Install\nsteps\n");
        let at = card.graft("## readme", vec![], readme);

        let roots: Vec<&str> = card.roots.iter().map(|i| card.nodes[*i].title()).collect();
        assert_eq!(roots, vec!["now", "next", "readme"]);
        assert_eq!(card.nodes[at].depth, 0);

        let children: Vec<&str> = card.nodes[at]
            .children
            .iter()
            .map(|i| card.nodes[*i].title())
            .collect();
        assert_eq!(children, vec!["The Tool"]);

        // The README's own structure survives, one level deeper.
        let tool = card.nodes[at].children[0];
        assert_eq!(card.nodes[tool].depth, 1);
        let inner: Vec<&str> = card.nodes[tool]
            .children
            .iter()
            .map(|i| card.nodes[*i].title())
            .collect();
        assert_eq!(inner, vec!["Install"]);
        assert_eq!(card.nodes[card.nodes[tool].children[0]].depth, 2);
    }

    #[test]
    fn grafting_leaves_existing_indices_alone() {
        let mut card = parse("## now\nparser\n");
        let before = card.text_of(0);
        card.graft("## readme", vec![], parse_document("## Install\nsteps\n"));
        assert_eq!(card.text_of(0), before);
    }

    #[test]
    fn subtree_is_the_node_and_its_descendants_in_order() {
        let outline = parse(DOCKET_CARD);
        let readme = *outline.roots.last().unwrap();
        let subtree = outline.subtree(readme);
        assert!(subtree.len() >= 6, "readme plus its whole document");
        assert!(subtree.windows(2).all(|w| w[0] < w[1]), "document order");
        assert_eq!(outline.subtree(outline.roots[0]), vec![outline.roots[0]]);
    }

    #[test]
    fn hashes_without_a_space_are_prose() {
        let outline = parse("## now\n#1 priority is spans\n#### \n");
        assert_eq!(outline.roots.len(), 1);
        assert_eq!(outline.nodes.len(), 1, "neither line is a heading");
    }

    #[test]
    fn shell_comments_inside_a_fence_are_not_headings() {
        let outline = parse(
            "## now\n```bash\ndk add .   # register it\n# and another\n```\n## next\nx\n",
        );
        assert_eq!(titles(&outline, &outline.roots), vec!["now", "next"]);
        assert_eq!(outline.nodes.len(), 2);
    }

    #[test]
    fn a_longer_fence_can_hold_a_shorter_one() {
        let outline = parse("## now\n````\n```\n# inner\n```\n````\n## next\nx\n");
        assert_eq!(titles(&outline, &outline.roots), vec!["now", "next"]);
    }
}
