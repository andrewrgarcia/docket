use crate::card::slug;

/// One card recovered from an AI reply.
#[derive(Debug, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub body: String,
}

/// Pull card blocks out of arbitrary prose. A block counts only if it is
/// fenced and its first non-empty line is `# <name>` — that pairing is rare
/// enough in ordinary text to be a reliable signal, and it means the model can
/// write whatever it likes around the blocks.
///
/// Nested fences (a code sample inside a card) are handled by matching the
/// closing fence to the opening one's length, the way CommonMark does.
pub fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut open: Option<(usize, Vec<String>)> = None;

    for line in text.lines() {
        let trimmed = line.trim_start();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        let is_fence = ticks >= 3;

        // A closing fence is at least as long as the opening one and carries
        // no info string. Decided before touching `open` to keep one borrow.
        let closes = match &open {
            Some((opening, _)) => {
                is_fence && ticks >= *opening && trimmed[ticks..].trim().is_empty()
            }
            None => false,
        };

        if closes {
            let (_, collected) = open.take().expect("closes implies an open fence");
            if let Some(block) = finish(collected) {
                blocks.push(block);
            }
        } else if let Some((_, collected)) = open.as_mut() {
            collected.push(line.to_string());
        } else if is_fence {
            open = Some((ticks, Vec::new()));
        }
    }

    blocks
}

fn finish(lines: Vec<String>) -> Option<Block> {
    let first = lines.iter().find(|l| !l.trim().is_empty())?;
    let heading = first.trim().strip_prefix("# ")?;
    let name = slug(heading);
    if name.is_empty() {
        return None;
    }
    let body = lines.join("\n").trim_end().to_string();
    Some(Block { name, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_card_amid_prose() {
        let reply = "Sure, here is the tightened card.\n\n\
                     ```\n# moxi\nstatus: active\nwhat: a language\n```\n\n\
                     Let me know if that works.";
        let blocks = parse(reply);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "moxi");
        assert!(blocks[0].body.starts_with("# moxi"));
    }

    #[test]
    fn ignores_ordinary_code_blocks() {
        let reply = "```rust\nfn main() {}\n```\n```\njust text\n```";
        assert!(parse(reply).is_empty());
    }

    #[test]
    fn reads_several_cards() {
        let reply = "```\n# moxi\nstatus: active\n```\ntext\n```\n# ygg\nstatus: paused\n```";
        let names: Vec<&str> = parse(reply).iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["moxi", "ygg"]);
    }

    #[test]
    fn survives_a_nested_fence() {
        let reply = "````\n# moxi\nstatus: active\n\n## notes\n```\ncargo run\n```\n````";
        let blocks = parse(reply);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].body.contains("cargo run"));
    }

    #[test]
    fn an_unclosed_fence_yields_nothing() {
        assert!(parse("```\n# moxi\nstatus: active\n").is_empty());
    }

    #[test]
    fn heading_is_slugged_so_a_chatty_title_still_lands() {
        let blocks = parse("```\n# Moxi Lang\nstatus: active\n```");
        assert_eq!(blocks[0].name, "moxi-lang");
    }
}
