use std::io::{self, Write};

use crate::card::Card;
use crate::editor::syntax::{self, Kind};
use crate::error::{Error, Result};
use crate::theme::{self, BOLD, CYAN, DIM, GREEN, RESET};

/// Widths outside this range mean a terminal that did not report honestly.
const MIN_WIDTH: usize = 40;
const FALLBACK_WIDTH: usize = 80;

fn terminal_width() -> usize {
    if !theme::enabled() {
        return FALLBACK_WIDTH;
    }
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .ok()
        .filter(|w| *w >= MIN_WIDTH)
        .unwrap_or(FALLBACK_WIDTH)
}

/// The list.
pub fn table(cards: &[Card]) -> String {
    render_table(cards, terminal_width())
}

/// One row per card, clipped to `width`. Descriptions are routinely longer
/// than a terminal is wide, and a table that wraps stops being a table.
fn render_table(cards: &[Card], width: usize) -> String {
    if cards.is_empty() {
        return "no cards yet — `dk add .` in a project, or `dk add` for an idea\n".into();
    }

    let ids: Vec<String> = cards.iter().map(|c| c.id.clone()).collect();
    let shorts: Vec<String> = cards.iter().map(|c| c.short_id(&ids)).collect();

    let hash_w = shorts.iter().map(|s| s.len()).max().unwrap_or(4).max(4);

    // The PROGRESS column appears only when some card has a checkbox; a column
    // of blanks is noise.
    let tallies: Vec<(usize, usize)> = cards.iter().map(Card::progress).collect();
    let any_boxes = tallies.iter().any(|(_, total)| *total > 0);
    let progress_cells: Vec<String> = tallies
        .iter()
        .map(|(done, total)| if *total == 0 { String::new() } else { format!("{done}/{total}") })
        .collect();
    let progress_w = if any_boxes {
        progress_cells.iter().map(|c| c.len()).max().unwrap_or(5).max(5)
    } else {
        0
    };
    let name_w = cards
        .iter()
        .map(|c| c.name.chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    let status_w = cards
        .iter()
        .map(|c| c.status.as_str().len())
        .max()
        .unwrap_or(6)
        .max(6);

    // hash + name + status + age (+ progress), each followed by two spaces.
    let used = 2 + hash_w + 2 + name_w + 2 + status_w + 2 + 5 + 2
        + if any_boxes { progress_w + 2 } else { 0 };
    let what_w = width.saturating_sub(used);

    let mut out = String::new();
    let progress_head = if any_boxes { format!("{:>progress_w$}  ", "PROGRESS") } else { String::new() };
    out.push_str(&theme::paint(
        &format!(
            "  {:<hash_w$}  {:<name_w$}  {:<status_w$}  {:>5}  {progress_head}{}",
            "HASH",
            "NAME",
            "STATUS",
            "AGE",
            clip("WHAT", what_w)
        ),
        &[BOLD],
    ));
    out.push('\n');

    for ((card, short), cell) in cards.iter().zip(&shorts).zip(&progress_cells) {
        let cold = card.status.is_cold();

        let hash = theme::paint(&pad(short, hash_w), &[DIM]);
        let name = theme::paint(&pad(&card.name, name_w), if cold { &[DIM] } else { &[BOLD] });
        let status = theme::paint(
            &pad(card.status.as_str(), status_w),
            &[theme::status_color(card.status.as_str())],
        );
        let age = theme::paint(&format!("{:>4}d", card.age_days), &[DIM]);
        let progress = if any_boxes {
            let (done, total) = card.progress();
            let color = if total > 0 && done == total { GREEN } else { CYAN };
            format!("{}  ", theme::paint(&format!("{cell:>progress_w$}"), &[color]))
        } else {
            String::new()
        };
        let what = clip(&card.what, what_w);
        let what = if cold { theme::paint(&what, &[DIM]) } else { what };

        out.push_str(&format!("  {hash}  {name}  {status}  {age}  {progress}{what}"));
        out.push('\n');
    }

    out.push('\n');
    out.push_str(&theme::paint(&format!("{} cards", cards.len()), &[DIM]));
    out.push('\n');
    out
}

/// A card for reading: the same text, coloured by line kind.
pub fn render_card(card: &Card) -> String {
    if !theme::enabled() {
        let mut body = card.body.clone();
        if !body.ends_with('\n') {
            body.push('\n');
        }
        return body;
    }
    let lines: Vec<String> = card.body.lines().map(String::from).collect();
    let kinds = syntax::classify(&lines);
    let mut out = String::new();
    for (line, kind) in lines.iter().zip(kinds) {
        match kind {
            Kind::Body => out.push_str(line),
            other => out.push_str(&theme::paint(line, other.codes())),
        }
        out.push('\n');
    }
    out
}

fn pad(text: &str, width: usize) -> String {
    let used = text.chars().count();
    let mut out = text.to_string();
    out.extend(std::iter::repeat(' ').take(width.saturating_sub(used)));
    out
}

/// Cut to `width` characters, ending in an ellipsis when something was lost.
/// Counts characters, never bytes, so an accented word cannot be split.
fn clip(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".into();
    }
    let cut: String = text.chars().take(width - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Read one line. An EOF (piped input, closed stdin) reads as an empty answer,
/// which every caller treats as "no".
pub fn prompt(message: &str) -> Result<String> {
    print!("{}", theme::paint(message, &[CYAN]));
    io::stdout()
        .flush()
        .map_err(|e| Error::other(format!("cannot write to stdout: {e}")))?;
    let mut line = String::new();
    let read = io::stdin()
        .read_line(&mut line)
        .map_err(|e| Error::other(format!("cannot read from stdin: {e}")))?;
    if read == 0 {
        return Ok(String::new());
    }
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, status: &str, what: &str) -> Card {
        // A distinct id per name, so short ids stay four characters.
        let id = format!("{:08x}", seed(name));
        Card::parse(
            name,
            &format!("# {name}\nid: {id}\nstatus: {status}\nwhat: {what}\n"),
            7,
        )
    }

    /// Deterministic per-name filler, so tests do not depend on a clock.
    fn seed(name: &str) -> u32 {
        name.bytes().fold(0x9e37_79b9u32, |acc, b| {
            acc.rotate_left(5) ^ (b as u32).wrapping_mul(0x85eb_ca6b)
        })
    }

    /// Colour is off in tests (no terminal), so widths are measurable.
    #[test]
    fn empty_store_explains_the_next_step() {
        assert!(render_table(&[], 80).contains("dk add"));
    }

    #[test]
    fn no_row_exceeds_the_terminal_width() {
        let long = "Turn your AI chats into a durable, local-first diary. Save messages, attach notes, organize conversations.";
        let cards = [card("cli", "active", long), card("yggdrasil-cli", "active", long)];
        for width in [40usize, 60, 80, 120] {
            for line in render_table(&cards, width).lines() {
                assert!(
                    line.chars().count() <= width,
                    "width {width}: {} chars in {line:?}",
                    line.chars().count()
                );
            }
        }
    }

    #[test]
    fn clipped_descriptions_end_in_an_ellipsis() {
        let cards = [card("a", "active", "a description far longer than the space here")];
        assert!(render_table(&cards, 40).contains('…'));
    }

    #[test]
    fn the_progress_column_appears_only_when_a_card_has_boxes() {
        let plain = [card("a", "active", "x")];
        assert!(!render_table(&plain, 80).contains("PROGRESS"));

        let with = Card::parse(
            "b",
            "# b\nid: bbbb0000\nstatus: active\nwhat: y\n\n## now\n[x] one\n[ ] two\n",
            0,
        );
        let out = render_table(&[card("a", "active", "x"), with], 80);
        assert!(out.contains("PROGRESS"), "{out}");
        assert!(out.contains("1/2"), "{out}");
    }

    #[test]
    fn the_hash_column_shows_a_short_unique_prefix() {
        let cards = [card("a", "active", "x"), card("b", "idea", "y")];
        let out = render_table(&cards, 80);
        assert!(out.contains("HASH"));
        for (row, c) in out.lines().skip(1).take(2).zip(cards.iter()) {
            assert!(row.trim_start().starts_with(&c.id[..4]), "{row}");
        }
    }


    #[test]
    fn a_piped_card_is_the_file_verbatim() {
        let c = Card::parse("a", "# a\nid: aaaa0000\nstatus: active\n", 0);
        assert_eq!(render_card(&c), "# a\nid: aaaa0000\nstatus: active\n");
        assert!(!render_card(&c).contains(RESET));
    }

    #[test]
    fn clip_counts_characters_not_bytes() {
        assert_eq!(clip("añañañ", 4), "aña…");
        assert_eq!(clip("añ", 4), "añ");
        assert_eq!(clip("abc", 1), "…");
        assert_eq!(clip("abc", 0), "");
    }
}
