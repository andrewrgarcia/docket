use std::io::{self, Write};

use crate::card::Card;
use crate::editor::syntax::{self, Kind};
use crate::error::{Error, Result};
use crate::theme::{self, BOLD, CYAN, DIM, GREEN};

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

/// The list, as it goes to the terminal.
pub fn table(cards: &[Card]) -> String {
    render_table(cards, terminal_width(), theme::enabled())
}

/// Colour, decided by the caller rather than read from the environment.
///
/// `theme::enabled()` asks whether stdout is a terminal, which is true inside
/// `cargo test` — the harness captures each thread's output, but the process
/// still owns the tty. Rendering would then emit escape codes that no width
/// assertion can see past. Passing the decision in keeps the renderer honest
/// and the tests deterministic.
fn ink(on: bool, text: &str, codes: &[&str]) -> String {
    if on {
        theme::paint_always(text, codes)
    } else {
        text.to_string()
    }
}

/// One row per card, clipped to `width`. Descriptions are routinely longer
/// than a terminal is wide, and a table that wraps stops being a table.
fn render_table(cards: &[Card], width: usize, color: bool) -> String {
    if cards.is_empty() {
        return "no cards yet — `dk add .` in a project, or `dk add` for an idea\n".into();
    }

    let ids: Vec<String> = cards.iter().map(|c| c.id.clone()).collect();
    let shorts: Vec<String> = cards.iter().map(|c| c.short_id(&ids)).collect();

    let hash_w = shorts.iter().map(|s| s.len()).max().unwrap_or(4).max(4);

    // How much of each card has actually been written. A card is a form you
    // fill in over time, and this is the column that shames the empty ones.
    let filled: Vec<(usize, usize)> = cards.iter().map(Card::completion).collect();
    let any_sections = filled.iter().any(|(_, total)| *total > 0);
    let done_w = if any_sections { "DONE".len().max(BAR + 4) } else { 0 };

    let status_w = cards
        .iter()
        .map(|c| c.status.as_str().len())
        .max()
        .unwrap_or(6)
        .max(6);

    // Everything but the name and the description is fixed width. The name
    // gets what it needs up to half the terminal, the description takes the
    // rest, and both are clipped — a table that wraps stops being a table.
    let fixed = 2 + hash_w + 2 + 2 + status_w + 2 + 5 + 2 + if any_sections { done_w + 2 } else { 0 };
    let longest_name = cards.iter().map(|c| c.name.chars().count()).max().unwrap_or(4).max(4);
    let name_w = longest_name
        .min(width.saturating_sub(fixed + 4).max(4))
        .min(width / 2);
    let what_w = width.saturating_sub(fixed + name_w);

    let mut out = String::new();
    let done_head = if any_sections { format!("{:<done_w$}  ", "DONE") } else { String::new() };
    out.push_str(&ink(
        color,
        &format!(
            "  {:<hash_w$}  {:<name_w$}  {:<status_w$}  {:>5}  {done_head}{}",
            "HASH",
            clip("NAME", name_w),
            "STATUS",
            "AGE",
            clip("WHAT", what_w)
        ),
        &[BOLD],
    ));
    out.push('\n');

    for ((card, short), (done, total)) in cards.iter().zip(&shorts).zip(&filled) {
        let cold = card.status.is_cold();

        let hash = ink(color, &pad(short, hash_w), &[DIM]);
        let name = ink(color, &pad(&clip(&card.name, name_w), name_w), if cold { &[DIM] } else { &[BOLD] });
        let status = ink(
            color,
            &pad(card.status.as_str(), status_w),
            &[theme::status_color(card.status.as_str())],
        );
        let age = ink(color, &format!("{:>4}d", card.age_days), &[DIM]);
        let done_cell = if any_sections {
            format!("{}  ", completion_cell(*done, *total, done_w, color))
        } else {
            String::new()
        };
        let what = clip(&card.what, what_w);
        let what = if cold { ink(color, &what, &[DIM]) } else { what };

        out.push_str(&format!("  {hash}  {name}  {status}  {age}  {done_cell}{what}"));
        out.push('\n');
    }

    out.push('\n');
    out.push_str(&ink(color, &format!("{} cards", cards.len()), &[DIM]));
    out.push('\n');
    out
}

