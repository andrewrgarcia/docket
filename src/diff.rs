/// A line-level diff, used only to show what an AI reply would change before
/// it is written. Cards are a few dozen lines, so an O(n·m) LCS is free and
/// exact; anything larger falls back to a wholesale replace.
#[derive(Debug, PartialEq, Eq)]
pub enum Change<'a> {
    Same(&'a str),
    Added(&'a str),
    Removed(&'a str),
}

const CELL_BUDGET: usize = 1_000_000;

pub fn diff<'a>(old: &'a str, new: &'a str) -> Vec<Change<'a>> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let (n, m) = (a.len(), b.len());

    if n.saturating_mul(m) > CELL_BUDGET {
        let mut out: Vec<Change> = a.iter().map(|l| Change::Removed(*l)).collect();
        out.extend(b.iter().map(|l| Change::Added(*l)));
        return out;
    }

    // lcs[i][j] = length of the longest common subsequence of a[i..] and b[j..]
    let width = m + 1;
    let mut lcs = vec![0usize; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * width + j] = if a[i] == b[j] {
                lcs[(i + 1) * width + j + 1] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }

    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push(Change::Same(a[i]));
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
            out.push(Change::Removed(a[i]));
            i += 1;
        } else {
            out.push(Change::Added(b[j]));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|l| Change::Removed(*l)));
    out.extend(b[j..].iter().map(|l| Change::Added(*l)));
    out
}

/// `(added, removed)`.
pub fn tally(changes: &[Change<'_>]) -> (usize, usize) {
    changes.iter().fold((0, 0), |(add, del), c| match c {
        Change::Added(_) => (add + 1, del),
        Change::Removed(_) => (add, del + 1),
        Change::Same(_) => (add, del),
    })
}

/// Only the changed lines, prefixed. Context is omitted deliberately: the user
/// already knows what the card says.
pub fn render(changes: &[Change<'_>]) -> String {
    let mut out = String::new();
    for change in changes {
        match change {
            Change::Added(line) => {
                out.push_str("    + ");
                out.push_str(line);
                out.push('\n');
            }
            Change::Removed(line) => {
                out.push_str("    - ");
                out.push_str(line);
                out.push('\n');
            }
            Change::Same(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_changes() {
        let changes = diff("a\nb\n", "a\nb\n");
        assert_eq!(tally(&changes), (0, 0));
    }

    #[test]
    fn detects_an_inserted_line() {
        let changes = diff("a\nc\n", "a\nb\nc\n");
        assert_eq!(tally(&changes), (1, 0));
        assert!(render(&changes).contains("+ b"));
    }

    #[test]
    fn detects_a_replacement() {
        let changes = diff("status: idea\n", "status: active\n");
        assert_eq!(tally(&changes), (1, 1));
    }

    #[test]
    fn handles_empty_sides() {
        assert_eq!(tally(&diff("", "a\nb\n")), (2, 0));
        assert_eq!(tally(&diff("a\nb\n", "")), (0, 2));
    }

    #[test]
    fn keeps_common_lines_out_of_the_render() {
        let rendered = render(&diff("keep\nold\n", "keep\nnew\n"));
        assert!(!rendered.contains("keep"));
        assert!(rendered.contains("- old"));
        assert!(rendered.contains("+ new"));
    }
}
