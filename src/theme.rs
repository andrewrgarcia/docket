//! Colour, in one place.
//!
//! Two rules: never colour when the output is not a terminal, and never colour
//! when `NO_COLOR` is set. Everything else in the program asks this module
//! rather than writing an escape code of its own.

use std::env;
use std::io::{self, IsTerminal};
use std::sync::OnceLock;

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const REVERSE: &str = "\x1b[7m";

pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const MAGENTA: &str = "\x1b[35m";
pub const CYAN: &str = "\x1b[36m";
pub const GREY: &str = "\x1b[90m";

/// Decided once. A pipe halfway through a run cannot change its mind.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        env::var_os("NO_COLOR").is_none()
            && env::var("TERM").map(|t| t != "dumb").unwrap_or(true)
            && io::stdout().is_terminal()
    })
}

/// Wrap `text` in `codes`, or return it untouched when colour is off.
pub fn paint(text: &str, codes: &[&str]) -> String {
    if !enabled() || codes.is_empty() {
        return text.to_string();
    }
    format!("{}{text}{RESET}", codes.concat())
}

/// The editor draws into raw mode, where `enabled()`'s stdout check still
/// holds but the caller wants codes regardless of piping decisions made for
/// the list. Same rules, stated separately so the intent is visible.
pub fn paint_always(text: &str, codes: &[&str]) -> String {
    if codes.is_empty() {
        return text.to_string();
    }
    format!("{}{text}{RESET}", codes.concat())
}

/// The colour a status prints in: live work stands out, finished work recedes.
pub fn status_color(status: &str) -> &'static str {
    match status {
        "active" => GREEN,
        "idea" => CYAN,
        "paused" => YELLOW,
        "done" => GREY,
        "dead" => GREY,
        _ => GREY,
    }
}
