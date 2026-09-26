//! End-to-end tests driving the real binary. No dev-dependencies: `cargo`
//! hands us the executable path, and every run gets a throwaway `DOCKET_HOME`.
//!
//! Output is piped, so colour is off and widths are measurable.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    home: PathBuf,
    scratch: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let base = env::temp_dir().join(format!("dk-e2e-{}-{id}", std::process::id()));
        let home = base.join("store");
        let scratch = base.join("work");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&scratch).unwrap();
        Sandbox { home, scratch }
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with_stdin(args, "")
    }

    fn run_with_stdin(&self, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_dk"))
            .args(args)
            .current_dir(&self.scratch)
            .env("DOCKET_HOME", &self.home)
            .env("NO_COLOR", "1")
            .env_remove("EDITOR")
            .env_remove("VISUAL")
            .env_remove("DOCKET_EDITOR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("binary should run");
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    }

    fn stdout(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(out.status.success(), "`{args:?}` failed: {}", err(&out));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn card(&self, name: &str, body: &str) {
        fs::write(self.home.join(format!("{name}.md")), body).unwrap();
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.home.join(format!("{name}.md"))).unwrap()
    }

    /// A project directory inside the sandbox.
    fn project(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = self.scratch.join(name);
        fs::create_dir_all(&dir).unwrap();
        for (file, body) in files {
            fs::write(dir.join(file), body).unwrap();
        }
        dir
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Some(base) = self.home.parent() {
            let _ = fs::remove_dir_all(base);
        }
    }
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn empty_store_points_at_the_next_step() {
    let s = Sandbox::new();
    assert!(s.stdout(&[]).contains("dk add"));
}

#[test]
fn add_captures_the_whole_readme_as_the_last_section() {
    let s = Sandbox::new();
    let readme = "# fur\n\nA local-first diary.\n\n## Install\n\ncargo install fur-cli\n";
    let dir = s.project(
        "cli",
        &[
            ("Cargo.toml", "[package]\nname = \"fur-cli\"\ndescription = \"a diary\"\n"),
            ("README.md", readme),
        ],
    );

    let printed = s.stdout(&["add", dir.to_str().unwrap()]);
    assert!(printed.contains("fur-cli"), "{printed}");
    assert!(printed.contains("README captured"), "{printed}");

    let card = s.read("fur-cli");
    assert!(card.contains("cargo install fur-cli"), "readme body missing");
    let readme_at = card.find("## readme").expect("readme section");
    for section in ["## now", "## next", "## open questions", "## notes"] {
        assert!(card.find(section).unwrap() < readme_at, "{section} after readme");
    }
}

#[test]
fn a_project_without_a_readme_gets_no_readme_section() {
    let s = Sandbox::new();
    let dir = s.project("bare", &[("Cargo.toml", "[package]\nname = \"bare\"\n")]);
    s.stdout(&["add", dir.to_str().unwrap()]);
    assert!(!s.read("bare").contains("## readme"));
}

#[test]
fn sync_refreshes_the_readme_and_leaves_notes_alone() {
    let s = Sandbox::new();
    let dir = s.project(
        "moxi",
        &[
            ("Cargo.toml", "[package]\nname = \"moxi\"\ndescription = \"a language\"\n"),
            ("README.md", "# moxi\n\nFirst version.\n"),
        ],
    );
    s.stdout(&["add", dir.to_str().unwrap()]);

    // The user writes a note, then the project's README moves on.
    let card = s.read("moxi").replace("## now\n", "## now\nparser work\n");
    fs::write(s.home.join("moxi.md"), card).unwrap();
    fs::write(dir.join("README.md"), "# moxi\n\nSecond version.\n").unwrap();

    let printed = s.stdout(&["sync"]);
    assert!(printed.contains("updated"), "{printed}");

    let body = s.read("moxi");
    assert!(body.contains("Second version."));
    assert!(!body.contains("First version."));
    assert!(body.contains("## now\nparser work"), "notes lost: {body}");
    assert_eq!(body.matches("## readme").count(), 1);
}

#[test]
fn sync_skips_ideas_and_vanished_paths() {
    let s = Sandbox::new();
    s.card("an-idea", "# an-idea\nstatus: idea\nwhat: someday\n");
    s.card("gone", "# gone\nstatus: active\npath: /nowhere/at/all\n");
    let printed = s.stdout(&["sync"]);
    assert!(printed.contains("no path"), "{printed}");
    assert!(printed.contains("path is gone"), "{printed}");
    assert!(printed.contains("0 updated"), "{printed}");
}

#[test]
fn show_prints_the_card_verbatim_when_piped() {
    let s = Sandbox::new();
    // With an id already present, nothing is backfilled and the file that
    // comes back out is byte-for-byte the file that went in.
    let body = "# moxi\nid: a43b21c0\nstatus: active\nwhat: a language\n\n## now\nparser\n";
    s.card("moxi", body);
    assert_eq!(s.stdout(&["show", "moxi"]), body);
    // A bare name is the same thing.
    assert_eq!(s.stdout(&["moxi"]), body);
}

#[test]
fn show_includes_an_id_the_store_had_to_backfill() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    let shown = s.stdout(&["show", "moxi"]);
    assert!(shown.contains("id: "), "{shown}");
    assert_eq!(shown, s.read("moxi"), "show matches the file on disk");
}

