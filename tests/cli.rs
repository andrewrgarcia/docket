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
fn add_links_the_readme_and_output_reads_it_live() {
    let s = Sandbox::new();
    let dir = s.project(
        "moxi",
        &[
            ("Cargo.toml", "[package]\nname = \"moxi\"\ndescription = \"a language\"\n"),
            ("README.md", "# moxi\n\nFirst version.\n"),
        ],
    );
    let printed = s.stdout(&["add", dir.to_str().unwrap()]);
    assert!(printed.contains("readme →"), "{printed}");

    let card = s.read("moxi");
    assert!(card.contains("readme: "), "{card}");
    assert!(!card.contains("First version."), "the card links, it does not copy");

    // Change the README; the next brief carries the new text with no sync.
    fs::write(dir.join("README.md"), "# moxi\n\nSecond version.\n").unwrap();
    s.stdout(&["out"]);
    let written = fs::read_to_string(s.scratch.join("DOCKET.md")).unwrap();
    assert!(written.contains("Second version."), "{written}");
    assert!(!written.contains("First version."));
}

#[test]
fn show_prints_the_linked_readme_with_the_card() {
    let s = Sandbox::new();
    let dir = s.project("thing", &[("README.md", "# thing\n\nThe readme body.\n")]);
    s.stdout(&["add", dir.to_str().unwrap()]);

    let shown = s.stdout(&["show", "thing"]);
    assert!(shown.contains("## now"), "the card's own sections");
    assert!(shown.contains("The readme body."), "and the linked file");
}

#[test]
fn a_missing_readme_is_reported_rather_than_dropped() {
    let s = Sandbox::new();
    s.card(
        "gone",
        "# gone\nstatus: active\npath: /nowhere\nreadme: /nowhere/README.md\n\n## now\nx\n",
    );
    let shown = s.stdout(&["show", "gone"]);
    assert!(shown.contains("no README at /nowhere/README.md"), "{shown}");
}

#[test]
fn an_embedded_readme_is_migrated_to_a_link() {
    let s = Sandbox::new();
    let dir = s.project("old", &[("README.md", "# old\n\nThe real readme.\n")]);
    // A card in the pre-link format: a copy of the README inside it.
    s.card(
        "old",
        &format!(
            "# old\nstatus: active\npath: {}\n\n## now\nmy note\n\n## readme\n\n# old\n\nA stale copy.\n",
            dir.display()
        ),
    );

    s.stdout(&[]); // any command triggers the one-time migration

    let card = s.read("old");
    assert!(card.contains("readme: "), "{card}");
    assert!(!card.contains("A stale copy."), "the copy is dropped");
    assert!(card.contains("## now\nmy note"), "the notes survive");
    assert!(s.stdout(&["show", "old"]).contains("The real readme."));
}


#[test]
fn a_project_without_a_readme_links_nothing() {
    let s = Sandbox::new();
    let dir = s.project("bare", &[("Cargo.toml", "[package]\nname = \"bare\"\n")]);
    s.stdout(&["add", dir.to_str().unwrap()]);
    let card = s.read("bare");
    assert!(!card.contains("readme:"), "{card}");
    assert!(!card.contains("## readme"));
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
fn code_reports_when_no_editor_is_installed() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");

    let out = Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(["code", "moxi"])
        .current_dir(&s.scratch)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .env("DOCKET_CODE", "definitely-not-an-editor-42")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(err(&out).contains("cannot run"), "{}", err(&out));
}

#[test]
fn code_uses_the_configured_editor_and_names_the_file() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    let editor = if cfg!(windows) { "cmd /c exit 0" } else { "true" };

    let out = Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(["code", "moxi"])
        .current_dir(&s.scratch)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .env("DOCKET_CODE", editor)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", err(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("moxi.md"));
}

#[test]
fn code_with_no_card_opens_the_store_folder() {
    let s = Sandbox::new();
    s.card("moxi", "# moxi\nstatus: active\n");
    let editor = if cfg!(windows) { "cmd /c exit 0" } else { "true" };

    let out = Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(["code"])
        .current_dir(&s.scratch)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .env("DOCKET_CODE", editor)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", err(&out));
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(printed.contains(s.home.to_str().unwrap()), "{printed}");
    assert!(!printed.contains(".md"), "the folder, not a card: {printed}");
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

#[test]
fn where_prints_the_store_path() {
    let s = Sandbox::new();
    let printed = s.stdout(&["where"]);
    assert_eq!(printed.trim(), s.home.to_string_lossy());
}