/// A card for reading: the same text, coloured by line kind.
pub fn render_card(card: &Card) -> String {
    render_card_with(card, theme::enabled())
}

fn render_card_with(card: &Card, color: bool) -> String {
    if !color {
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
            other => out.push_str(&theme::paint_always(line, other.codes())),
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

/// Blocks in the completion bar.
const BAR: usize = 4;

/// `▰▰▱▱ 2/4` — full when every section has something in it, dim when the
/// card has no sections at all.
fn completion_cell(done: usize, total: usize, width: usize, color: bool) -> String {
    if total == 0 {
        return ink(color, &pad("—", width), &[DIM]);
    }
    let lit = (done * BAR).div_ceil(total.max(1)).min(BAR);
    let bar: String = "▰".repeat(lit) + &"▱".repeat(BAR - lit);
    let code = if done == total { GREEN } else { CYAN };
    let text = format!("{bar} {done}/{total}");
    let padding = width.saturating_sub(text.chars().count());
    format!("{}{}", ink(color, &text, &[code]), " ".repeat(padding))
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
    use crate::theme::RESET;

    /// Render with colour off — the assertions below measure widths and
    /// prefixes, which escape codes would make meaningless.
    fn plain(cards: &[Card], width: usize) -> String {
        render_table(cards, width, false)
    }

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

    #[test]
    fn empty_store_explains_the_next_step() {
        assert!(plain(&[], 80).contains("dk add"));
    }

    #[test]
    fn no_row_exceeds_the_terminal_width() {
        let long = "Turn your AI chats into a durable, local-first diary. Save messages, attach notes, organize conversations.";
        let cards = [card("cli", "active", long), card("yggdrasil-cli", "active", long)];
        for width in [40usize, 60, 80, 120] {
            for line in plain(&cards, width).lines() {
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
        assert!(plain(&cards, 40).contains('…'));
    }

    #[test]
    fn the_done_column_reports_filled_sections() {
        let bare = [card("a", "active", "x")];
        assert!(!plain(&bare, 80).contains("DONE"), "no sections, no column");

        let half = Card::parse(
            "b",
            "# b\nid: bbbb0000\nstatus: active\nwhat: y\n\n## now\nparser\n\n## next\n",
            0,
        );
        let out = plain(&[card("a", "active", "x"), half], 80);
        assert!(out.contains("DONE"), "{out}");
        assert!(out.contains("1/2"), "{out}");
        assert!(out.contains('▰') && out.contains('▱'), "{out}");
    }

    #[test]
    fn a_full_card_fills_the_bar() {
        let full = Card::parse(
            "b",
            "# b\nid: bbbb0000\nstatus: active\nwhat: y\n\n## now\na\n\n## next\nb\n",
            0,
        );
        let out = plain(&[full], 80);
        assert!(out.contains("▰▰▰▰ 2/2"), "{out}");
    }

    #[test]
    fn the_hash_column_shows_a_short_unique_prefix() {
        let cards = [card("a", "active", "x"), card("b", "idea", "y")];
        let out = plain(&cards, 80);
        assert!(out.contains("HASH"));
        for (row, c) in out.lines().skip(1).take(2).zip(cards.iter()) {
            assert!(row.trim_start().starts_with(&c.id[..4]), "{row}");
        }
    }


    #[test]
    fn a_piped_card_is_the_file_verbatim() {
        let c = Card::parse("a", "# a\nid: aaaa0000\nstatus: active\n", 0);
        assert_eq!(render_card_with(&c, false), "# a\nid: aaaa0000\nstatus: active\n");
        assert!(!render_card_with(&c, false).contains(RESET));
    }

    #[test]
    fn clip_counts_characters_not_bytes() {
        assert_eq!(clip("añañañ", 4), "aña…");
        assert_eq!(clip("añ", 4), "añ");
        assert_eq!(clip("abc", 1), "…");
        assert_eq!(clip("abc", 0), "");
    }
}
