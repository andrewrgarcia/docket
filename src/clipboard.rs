//! Copying to the system clipboard.
//!
//! Two routes, in order:
//!
//! 1. A native tool (`wl-copy`, `xclip`, `pbcopy`, `clip.exe`). Reliable and
//!    unlimited in size, but needs the binary and a local display.
//! 2. OSC 52 — an escape sequence asking the *terminal* to set the clipboard.
//!    Needs no binary and works over SSH, but many terminals cap the payload
//!    and some ignore it silently, with no way to detect either.
//!
//! Native first, because a brief routinely exceeds what OSC 52 will carry,
//! and OSC 52 is reported as "attempted" rather than "copied" for the same
//! reason.

use std::io::Write;
use std::process::{Command, Stdio};

/// How the text reached the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Tool(&'static str),
    /// Sent, but the terminal may have dropped it.
    Osc52,
}

/// How a tool wants its stdin encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Utf8,
    /// Windows `clip` decodes stdin with the system ANSI codepage, which
    /// mangles every box-drawing character. UTF-16LE with a BOM is the one
    /// form it reads unambiguously.
    Utf16Le,
}

/// Wayland before X11, so a Wayland session is not routed through XWayland.
/// `clip.exe` covers WSL as well as native Windows.
const TOOLS: &[(&str, &[&str], Encoding)] = &[
    ("wl-copy", &[], Encoding::Utf8),
    ("xclip", &["-selection", "clipboard"], Encoding::Utf8),
    ("xsel", &["--clipboard", "--input"], Encoding::Utf8),
    ("pbcopy", &[], Encoding::Utf8),
    ("clip.exe", &[], Encoding::Utf16Le),
    ("clip", &[], Encoding::Utf16Le),
];

pub fn copy(text: &str) -> Option<Route> {
    for (bin, args, encoding) in TOOLS {
        if feed(bin, args, *encoding, text) {
            return Some(Route::Tool(bin));
        }
    }
    osc52(text).then_some(Route::Osc52)
}

/// What to suggest when nothing worked.
pub fn install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "pbcopy should be built in — check your PATH"
    } else if cfg!(target_os = "windows") {
        "clip.exe should be built in — check your PATH"
    } else if std::env::var("WAYLAND_DISPLAY").is_ok() {
        "install wl-clipboard (apt install wl-clipboard)"
    } else {
        "install xclip (apt install xclip) or wl-clipboard"
    }
}

fn feed(bin: &str, args: &[&str], encoding: Encoding, text: &str) -> bool {
    let Ok(mut child) = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let payload = match encoding {
            Encoding::Utf8 => text.as_bytes().to_vec(),
            Encoding::Utf16Le => utf16le_with_bom(text),
        };
        if stdin.write_all(&payload).is_err() {
            return false;
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut out = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// `ESC ] 52 ; c ; <base64> BEL`, written to the terminal.
fn osc52(text: &str) -> bool {
    let mut out = std::io::stdout();
    write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes())).is_ok() && out.flush().is_ok()
}

/// Standard base64. Six lines, rather than a dependency for one call site.
fn base64(bytes: &[u8]) -> String {
    const SET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(SET[(n >> (18 - 6 * i) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_examples() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn utf16_payload_starts_with_the_bom() {
        let bytes = utf16le_with_bom("hi");
        assert_eq!(&bytes[..2], &[0xff, 0xfe]);
        assert_eq!(&bytes[2..], &[b'h', 0, b'i', 0]);
    }
}
