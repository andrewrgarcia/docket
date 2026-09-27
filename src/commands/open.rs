use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};
use crate::store::Store;
use crate::theme::{self, BOLD, DIM};

/// Hand the card's file to whatever the desktop opens `.md` with — gedit,
/// TextEdit, Typora, VS Code, whatever is registered. This is the escape hatch
/// from the terminal: `dk edit` is for a quick line, `dk open` is for when
/// you want a real window and a mouse.
///
/// `$DOCKET_OPENER` overrides the platform default, for a machine where the
/// registered handler is wrong or missing.
pub fn card(store: &Store, query: &str) -> Result<()> {
    let card = store.get(query)?;
    let path = store.card_path(&card.name);
    launch(&path)?;
    println!(
        "{} {}",
        theme::paint("opened", &[BOLD]),
        theme::paint(&path.display().to_string(), &[DIM])
    );
    Ok(())
}

/// The store itself, for when you want the folder rather than one card.
pub fn store_dir(store: &Store) -> Result<()> {
    launch(store.root())?;
    println!(
        "{} {}",
        theme::paint("opened", &[BOLD]),
        theme::paint(&store.root().display().to_string(), &[DIM])
    );
    Ok(())
}

fn launch(path: &Path) -> Result<()> {
    if let Some(custom) = std::env::var_os("DOCKET_OPENER") {
        let custom = custom.to_string_lossy().into_owned();
        let mut words = custom.split_whitespace();
        let program = words
            .next()
            .ok_or_else(|| Error::other("$DOCKET_OPENER is empty"))?;
        return spawn(program, &words.collect::<Vec<_>>(), path)
            .map_err(|e| Error::other(format!("cannot run `{program}`: {e}")));
    }

    for (program, args) in openers() {
        if spawn(program, args, path).is_ok() {
            return Ok(());
        }
    }
    Err(Error::other(
        "no desktop opener found — set $DOCKET_OPENER to a program, or use `dk edit`",
    ))
}

/// Detached, with stdio silenced: a GUI editor keeps running after docket
/// exits, and gio/gvfs warnings on stderr are not docket's to print.
fn spawn(program: &str, args: &[&str], path: &Path) -> std::io::Result<()> {
    Command::new(program)
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn openers() -> &'static [(&'static str, &'static [&'static str])] {
    #[cfg(target_os = "windows")]
    {
        // `start` is a shell builtin, so it goes through cmd. The empty string
        // is the window title, which `start` otherwise steals from the path.
        &[("cmd", &["/C", "start", ""]), ("explorer", &[])]
    }
    #[cfg(target_os = "macos")]
    {
        &[("open", &[])]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // xdg-open respects the desktop's own association; the rest are for a
        // machine without it. WSL falls through to the Windows handler.
        &[
            ("xdg-open", &[]),
            ("gio", &["open"]),
            ("gnome-open", &[]),
            ("kde-open", &[]),
            ("wslview", &[]),
        ]
    }
}
