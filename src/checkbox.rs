//! Checkbox lines. `[ ] thing` and `[x] thing`, with or without a leading
//! `- `, anywhere in a card's body. This is the one piece of structure inside
//! the free-text sections, and it is deliberately just GitHub's checklist
//! syntax so the cards read correctly on any markdown renderer.

/// Where the checkbox sits in a line, if there is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkbox {
    /// Byte offset of the character between the brackets.
    pub mark: usize,
    pub done: bool,
}

pub fn find(line: &str) -> Option<Checkbox> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let after_bullet = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .unwrap_or(trimmed);
    let bullet = trimmed.len() - after_bullet.len();

    let rest = after_bullet.strip_prefix('[')?;
    let mark_char = rest.chars().next()?;
    let done = match mark_char {
        ' ' => false,
        'x' | 'X' => true,
        _ => return None,
    };
    if !rest[mark_char.len_utf8()..].starts_with(']') {
        return None;
    }
    Some(Checkbox {
        mark: indent + bullet + 1,
        done,
    })
}

/// The line with its box flipped, or unchanged when it has no box.
pub fn toggle(line: &str) -> Option<String> {
    let cb = find(line)?;
    let mut out = String::with_capacity(line.len());
    out.push_str(&line[..cb.mark]);
    out.push(if cb.done { ' ' } else { 'x' });
    out.push_str(&line[cb.mark + 1..]);
    Some(out)
}

/// `(done, total)` across a card's text.
pub fn tally(text: &str) -> (usize, usize) {
    text.lines().filter_map(find).fold((0, 0), |(done, total), cb| {
        (done + usize::from(cb.done), total + 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_boxes_with_and_without_bullets() {
        assert_eq!(find("[ ] a").map(|c| c.done), Some(false));
        assert_eq!(find("[x] a").map(|c| c.done), Some(true));
        assert_eq!(find("[X] a").map(|c| c.done), Some(true));
        assert_eq!(find("- [ ] a").map(|c| c.done), Some(false));
        assert_eq!(find("  * [x] a").map(|c| c.done), Some(true));
    }

    #[test]
    fn ignores_things_that_only_look_like_boxes() {
        assert!(find("[link](url)").is_none());
        assert!(find("[ok] fine").is_none());
        assert!(find("see [ ] later").is_none());
        assert!(find("[]").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn toggling_flips_only_the_mark() {
        assert_eq!(toggle("[ ] write tests").as_deref(), Some("[x] write tests"));
        assert_eq!(toggle("  - [x] done").as_deref(), Some("  - [ ] done"));
        assert_eq!(toggle("[X] loud").as_deref(), Some("[ ] loud"));
        assert!(toggle("plain prose").is_none());
    }

    #[test]
    fn tally_counts_done_over_total() {
        let text = "## now\n[ ] a\n[x] b\nprose\n- [x] c\n\n## next\n[ ] d\n";
        assert_eq!(tally(text), (2, 4));
        assert_eq!(tally("no boxes here"), (0, 0));
    }

    #[test]
    fn toggle_keeps_unicode_after_the_box_intact() {
        assert_eq!(toggle("[ ] añadir 日本").as_deref(), Some("[x] añadir 日本"));
    }
}
