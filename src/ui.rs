use std::io::{self, Write};

use crate::card::Card;
use crate::editor::syntax::{self, Kind};
use crate::error::{Error, Result};
use crate::theme::{self, BOLD, CYAN, DIM};

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

    let status_w = cards
        .iter()
        .map(|c| c.status.as_str().len())
        .max()
        .unwrap_or(6)
        .max(6);

    // Everything but the name and the description is fixed width. The name
    // gets what it needs up to half the terminal, the description takes the
    // rest, and both are clipped — a table that wraps stops being a table.
    let fixed = 2 + hash_w + 2 + 2 + status_w + 2 + 5 + 2;
    let longest_name = cards.iter().map(|c| c.name.chars().count()).max().unwrap_or(4).max(4);
    let name_w = longest_name
        .min(width.saturating_sub(fixed + 4).max(4))
        .min(width / 2);
    let what_w = width.saturating_sub(fixed + name_w);

    let mut out = String::new();
    out.push_str(&ink(
        color,
        &format!(
            "  {:<hash_w$}  {:<name_w$}  {:<status_w$}  {:>5}  {}",
            "HASH",
            clip("NAME", name_w),
            "STATUS",
            "AGE",
            clip("WHAT", what_w)
        ),
        &[BOLD],
    ));
    out.push('\n');

    for (card, short) in cards.iter().zip(&shorts) {
        let cold = card.status.is_cold();

        let hash = ink(color, &pad(short, hash_w), &[DIM]);
        let name = ink(color, &pad(&clip(&card.name, name_w), name_w), if cold { &[DIM] } else { &[BOLD] });
        let status = ink(
            color,
            &pad(card.status.as_str(), status_w),
            &[theme::status_color(card.status.as_str())],
        );
        let age = ink(color, &format!("{:>4}d", card.age_days), &[DIM]);
        let what = clip(&card.what, what_w);
        let what = if cold { ink(color, &what, &[DIM]) } else { what };

        out.push_str(&format!("  {hash}  {name}  {status}  {age}  {what}"));
        out.push('\n');
    }

    out.push('\n');
    out.push_str(&ink(color, &summary(cards), &[DIM]));
    out.push('\n');
    out
}

/// Days after which a project counts as forgotten. A quarter is long enough
/// that a slow month does not accuse you, and short enough that a year does.
const STALE_DAYS: u64 = 90;

/// The line under the table: what the table cannot show at a glance.
fn summary(cards: &[Card]) -> String {
    let stale = cards.iter().filter(|c| c.age_days >= STALE_DAYS && !c.status.is_cold()).count();
    let tokens: usize = cards.iter().map(|c| c.body.chars().count() / 4).sum();

    let mut parts = vec![format!("{} cards", cards.len())];
    if stale > 0 {
        parts.push(format!("{stale} untouched {STALE_DAYS}d+"));
    }
    parts.push(format!("~{} tokens", human(tokens)));
    parts.join(" · ")
}

/// Thousands as `48k`, so the total fits beside the rest.
fn human(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// A card as it is read: the body, then the linked README under `## readme`.
/// What `dk show` prints when piped and what its fold view displays.
pub fn card_text(card: &Card) -> String {
    let mut text = card.body.trim_end().to_string();
    text.push('\n');

    // The card links its README; showing the card means showing what the card
    // stands for, so the linked file is read and appended here.
    match (card.readme.is_empty(), card.readme_text()) {
        (false, Some(readme)) => {
            text.push_str(&format!("\n{}\n\n{}\n", crate::card::README_HEADING, readme.trim_end()));
        }
        (false, None) => {
            text.push_str(&format!(
                "\n{}\n\n[no README at {}]\n",
                crate::card::README_HEADING,
                card.readme
            ));
        }
        (true, _) => {}
    }
    text
}

/// A card for reading: the same text, coloured by line kind.
pub fn render_card(card: &Card) -> String {
    render_card_with(card, theme::enabled())
}

fn render_card_with(card: &Card, color: bool) -> String {
    let text = card_text(card);
    if !color {
        return text;
    }
    let lines: Vec<String> = text.lines().map(String::from).collect();
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
    fn there_is_no_done_column() {
        let full = Card::parse(
            "b",
            "# b\nid: bbbb0000\nstatus: active\nwhat: y\n\n## now\na\n\n## next\n",
            0,
        );
        let out = plain(&[full], 80);
        assert!(!out.contains("DONE"), "{out}");
        assert!(!out.contains('▰') && !out.contains("1/2"), "{out}");
        assert!(!out.contains("sections written"), "{out}");
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
    fn the_summary_reports_what_the_table_cannot_show() {
        let fresh = Card::parse("a", "# a\nid: aaaa0000\nstatus: active\n\n## now\nx\n", 3);
        let mut old = Card::parse("b", "# b\nid: bbbb0000\nstatus: active\n\n## now\n", 0);
        old.age_days = 200;

        let cards = [fresh, old];
        let line = summary(&cards);

        assert!(line.contains("2 cards"), "{line}");
        assert!(line.contains("1 untouched 90d+"), "{line}");
        assert!(line.contains("tokens"), "{line}");
    }

    #[test]
    fn a_fresh_store_does_not_mention_staleness() {
        let cards = [card("a", "active", "x")];
        let line = summary(&cards);
        assert!(!line.contains("untouched"), "{line}");
    }

    #[test]
    fn a_finished_project_is_not_counted_as_forgotten() {
        let mut done = Card::parse("b", "# b\nid: bbbb0000\nstatus: done\n", 0);
        done.age_days = 400;
        assert!(!summary(&[done]).contains("untouched"));
    }

    #[test]
    fn clip_counts_characters_not_bytes() {
        assert_eq!(clip("añañañ", 4), "aña…");
        assert_eq!(clip("añ", 4), "añ");
        assert_eq!(clip("abc", 1), "…");
        assert_eq!(clip("abc", 0), "");
    }
}