#[test]
fn out_writes_every_card_to_one_file() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\nwhat: a language\n\n## now\nparser\n");
    s.card("kol", "# kol\nstatus: idea\nwhat: a game\n\n## now\nnothing\n");

    let printed = s.stdout(&["out"]);
    assert!(printed.contains("DOCKET.md"), "{printed}");
    assert!(printed.contains("tokens"), "{printed}");

    let written = fs::read_to_string(s.scratch.join("DOCKET.md")).unwrap();
    assert!(written.contains("* moxi [active"));
    assert!(written.contains("* kol [idea"));
    assert!(written.contains("parser"));
    assert!(written.contains("nothing"));
}

#[test]
fn out_honours_a_chosen_filename() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    s.stdout(&["out", "--out", "brief.md"]);
    assert!(s.scratch.join("brief.md").exists());
}

#[test]
fn pick_points_at_the_non_interactive_route() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    let out = s.run(&["pick"]);
    assert!(!out.status.success());
    assert!(err(&out).contains("dk out"), "{}", err(&out));
}

#[test]
fn add_names_the_card_after_the_manifest_not_the_directory() {
    let s = Sandbox::new();
    let dir = s.project(
        "cli",
        &[("Cargo.toml", "[package]\nname = \"fur-cli\"\n\n[dependencies]\nname = \"wrong\"\n")],
    );
    s.stdout(&["add", dir.to_str().unwrap()]);
    assert!(s.home.join("fur-cli.md").exists());
    assert!(!s.home.join("cli.md").exists());
}

#[test]
fn adding_the_same_directory_twice_is_refused() {
    let s = Sandbox::new();
    let dir = s.project("once", &[("Cargo.toml", "[package]\nname = \"once\"\n")]);
    s.stdout(&["add", dir.to_str().unwrap()]);
    let second = s.run(&["add", dir.to_str().unwrap()]);
    assert!(!second.status.success());
    assert!(err(&second).contains("already"));
}

#[test]
fn the_list_shows_how_much_of_each_card_is_written() {
    let s = Sandbox::new();
    s.card(
        "moxi",
        "# moxi\nstatus: active\nwhat: x\n\n## now\nparser\n\n## next\n\n## notes\n",
    );
    s.card("kol", "# kol\nstatus: idea\nwhat: y\n");
    let listing = s.stdout(&[]);
    assert!(listing.contains("DONE"), "{listing}");
    assert!(listing.contains("1/3"), "one of three sections written: {listing}");
    assert!(listing.contains("—"), "a card with no sections shows a dash: {listing}");
    assert!(!listing.contains("PROGRESS"), "the old column is gone");
}

#[test]
fn pick_and_its_aliases_all_need_a_terminal() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n\n## now\nx\n");
    for verb in ["pick", "p", "tree", "t"] {
        let out = s.run(&[verb]);
        assert!(!out.status.success(), "{verb} should refuse a pipe");
        assert!(err(&out).contains("needs a terminal"), "{verb}: {}", err(&out));
    }
}

#[test]
fn docket_rewrites_do_not_reset_a_cards_age() {
    use std::time::{Duration, SystemTime};
    let s = Sandbox::new();
    s.card("old", "# old\nstatus: active\nwhat: x\n");

    // Pretend the card was last touched ten days ago.
    let path = s.home.join("old.md");
    let ten_days = SystemTime::now() - Duration::from_secs(10 * 86_400);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(ten_days)
        .unwrap();

    // The first listing backfills an id, which rewrites the file …
    let listing = s.stdout(&[]);
    assert!(s.read("old").contains("id: "));
    // … and the age must survive it.
    assert!(listing.contains("  10d"), "{listing}");
}

