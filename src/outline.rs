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
fn fence_marker(line: &str) -> Option<usize> {
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
fn heading(line: &str) -> Option<(usize, &str)> {
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
