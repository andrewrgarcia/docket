use std::io::Write;
use std::process::{Command, Stdio};

/// Clipboard access by shelling out to whatever the system already has. This
/// avoids linking X11 or pulling a clipboard crate, and it degrades honestly:
/// over SSH or in a container there is simply no tool, and the caller prints
/// to stdout instead.
type Tool = (&'static str, &'static [&'static str]);

fn copy_tools() -> &'static [Tool] {
    #[cfg(target_os = "windows")]
    {
        &[("clip.exe", &[]), ("clip", &[])]
    }
    #[cfg(target_os = "macos")]
    {
        &[("pbcopy", &[])]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // clip.exe last: it is present under WSL when nothing native is.
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
            ("clip.exe", &[]),
        ]
    }
}

fn paste_tools() -> &'static [Tool] {
    #[cfg(target_os = "windows")]
    {
        &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])]
    }
    #[cfg(target_os = "macos")]
    {
        &[("pbpaste", &[])]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        &[
            ("wl-paste", &["--no-newline"]),
            ("xclip", &["-selection", "clipboard", "-o"]),
            ("xsel", &["--clipboard", "--output"]),
            ("powershell.exe", &["-NoProfile", "-Command", "Get-Clipboard"]),
        ]
    }
}

/// `Ok(true)` if a tool took the text, `Ok(false)` if none exists.
pub fn copy(text: &str) -> bool {
    for (bin, args) in copy_tools() {
        let Ok(mut child) = Command::new(bin).args(*args).stdin(Stdio::piped()).spawn() else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(text.as_bytes()).is_err() {
                continue;
            }
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }
    false
}

/// `None` when no tool exists or the clipboard holds nothing useful.
pub fn paste() -> Option<String> {
    for (bin, args) in paste_tools() {
        let Ok(output) = Command::new(bin).args(*args).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
    None
}