#[test]
fn every_card_gets_a_hash_and_answers_to_it() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\nwhat: a language\n");

    // The first listing backfills the id and writes it into the file.
    let listing = s.stdout(&[]);
    assert!(listing.contains("HASH"), "{listing}");

    let body = s.read("moxi");
    let id_line = body.lines().find(|l| l.starts_with("id: ")).expect("id line");
    let id = id_line.trim_start_matches("id: ").to_string();
    assert_eq!(id.len(), 8, "{body}");

    // Addressable by the whole id and by its first four characters.
    assert!(s.stdout(&[&id]).contains("# moxi"));
    assert!(s.stdout(&[&id[..4]]).contains("# moxi"));

    // And stable: a second run does not renumber it.
    s.stdout(&[]);
    assert!(s.read("moxi").contains(&format!("id: {id}")));
}

#[test]
fn open_reports_when_no_desktop_opener_exists() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");

    // A opener that cannot be spawned stands in for a headless machine.
    let out = Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(["open", "moxi"])
        .current_dir(&s.scratch)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .env("DOCKET_OPENER", "definitely-not-an-opener-42")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(err(&out).contains("cannot run"), "{}", err(&out));
}

#[test]
fn open_uses_the_configured_opener() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    let opener = if cfg!(windows) { "cmd /c exit 0" } else { "true" };
    let out = Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(["open", "moxi"])
        .current_dir(&s.scratch)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .env("DOCKET_OPENER", opener)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", err(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("opened"));
}

#[test]
fn prefixes_resolve_and_ambiguity_is_refused() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    assert!(s.stdout(&["mo"]).contains("# moxi"));

    s.card("morse", "# morse\nstatus: idea\n");
    let out = s.run(&["mo"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(err(&out).contains("matches"));
}

#[test]
fn rename_moves_the_card_and_its_heading() {
    let s = Sandbox::new();
    s.card("cli", "# cli\nstatus: active\nwhat: a diary\n\n## now\nparser\n");
    s.stdout(&["rename", "cli", "fur-cli"]);
    assert!(!s.home.join("cli.md").exists());
    let body = s.read("fur-cli");
    assert!(body.starts_with("# fur-cli\n"), "{body}");
    assert!(body.contains("## now\nparser"));
}

#[test]
fn rename_onto_an_existing_name_is_refused() {
    let s = Sandbox::new();
    s.card("cli", "# cli\nstatus: active\n");
    s.card("fur-cli", "# fur-cli\nstatus: active\n");
    assert_eq!(s.run(&["rename", "cli", "fur-cli"]).status.code(), Some(4));
    assert!(s.home.join("cli.md").exists());
}

#[test]
fn rm_requires_the_name_typed_back() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");

    let refused = s.run_with_stdin(&["rm", "moxi"], "no\n");
    assert!(refused.status.success());
    assert!(s.home.join("moxi.md").exists());

    let done = s.run_with_stdin(&["rm", "moxi"], "moxi\n");
    assert!(done.status.success());
    assert!(!s.home.join("moxi.md").exists());
}

#[test]
fn long_descriptions_do_not_wrap_the_table() {
    let s = Sandbox::new();
    let long = "Turn your AI chats into a durable, local-first diary. \
                Save messages, attach notes, organize conversations.";
    s.card("cli", &format!("# cli\nstatus: active\nwhat: {long}\n"));
    for line in s.stdout(&[]).lines() {
        assert!(line.chars().count() <= 80, "{} chars: {line:?}", line.chars().count());
    }
}

#[test]
fn the_built_in_editor_refuses_to_start_without_a_terminal() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    let out = s.run(&["edit", "moxi"]);
    assert!(!out.status.success());
    assert!(err(&out).contains("needs a terminal"), "{}", err(&out));
}

#[test]
fn a_configured_editor_is_used_instead() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    let editor = if cfg!(windows) { "cmd /c exit 0" } else { "true" };
    let out = Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(["edit", "moxi"])
        .current_dir(&s.scratch)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .env("EDITOR", editor)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", err(&out));
}

#[test]
fn unknown_flags_are_usage_errors() {
    let s = Sandbox::new();
    assert_eq!(s.run(&["--nope"]).status.code(), Some(2));
}

#[test]
fn a_missing_card_exits_three() {
    let s = Sandbox::new();
    assert_eq!(s.run(&["ghost"]).status.code(), Some(3));
}
