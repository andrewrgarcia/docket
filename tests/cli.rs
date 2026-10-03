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
        self.spawn(args, stdin, true, &[])
    }

    /// Like `run`, but without `DOCKET_HOME`, so the registry decides which
    /// book is used. `extra` adds environment variables for this run only.
    fn run_books(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        self.spawn(args, "", false, extra)
    }

    fn spawn(&self, args: &[&str], stdin: &str, with_home: bool, extra: &[(&str, &str)]) -> Output {
        use std::io::Write;
        let base = self.home.parent().unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_dk"));
        command
            .args(args)
            .current_dir(&self.scratch)
            // Nothing a test runs may read or write the developer's own books.
            .env("DOCKET_CONFIG", base.join("config").join("books.toml"))
            .env("HOME", base.join("hm"))
            .env("XDG_DATA_HOME", base.join("hm").join("data"))
            .env("XDG_CONFIG_HOME", base.join("hm").join("config"))
            .env_remove("DOCKET_BOOK")
            .env("NO_COLOR", "1")
            .env_remove("EDITOR")
            .env_remove("VISUAL")
            .env_remove("DOCKET_EDITOR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if with_home {
            command.env("DOCKET_HOME", &self.home);
        } else {
            command.env_remove("DOCKET_HOME");
        }
        for (key, value) in extra {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("binary should run");
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

// ---------------------------------------------------------------------------
// dk resume — docs/resume-contract.md, D1–D5
// ---------------------------------------------------------------------------

const CARD_ID: &str = "a43b21c0";

/// A card with a known id, so a session conversation can be tagged for it.
fn resume_card(s: &Sandbox, name: &str, extra_header: &str, project: Option<&PathBuf>) {
    let path = project.map(|p| format!("path: {}\n", p.display())).unwrap_or_default();
    s.card(
        name,
        &format!(
            "# {name}\nid: {CARD_ID}\nstatus: active\nwhat: a language\n{path}{extra_header}\n## now\nparser\n\n## next\n[ ] M2\n"
        ),
    );
}

/// One session entry in the contract's shape. `tag` shows up in the title so a
/// test can tell entries apart; `next` is what the earlier-index quotes.
fn entry(tag: &str, next: &str) -> String {
    format!(
        "<!-- dk:session v1 -->\n# moxi · {tag}\n\n## done\n- x\n\n## decided\nnone\n\n## rejected\nnone\n\n## state\nM1\n\n## blockers\nnone\n\n## next\n{next}\n\n## files\nnone\n"
    )
}

/// A fur conversation under the store's `sessions/`, tagged for a card, with
/// one marker per `(file, body)`, oldest first.
fn sessions(s: &Sandbox, folder: &str, tag: &str, entries: &[(&str, String)]) -> PathBuf {
    let dir = s.home.join("sessions").join("chats").join(folder);
    fs::create_dir_all(&dir).unwrap();
    let mut spine = format!(
        "---\nfur_schema: 1\nconversation_id: 3f2a91c4-0b7e-4c1d-9a55-2e6f0c8d1b37\ntitle: sessions\ncreated_at: 2026-09-30T22:14:00Z\ntags:\n  - {tag}\n  - session\n---\n"
    );
    for (i, (file, body)) in entries.iter().enumerate() {
        fs::write(dir.join(file), body).unwrap();
        spine.push_str(&format!(
            "\n<!-- fur:msg id=m{i} avatar=claude ts=2026-09-30T22:14:0{i}Z link={file} -->\n"
        ));
    }
    fs::write(dir.join("convo.md"), spine).unwrap();
    dir
}

fn resumed(s: &Sandbox, card: &str) -> String {
    s.stdout(&["resume", card]);
    fs::read_to_string(s.scratch.join("RESUME.md")).unwrap()
}

/// Run `dk` with `PATH` replaced, to control which `ygg` it can find.
fn run_with_path(s: &Sandbox, args: &[&str], path: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(args)
        .current_dir(&s.scratch)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[cfg(unix)]
fn fake_ygg(s: &Sandbox, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = s.scratch.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("ygg");
    fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// Records its own arguments and working directory into the file `--out` names.
#[cfg(unix)]
const RECORDING_YGG: &str = r#"
all=""
while [ $# -gt 0 ]; do
  if [ "$1" = "--out" ]; then out="$2"; fi
  all="$all $1"
  shift
done
printf 'FAKE CODEX\nargs:%s\ncwd:%s\n' "$all" "$(pwd)" > "$out"
"#;

#[test]
fn resume_orders_card_then_sessions_then_code() {
    let s = Sandbox::new();
    let project = s.project("moxi-proj", &[("README.md", "# moxi\n\nREADME TEXT\n")]);
    let readme = format!("readme: {}\n", project.join("README.md").display());
    resume_card(&s, "moxi", &readme, Some(&project));
    sessions(
        &s,
        "moxi-sessions-3f2a91c4",
        &format!("dk-{CARD_ID}"),
        &[
            ("SES-20260901-110233.md", entry("title-1", "next-1")),
            ("SES-20260915-090000.md", entry("title-2", "next-2")),
            ("SES-20260920-090000.md", entry("title-3", "next-3")),
            ("SES-20260925-090000.md", entry("title-4", "next-4")),
            ("SES-20260930-221400.md", entry("title-5", "next-5")),
        ],
    );

    let text = resumed(&s, "moxi");
    assert!(text.starts_with("<!-- dk:resume v1 -->\n# RESUME · moxi\n"), "{text}");

    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("missing {needle:?}:\n{text}"));
    assert!(at("## card") < at("## sessions"), "{text}");
    assert!(at("## sessions") < at("## code"), "{text}");

    // The card is its own sections; the README is the project's text, not it.
    assert!(text.contains("## now\nparser"), "{text}");
    assert!(!text.contains("README TEXT"), "{text}");

    // Newest three, whole, newest first.
    assert!(at("title-5") < at("title-4"), "{text}");
    assert!(at("title-4") < at("title-3"), "{text}");
    assert!(!text.contains("title-2"), "older entries are index lines only: {text}");
    assert!(!text.contains("title-1"), "{text}");

    // The older two survive as one line each: file, first line of `next`, date.
    assert!(at("### earlier") > at("title-3"), "{text}");
    assert!(text.contains("- SES-20260915-090000 · next-2 · 2026-09-15"), "{text}");
    assert!(text.contains("- SES-20260901-110233 · next-1 · 2026-09-01"), "{text}");

    // No manifest in the project, and the output says so.
    assert!(text.contains("[no WHITE.md at "), "{text}");
}

#[test]
fn resume_prints_the_path_and_the_cost_of_each_section() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);

    let out = s.run(&["resume", "moxi"]);
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "RESUME.md");

    let costs = err(&out);
    for label in ["card", "sessions", "code", "total"] {
        assert!(costs.contains(label), "{costs}");
    }
    assert!(costs.contains("tok"), "{costs}");
}

#[test]
fn resume_with_no_sessions_says_so_and_an_idea_has_no_code() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    let text = resumed(&s, "moxi");
    assert!(text.contains("[no sessions yet]"), "{text}");
    assert!(text.contains("[no project path"), "{text}");
}

#[test]
fn resume_ignores_conversations_tagged_for_other_cards() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    sessions(
        &s,
        "other-sessions-ffffffff",
        "dk-ffffffff",
        &[("SES-20260930-221400.md", entry("not-mine", "x"))],
    );
    let text = resumed(&s, "moxi");
    assert!(text.contains("[no sessions yet]"), "{text}");
    assert!(!text.contains("not-mine"), "{text}");
}

#[test]
fn resume_refuses_two_conversations_for_one_card() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    let tag = format!("dk-{CARD_ID}");
    sessions(&s, "first-aaaaaaaa", &tag, &[("SES-20260930-221400.md", entry("a", "x"))]);
    sessions(&s, "second-bbbbbbbb", &tag, &[("SES-20260930-221400.md", entry("b", "x"))]);

    let out = s.run(&["resume", "moxi"]);
    assert_eq!(out.status.code(), Some(1), "{}", err(&out));
    assert!(err(&out).contains("first-aaaaaaaa"), "{}", err(&out));
    assert!(err(&out).contains("second-bbbbbbbb"), "{}", err(&out));
    assert!(!s.scratch.join("RESUME.md").exists(), "a refusal writes nothing");
}

#[test]
fn resume_skips_messages_that_are_not_session_entries() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    let dir = sessions(
        &s,
        "moxi-sessions-3f2a91c4",
        &format!("dk-{CARD_ID}"),
        &[
            ("SES-20260901-110233.md", entry("real-1", "n1")),
            ("NOTE.md", "NOT A SESSION\n".to_string()),
            ("SES-20260930-221400.md", entry("real-2", "n2")),
        ],
    );
    // An inline message with no link at all.
    let spine = fs::read_to_string(dir.join("convo.md")).unwrap();
    fs::write(
        dir.join("convo.md"),
        format!("{spine}\n<!-- fur:msg id=x avatar=andrew ts=2026-09-30T23:00:00Z -->\n\nINLINE CHATTER\n"),
    )
    .unwrap();

    let text = resumed(&s, "moxi");
    assert!(text.contains("real-1") && text.contains("real-2"), "{text}");
    assert!(!text.contains("NOT A SESSION"), "{text}");
    assert!(!text.contains("INLINE CHATTER"), "{text}");
    assert!(!text.contains("### earlier"), "two entries leave nothing earlier: {text}");
}

#[test]
fn resume_reports_a_missing_session_file_and_never_reads_outside_the_folder() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    fs::write(s.scratch.join("secret.md"), "<!-- dk:session v1 -->\nLEAKED\n").unwrap();
    let dir = sessions(
        &s,
        "moxi-sessions-3f2a91c4",
        &format!("dk-{CARD_ID}"),
        &[("SES-20260901-110233.md", entry("real", "n"))],
    );
    let spine = fs::read_to_string(dir.join("convo.md")).unwrap();
    fs::write(
        dir.join("convo.md"),
        format!(
            "{spine}\n<!-- fur:msg id=e avatar=claude ts=2026-09-30T23:00:00Z link=../../../../work/secret.md -->\n\
             \n<!-- fur:msg id=f avatar=claude ts=2026-09-30T23:01:00Z link=SES-GONE.md -->\n"
        ),
    )
    .unwrap();

    let text = resumed(&s, "moxi");
    assert!(text.contains("real"), "{text}");
    assert!(!text.contains("LEAKED"), "{text}");
    assert!(text.contains("[skipped ../../../../work/secret.md"), "{text}");
    assert!(text.contains("[missing SES-GONE.md"), "{text}");
}

#[test]
fn resume_reports_a_missing_ygg_by_name() {
    let s = Sandbox::new();
    let project = s.project("moxi-proj", &[("WHITE.md", "src/main.rs\n")]);
    resume_card(&s, "moxi", "", Some(&project));

    // A PATH with nothing on it: dk must still run, and say what is missing.
    let empty = s.scratch.join("empty-path");
    fs::create_dir_all(&empty).unwrap();
    let out = run_with_path(&s, &["resume", "moxi"], &empty);
    assert!(out.status.success(), "{}", err(&out));

    let text = fs::read_to_string(s.scratch.join("RESUME.md")).unwrap();
    assert!(text.contains("[ygg not found"), "{text}");
}

#[cfg(unix)]
#[test]
fn resume_lists_the_manifest_through_ygg_from_the_project_directory() {
    let s = Sandbox::new();
    let project = s.project("moxi-proj", &[("WHITE.md", "src/main.rs\n")]);
    resume_card(&s, "moxi", "", Some(&project));
    let bin = fake_ygg(&s, RECORDING_YGG);

    let out = run_with_path(&s, &["resume", "moxi"], &bin);
    assert!(out.status.success(), "{}", err(&out));

    let text = fs::read_to_string(s.scratch.join("RESUME.md")).unwrap();
    assert!(text.contains("FAKE CODEX"), "{text}");
    assert!(text.contains("--white"), "{text}");
    assert!(!text.contains("--contents"), "the code part is the index, not the files: {text}");
    assert!(text.contains(&format!("{}", project.join("WHITE.md").display())), "{text}");
    assert!(text.contains("moxi-proj"), "ygg runs inside the project: {text}");
}

#[cfg(unix)]
#[test]
fn resume_uses_the_white_field_over_the_projects_manifest() {
    let s = Sandbox::new();
    let project = s.project("moxi-proj", &[("WHITE.md", "a\n"), ("alt.txt", "b\n")]);
    resume_card(&s, "moxi", "white: alt.txt\n", Some(&project));
    let bin = fake_ygg(&s, RECORDING_YGG);

    run_with_path(&s, &["resume", "moxi"], &bin);
    let text = fs::read_to_string(s.scratch.join("RESUME.md")).unwrap();
    assert!(text.contains("alt.txt"), "{text}");
    assert!(!text.contains("WHITE.md"), "{text}");
}

#[cfg(unix)]
#[test]
fn resume_reports_a_failing_ygg_with_its_own_words() {
    let s = Sandbox::new();
    let project = s.project("moxi-proj", &[("WHITE.md", "src/main.rs\n")]);
    resume_card(&s, "moxi", "", Some(&project));
    let bin = fake_ygg(&s, "echo 'boom: no such file' >&2\nexit 3");

    let out = run_with_path(&s, &["resume", "moxi"], &bin);
    assert!(out.status.success(), "a broken codex must not lose the card: {}", err(&out));
    let text = fs::read_to_string(s.scratch.join("RESUME.md")).unwrap();
    assert!(text.contains("[ygg failed: boom: no such file]"), "{text}");
    assert!(text.contains("## now\nparser"), "{text}");
}

#[test]
fn resume_refuses_to_write_into_the_store() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);

    // The store sits beside the working directory in the sandbox, so a relative
    // path reaches it too — the check has to see through `..`.
    let relative = s.run(&["resume", "moxi", "--out", "../store/RESUME.md"]);
    assert_eq!(relative.status.code(), Some(2), "{}", err(&relative));
    assert!(err(&relative).contains("store"), "{}", err(&relative));

    let absolute = s.run(&["resume", "moxi", "--out", &format!("{}/x.md", s.home.display())]);
    assert_eq!(absolute.status.code(), Some(2), "{}", err(&absolute));
    assert!(!s.home.join("RESUME.md").exists());
    assert!(!s.home.join("x.md").exists());
}

#[test]
fn resume_honours_a_chosen_filename() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    s.stdout(&["resume", "moxi", "--out", "handoff.md"]);
    assert!(s.scratch.join("handoff.md").exists());
    assert!(!s.scratch.join("RESUME.md").exists());
}

#[test]
fn resume_needs_a_card_that_exists() {
    let s = Sandbox::new();
    assert_eq!(s.run(&["resume"]).status.code(), Some(2));
    assert_eq!(s.run(&["resume", "ghost"]).status.code(), Some(3));
}

fn doc(title: &str, status: &str, body: &str) -> String {
    format!("<!-- dk:doc v1 -->\n# {title}\nstatus: {status}\n\n{body}\n")
}

#[test]
fn resume_lists_documents_by_name_and_never_inlines_them() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    sessions(
        &s,
        "moxi-sessions-3f2a91c4",
        &format!("dk-{CARD_ID}"),
        &[
            ("SES-20260901-110233.md", entry("title-1", "next-1")),
            ("DOC-20260902-old-plan.md", doc("Old plan", "superseded", "OLD PLAN BODY")),
            ("SES-20260915-090000.md", entry("title-2", "next-2")),
            ("SES-20260920-090000.md", entry("title-3", "next-3")),
            ("DOC-20261001-workflow-design.md", doc("Unified workflow", "adopted", &"WORKFLOW BODY ".repeat(100))),
            ("SES-20260930-221400.md", entry("title-4", "next-4")),
        ],
    );

    let text = resumed(&s, "moxi");
    assert!(!text.contains("WORKFLOW BODY"), "documents are listed, not inlined: {text}");
    assert!(!text.contains("OLD PLAN BODY"), "{text}");

    // Newest first, with title, status, cost and date.
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("missing {needle:?}:\n{text}"));
    assert!(text.contains("- DOC-20261001-workflow-design · Unified workflow · adopted · 364 tok · 2026-10-01"), "{text}");
    assert!(text.contains("- DOC-20260902-old-plan · Old plan · superseded · "), "{text}");
    assert!(at("DOC-20261001-workflow-design") < at("DOC-20260902-old-plan"), "{text}");

    // Documents sit after the whole entries and before the earlier-index, and
    // they do not use up the three-entry window.
    assert!(at("title-2") < at("### documents"), "{text}");
    assert!(at("### documents") < at("### earlier"), "{text}");
    assert!(text.contains("- SES-20260901-110233 · next-1 · 2026-09-01"), "only title-1 is older than the newest three: {text}");
    assert!(!text.contains("# moxi · title-1"), "{text}");
}

#[test]
fn a_conversation_with_only_documents_still_says_no_sessions() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    sessions(
        &s,
        "moxi-sessions-3f2a91c4",
        &format!("dk-{CARD_ID}"),
        &[("DOC-20261001-plan.md", doc("Plan", "draft", "x"))],
    );
    let text = resumed(&s, "moxi");
    assert!(text.contains("[no sessions yet]"), "{text}");
    assert!(text.contains("- DOC-20261001-plan · Plan · draft · "), "{text}");
}

// ---- books ----------------------------------------------------------------

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

impl Sandbox {
    /// A second folder of cards, outside the sandbox's own store.
    fn folder(&self, name: &str) -> PathBuf {
        let dir = self.scratch.join(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Run with the registry deciding the book, and expect success.
    fn books_ok(&self, args: &[&str]) -> String {
        let out = self.run_books(args, &[]);
        assert!(out.status.success(), "`{args:?}` failed: {}", err(&out));
        text(&out)
    }
}

const CARD: &str = "# moxi\nid: a43b21c0\nstatus: active\nwhat: a language\n";

#[test]
fn with_no_registry_book_says_there_is_one_store() {
    let s = Sandbox::new();
    let out = s.books_ok(&["book"]);
    assert!(out.contains("one store"), "{out}");
}

#[test]
fn the_first_book_keeps_your_existing_store_as_the_default() {
    let s = Sandbox::new();
    s.card("moxi", CARD);
    let bcrp = s.folder("bcrp");

    // The existing store is found through DOCKET_HOME, as it would be for you.
    let made = s.run(&["book", "new", "bcrp", bcrp.to_str().unwrap()]);
    assert!(made.status.success(), "{}", err(&made));
    assert!(text(&made).contains("your existing store"), "{}", text(&made));

    // From here the registry decides. Two books, so bare `dk` is the index
    // (as text, through a pipe), with the old store marked as the default...
    let index = s.books_ok(&[]);
    assert!(index.lines().any(|l| l.starts_with("* store")), "{index}");
    assert!(index.contains("bcrp"), "{index}");
    // ...every other command uses that default...
    assert!(s.books_ok(&["show", "moxi"]).contains("a language"));
    let list = s.books_ok(&["-b", "store"]);
    assert!(list.contains("moxi") && list.contains("book: store"), "{list}");
    // ...and the new book is empty and named on request.
    let empty = s.books_ok(&["-b", "bcrp"]);
    assert!(empty.contains("book: bcrp") && empty.contains("no cards yet"), "{empty}");
}

#[test]
fn a_book_listing_shows_counts_and_marks_the_default() {
    let s = Sandbox::new();
    s.card("moxi", CARD);
    s.card("old", "# old\nid: 0badf00d\nstatus: done\nwhat: finished\n");
    let bcrp = s.folder("bcrp");
    s.run(&["book", "new", "bcrp", bcrp.to_str().unwrap()]);

    let out = s.books_ok(&["book"]);
    let store_row = out.lines().find(|l| l.contains(" store ")).expect(&out);
    let bcrp_row = out.lines().find(|l| l.contains(" bcrp ")).expect(&out);
    assert!(store_row.starts_with('*'), "{out}");
    assert!(!bcrp_row.starts_with('*'), "{out}");
    // two cards, one of them live
    assert!(store_row.contains("    2") && store_row.contains("     1"), "{store_row}");
}

#[test]
fn listing_books_never_rewrites_a_card() {
    let s = Sandbox::new();
    let bare = "# bare\nstatus: active\nwhat: no id yet\n";
    s.card("bare", bare);
    let bcrp = s.folder("bcrp");
    s.run(&["book", "new", "bcrp", bcrp.to_str().unwrap()]);
    s.books_ok(&["book"]);
    assert_eq!(s.read("bare"), bare, "`dk book` must only read");
}

#[test]
fn a_qualified_name_picks_its_book_and_beats_the_environment() {
    let s = Sandbox::new();
    let work = s.folder("work");
    let home = s.folder("home");
    fs::write(work.join("moxi.md"), "# moxi\nid: a43b21c0\nstatus: active\nwhat: in work\n").unwrap();
    fs::write(home.join("moxi.md"), "# moxi\nid: b54c32d1\nstatus: active\nwhat: in home\n").unwrap();
    s.books_ok(&["book", "add", work.to_str().unwrap(), "work"]);
    s.books_ok(&["book", "add", home.to_str().unwrap(), "home"]);

    assert!(s.books_ok(&["show", "work/moxi"]).contains("in work"));
    assert!(s.books_ok(&["show", "home/moxi"]).contains("in home"));
    assert!(s.books_ok(&["show", "moxi", "-b", "home"]).contains("in home"));

    let env_home = s.run_books(&["show", "moxi"], &[("DOCKET_BOOK", "home")]);
    assert!(text(&env_home).contains("in home"), "{}", err(&env_home));
    let qualified_wins = s.run_books(&["show", "work/moxi"], &[("DOCKET_BOOK", "home")]);
    assert!(text(&qualified_wins).contains("in work"), "{}", err(&qualified_wins));
}

#[test]
fn a_flag_and_a_qualified_name_must_agree() {
    let s = Sandbox::new();
    let work = s.folder("work");
    let home = s.folder("home");
    s.books_ok(&["book", "add", work.to_str().unwrap(), "work"]);
    s.books_ok(&["book", "add", home.to_str().unwrap(), "home"]);
    let out = s.run_books(&["show", "work/moxi", "-b", "home"], &[]);
    assert_eq!(out.status.code(), Some(2), "{}", err(&out));
    assert!(err(&out).contains("different books"), "{}", err(&out));
}

#[test]
fn an_unknown_book_exits_three_and_names_the_known_ones() {
    let s = Sandbox::new();
    let work = s.folder("work");
    s.books_ok(&["book", "add", work.to_str().unwrap(), "work"]);
    let out = s.run_books(&["-b", "nope"], &[]);
    assert_eq!(out.status.code(), Some(3), "{}", err(&out));
    assert!(err(&out).contains("work"), "{}", err(&out));

    let none = Sandbox::new();
    let out = none.run_books(&["-b", "nope"], &[]);
    assert_eq!(out.status.code(), Some(3));
    assert!(err(&out).contains("no books are registered"), "{}", err(&out));
}

#[test]
fn several_books_and_no_default_ask_which_one() {
    let s = Sandbox::new();
    let a = s.folder("a");
    let b = s.folder("b");
    let c = s.folder("c");
    for (path, name) in [(&a, "a"), (&b, "b"), (&c, "c")] {
        s.books_ok(&["book", "add", path.to_str().unwrap(), name]);
    }
    // `a` became the default; removing it leaves two books and no default.
    s.books_ok(&["book", "rm", "a"]);
    let out = s.run_books(&["where"], &[]);
    assert_eq!(out.status.code(), Some(2), "{}", err(&out));
    assert!(err(&out).contains("no default"), "{}", err(&out));
    // Bare `dk` is the index, which is how you choose; it does not fail.
    assert!(s.books_ok(&[]).contains("no default"));

    s.books_ok(&["book", "use", "b"]);
    let b = fs::canonicalize(&b).unwrap();
    assert_eq!(s.books_ok(&["where"]).trim_end(), b.to_str().unwrap());
}

#[test]
fn removing_a_book_forgets_it_and_leaves_the_folder_and_cards_alone() {
    let s = Sandbox::new();
    let work = s.folder("work");
    fs::write(work.join("moxi.md"), CARD).unwrap();
    s.books_ok(&["book", "add", work.to_str().unwrap(), "work"]);
    let out = s.books_ok(&["book", "rm", "work"]);
    assert!(out.contains("untouched"), "{out}");
    assert!(work.join("moxi.md").exists());
    assert!(s.books_ok(&["book"]).contains("one store"));
}

#[test]
fn removing_the_default_hands_it_to_the_only_book_left() {
    let s = Sandbox::new();
    let a = s.folder("a");
    let b = s.folder("b");
    s.books_ok(&["book", "add", a.to_str().unwrap(), "a"]);
    s.books_ok(&["book", "add", b.to_str().unwrap(), "b"]);
    s.books_ok(&["book", "rm", "a"]);
    assert!(s.books_ok(&[]).contains("book: b"));
}

#[test]
fn a_book_name_must_be_plain_and_unused_and_a_folder_registers_once() {
    let s = Sandbox::new();
    let a = s.folder("a");
    let bad = s.run_books(&["book", "add", a.to_str().unwrap(), "Bad Name"], &[]);
    assert_eq!(bad.status.code(), Some(2), "{}", err(&bad));

    s.books_ok(&["book", "add", a.to_str().unwrap(), "a"]);
    let again = s.run_books(&["book", "add", a.to_str().unwrap(), "other"], &[]);
    assert_eq!(again.status.code(), Some(4), "{}", err(&again));
    assert!(err(&again).contains("already the book `a`"), "{}", err(&again));

    let b = s.folder("b");
    let taken = s.run_books(&["book", "add", b.to_str().unwrap(), "a"], &[]);
    assert_eq!(taken.status.code(), Some(4), "{}", err(&taken));
}

#[test]
fn a_book_whose_folder_has_gone_is_an_error_not_a_new_empty_book() {
    let s = Sandbox::new();
    let gone = s.folder("gone");
    s.books_ok(&["book", "add", gone.to_str().unwrap(), "gone"]);
    fs::remove_dir_all(&gone).unwrap();

    let out = s.run_books(&[], &[]);
    assert_eq!(out.status.code(), Some(1), "{}", err(&out));
    assert!(err(&out).contains("not a folder"), "{}", err(&out));
    assert!(!gone.exists(), "docket must not recreate a missing book");
    // The listing says so too, without failing.
    assert!(s.books_ok(&["book"]).contains("missing folder"));
}

#[test]
fn where_prints_the_chosen_books_path_alone() {
    let s = Sandbox::new();
    let a = s.folder("a");
    let b = s.folder("b");
    s.books_ok(&["book", "add", a.to_str().unwrap(), "a"]);
    s.books_ok(&["book", "add", b.to_str().unwrap(), "b"]);
    let got = s.books_ok(&["where", "-b", "b"]);
    assert_eq!(got.trim_end(), fs::canonicalize(&b).unwrap().to_str().unwrap());
}

#[test]
fn new_without_a_path_makes_a_folder_beside_the_data_directory() {
    let s = Sandbox::new();
    s.books_ok(&["book", "new", "demo"]);
    let list = s.books_ok(&["book"]);
    assert!(list.contains("docket-books"), "{list}");
    assert!(s.books_ok(&["-b", "demo"]).contains("no cards yet"));
}

#[test]
fn a_cards_work_happens_in_its_own_book() {
    let s = Sandbox::new();
    let work = s.folder("work");
    let home = s.folder("home");
    s.books_ok(&["book", "add", work.to_str().unwrap(), "work"]);
    s.books_ok(&["book", "add", home.to_str().unwrap(), "home"]);
    fs::write(work.join("moxi.md"), CARD).unwrap();

    s.books_ok(&["rename", "work/moxi", "work/moxi2"]);
    assert!(work.join("moxi2.md").exists() && !work.join("moxi.md").exists());
    let cross = s.run_books(&["rename", "work/moxi2", "home/moxi"], &[]);
    assert_eq!(cross.status.code(), Some(2), "{}", err(&cross));
    assert!(home.read_dir().unwrap().next().is_none(), "nothing may land in the other book");
}

#[test]
fn a_registry_that_cannot_be_read_is_reported_with_its_line() {
    let s = Sandbox::new();
    let config = s.home.parent().unwrap().join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("books.toml"), "[books]\nthis is not a book\n").unwrap();
    let out = s.run_books(&[], &[]);
    assert_eq!(out.status.code(), Some(1), "{}", err(&out));
    assert!(err(&out).contains("books.toml:2"), "{}", err(&out));
}

#[test]
fn bare_dk_with_several_books_is_the_index_and_with_one_is_the_cards() {
    let s = Sandbox::new();
    let a = s.folder("a");
    fs::write(a.join("moxi.md"), CARD).unwrap();
    s.books_ok(&["book", "add", a.to_str().unwrap(), "a"]);
    let one = s.books_ok(&[]);
    assert!(one.contains("moxi") && one.contains("book: a"), "{one}");

    let b = s.folder("b");
    s.books_ok(&["book", "add", b.to_str().unwrap(), "b"]);
    let index = s.books_ok(&[]);
    assert!(index.contains("NAME") && index.lines().any(|l| l.starts_with("* a")), "{index}");
    assert!(!index.contains("moxi"), "the index lists books, not cards: {index}");
}

#[test]
fn anything_that_already_chose_a_book_skips_the_index() {
    let s = Sandbox::new();
    let a = s.folder("a");
    let b = s.folder("b");
    fs::write(a.join("moxi.md"), CARD).unwrap();
    s.books_ok(&["book", "add", a.to_str().unwrap(), "a"]);
    s.books_ok(&["book", "add", b.to_str().unwrap(), "b"]);

    assert!(s.books_ok(&["-b", "a"]).contains("moxi"));
    let env = s.run_books(&[], &[("DOCKET_BOOK", "a")]);
    assert!(text(&env).contains("moxi"), "{}", err(&env));
    // DOCKET_HOME is a store chosen by hand: the plain list, no index.
    let home = s.run(&[]);
    assert!(text(&home).contains("no cards yet"), "{}", text(&home));
}

#[test]
fn managing_books_warns_when_docket_home_would_hide_them() {
    let s = Sandbox::new();
    let a = s.folder("a");
    // Sandbox::run sets DOCKET_HOME, as an old shell profile would.
    let with_home = s.run(&["book", "add", a.to_str().unwrap(), "a"]);
    assert!(with_home.status.success(), "{}", err(&with_home));
    assert!(err(&with_home).contains("DOCKET_HOME is set"), "{}", err(&with_home));
    let listed = s.run(&["book"]);
    assert!(err(&listed).contains("overrides your default book"), "{}", err(&listed));

    let without = s.run_books(&["book"], &[]);
    assert!(!err(&without).contains("DOCKET_HOME"), "{}", err(&without));
}

// ---------------------------------------------------------------------------
// places: one card, several folders
// ---------------------------------------------------------------------------

fn in_dir(s: &Sandbox, dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dk"))
        .args(args)
        .current_dir(dir)
        .env("DOCKET_HOME", &s.home)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn two_places(s: &Sandbox) -> (PathBuf, PathBuf) {
    let main = s.project("moxi-main", &[("WHITE.md", "src/main.rs\n")]);
    let eval = s.project("moxi-eval", &[("WHITE.md", "harness.py\n")]);
    resume_card(
        s,
        "moxi",
        &format!("place: eval {}\n", eval.display()),
        Some(&main),
    );
    (main, eval)
}

#[cfg(unix)]
#[test]
fn resume_indexes_every_place_under_its_label() {
    let s = Sandbox::new();
    let (main, eval) = two_places(&s);
    let bin = fake_ygg(&s, RECORDING_YGG);

    let out = run_with_path(&s, &["resume", "moxi"], &bin);
    assert!(out.status.success(), "{}", err(&out));
    let text = fs::read_to_string(s.scratch.join("RESUME.md")).unwrap();
    assert!(text.contains(&format!("### main · {}", main.display())), "{text}");
    assert!(text.contains(&format!("### eval · {}", eval.display())), "{text}");
    assert_eq!(text.matches("FAKE CODEX").count(), 2, "{text}");
    assert!(text.contains("place: eval "), "the card part keeps its place line: {text}");
}

#[cfg(unix)]
#[test]
fn resume_place_limits_the_code_to_one_folder() {
    let s = Sandbox::new();
    let (_, eval) = two_places(&s);
    let bin = fake_ygg(&s, RECORDING_YGG);

    let out = run_with_path(&s, &["resume", "moxi", "--place", "eval"], &bin);
    assert!(out.status.success(), "{}", err(&out));
    let text = fs::read_to_string(s.scratch.join("RESUME.md")).unwrap();
    assert!(text.contains(&format!("### eval · {}", eval.display())), "{text}");
    assert!(!text.contains("### main"), "{text}");
    assert_eq!(text.matches("FAKE CODEX").count(), 1, "{text}");
}

#[test]
fn resume_names_the_places_when_the_label_is_wrong() {
    let s = Sandbox::new();
    two_places(&s);
    let out = s.run(&["resume", "moxi", "--place", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    let said = err(&out);
    assert!(said.contains("no place `nope`") && said.contains("main, eval"), "{said}");
}

#[test]
fn a_card_with_one_place_resumes_without_headings() {
    let s = Sandbox::new();
    let project = s.project("moxi-proj", &[]);
    resume_card(&s, "moxi", "", Some(&project));
    let text = resumed(&s, "moxi");
    assert!(!text.contains("### main"), "{text}");
    assert!(text.contains("[no WHITE.md at "), "{text}");
}

#[test]
fn a_gone_place_is_reported_without_losing_the_others() {
    let s = Sandbox::new();
    let main = s.project("moxi-main", &[]);
    resume_card(&s, "moxi", "place: eval /nonexistent/moxi-eval\n", Some(&main));
    let text = resumed(&s, "moxi");
    assert!(text.contains("[no WHITE.md at "), "{text}");
    assert!(text.contains("### eval · /nonexistent/moxi-eval\n\n[project path is gone: /nonexistent/moxi-eval]"), "{text}");
}

#[test]
fn here_finds_the_card_from_any_of_its_places() {
    let s = Sandbox::new();
    let (main, eval) = two_places(&s);
    let nested = eval.join("tasks");
    fs::create_dir_all(&nested).unwrap();

    for dir in [&main, &eval, &nested] {
        let out = in_dir(&s, dir, &["here"]);
        assert!(out.status.success(), "{}: {}", dir.display(), err(&out));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "moxi\n");
    }
    let out = in_dir(&s, &eval, &["here"]);
    assert!(err(&out).contains("in place: eval"), "{}", err(&out));
}

#[test]
fn here_in_an_unclaimed_folder_exits_three() {
    let s = Sandbox::new();
    two_places(&s);
    let stranger = s.project("elsewhere", &[]);
    let out = in_dir(&s, &stranger, &["here"]);
    assert_eq!(out.status.code(), Some(3), "{}", err(&out));
    assert!(out.stdout.is_empty());
}

#[test]
fn here_prefers_the_deepest_place() {
    let s = Sandbox::new();
    let outer = s.project("outer", &[]);
    let inner = outer.join("inner");
    fs::create_dir_all(&inner).unwrap();
    s.card("outer", &format!("# outer\nid: aaaaaaaa\npath: {}\n", outer.display()));
    s.card("inner", &format!("# inner\nid: bbbbbbbb\npath: {}\n", inner.display()));
    let out = in_dir(&s, &inner, &["here"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "inner\n", "{}", err(&out));
}

#[test]
fn here_refuses_two_cards_claiming_the_same_folder() {
    let s = Sandbox::new();
    let dir = s.project("shared", &[]);
    s.card("one", &format!("# one\nid: aaaaaaaa\npath: {}\n", dir.display()));
    s.card("two", &format!("# two\nid: bbbbbbbb\nplace: x {}\n", dir.display()));
    let out = in_dir(&s, &dir, &["here"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(err(&out).contains("one, two"), "{}", err(&out));
}

// ---------------------------------------------------------------------------
// writing without an editor, and ending a session — the loop an AI runs
// ---------------------------------------------------------------------------

#[test]
fn the_writing_verbs_change_one_thing_each_and_undo_swaps_back() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    let before = s.read("moxi");

    s.stdout(&["set", "moxi", "status", "paused"]);
    s.stdout(&["set", "moxi", "place", "paper", "~/papers/x"]);
    s.stdout(&["set", "moxi", "state", "P0", "done"]);
    s.stdout(&["todo", "moxi", "write", "the", "intro"]);
    s.stdout(&["tick", "moxi", "M2"]);
    s.stdout(&["note", "moxi", "the", "game", "is", "the", "dev", "set"]);
    let out = s.run_with_stdin(&["note", "moxi", "--section", "decisions", "-"], "one paper,\ntwo studies\n");
    assert!(out.status.success(), "{}", err(&out));

    let card = s.read("moxi");
    assert!(card.contains("status: paused\n"), "{card}");
    assert!(card.contains("place: paper ~/papers/x\n\n## now"), "{card}");
    assert!(card.contains("## now\nstate: P0 done\nparser"), "{card}");
    assert!(card.contains("[x] M2\n[ ] write the intro\n"), "{card}");
    assert!(card.contains("## notes\nthe game is the dev set\n"), "{card}");
    assert!(card.contains("## decisions\none paper,\ntwo studies\n"), "{card}");
    assert!(card.contains(&format!("id: {CARD_ID}")), "the id never moves");

    // One level of undo, and undo of undo is redo.
    s.stdout(&["undo", "moxi"]);
    assert!(!s.read("moxi").contains("## decisions"));
    s.stdout(&["undo", "moxi"]);
    assert!(s.read("moxi").contains("## decisions"));

    // A whole-card rewrite keeps the id even when the new text leaves it out.
    let out = s.run_with_stdin(&["write", "moxi", "-"], "# moxi\nstatus: active\nwhat: new\n\n## now\nfresh\n");
    assert!(out.status.success(), "{}", err(&out));
    assert!(s.read("moxi").starts_with(&format!("# moxi\nid: {CARD_ID}\nstatus: active")));
    let out = s.run_with_stdin(&["write", "moxi", "-"], "# moxi\nid: deadbeef\n");
    assert!(!out.status.success(), "a different id is refused");
    s.stdout(&["undo", "moxi"]);
    assert_ne!(s.read("moxi"), before, "undo went back one step, not to the start");
}

#[test]
fn tick_refuses_to_guess_and_leaves_the_card_alone() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    s.stdout(&["todo", "moxi", "M2b"]);
    let before = s.read("moxi");
    let out = s.run(&["tick", "moxi", "M"]);
    assert!(!out.status.success());
    assert!(err(&out).contains("2 open boxes"), "{}", err(&out));
    assert_eq!(s.read("moxi"), before);
    assert!(!s.run(&["note", "moxi", "--section", "readme", "x"]).status.success());
}

#[test]
fn save_creates_the_conversation_writes_back_the_card_and_resume_reads_it() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    let entry_file = s.scratch.join("entry.md");
    // No marker line: save adds it.
    fs::write(&entry_file, entry("first", "P1 · sonnet medium · tests").replace("<!-- dk:session v1 -->\n", "")).unwrap();
    let doc_file = s.scratch.join("plan.md");
    fs::write(&doc_file, "# The v2 plan\nstatus: draft\n\nwhy\n").unwrap();

    let dry = s.run(&["save", "moxi", entry_file.to_str().unwrap(), "--dry-run"]);
    assert!(dry.status.success(), "{}", err(&dry));
    assert!(!s.home.join("sessions").exists(), "a dry run writes nothing");

    let out = s.run(&[
        "save", "moxi", entry_file.to_str().unwrap(),
        "--doc", doc_file.to_str().unwrap(),
        "--tick", "M2", "--next", "write the intro",
    ]);
    assert!(out.status.success(), "{}", err(&out));
    let written: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(String::from).collect();
    assert_eq!(written.len(), 2, "{written:?}");
    assert!(written[0].contains("DOC-") && written[0].ends_with("-the-v2-plan.md"), "{written:?}");
    assert!(written[1].contains("/SES-"), "{written:?}");

    let chats = s.home.join("sessions").join("chats");
    let folders: Vec<_> = fs::read_dir(&chats).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(folders.len(), 1);
    let spine = fs::read_to_string(folders[0].join("convo.md")).unwrap();
    assert!(spine.contains(&format!("  - dk-{CARD_ID}\n")), "{spine}");
    let markers: Vec<&str> = spine.lines().filter(|l| l.starts_with("<!-- fur:msg")).collect();
    assert_eq!(markers.len(), 2);
    assert!(markers[0].contains("link=DOC-") && markers[1].contains("link=SES-"), "doc before entry");
    assert!(fs::read_to_string(&written[1]).unwrap().starts_with("<!-- dk:session v1 -->\n# moxi"));

    let card = s.read("moxi");
    assert!(card.contains("## now\nstate: M1\n"), "{card}");
    assert!(card.contains("[x] M2\n[ ] write the intro\n"), "{card}");

    // A second save appends to the same conversation; resume shows it first.
    let second = s.run_with_stdin(&["save", "moxi", "-"], &entry("second", "P2"));
    assert!(second.status.success(), "{}", err(&second));
    let spine = fs::read_to_string(folders[0].join("convo.md")).unwrap();
    assert_eq!(spine.lines().filter(|l| l.starts_with("<!-- fur:msg")).count(), 3);
    let text = String::from_utf8_lossy(&s.run(&["resume", "moxi", "--out", "-"]).stdout).into_owned();
    let (second_at, first_at) = (text.find("# moxi · second").unwrap(), text.find("# moxi · first").unwrap());
    assert!(second_at < first_at, "newest first: {text}");
    assert!(text.contains("### documents") && text.contains("The v2 plan"), "{text}");
    assert!(!s.scratch.join("RESUME.md").exists(), "--out - writes no file");
}

#[test]
fn save_checks_everything_before_writing_anything() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    let before = s.read("moxi");
    // A bad --tick stops the save.
    let out = s.run_with_stdin(&["save", "moxi", "-", "--tick", "no such box"], &entry("x", "y"));
    assert!(!out.status.success());
    // A malformed entry names what is missing.
    let broken = entry("x", "y").replace("## blockers\nnone\n\n", "");
    let out = s.run_with_stdin(&["save", "moxi", "-"], &broken);
    assert!(!out.status.success());
    assert!(err(&out).contains("## blockers"), "{}", err(&out));
    assert!(!s.home.join("sessions").exists());
    assert_eq!(s.read("moxi"), before);
}

#[test]
fn save_revises_a_document_in_place_without_a_new_marker() {
    let s = Sandbox::new();
    resume_card(&s, "moxi", "", None);
    let dir = sessions(&s, "moxi-sessions-3f2a91c4", &format!("dk-{CARD_ID}"), &[
        ("DOC-20261001-plan.md", "<!-- dk:doc v1 -->\n# Plan\nstatus: draft\n".into()),
    ]);
    let revised = s.scratch.join("DOC-20261001-plan.md");
    fs::write(&revised, "# Plan\nstatus: adopted\n").unwrap();
    let out = s.run_with_stdin(&["save", "moxi", "-", "--doc", revised.to_str().unwrap()], &entry("x", "y"));
    assert!(out.status.success(), "{}", err(&out));
    let spine = fs::read_to_string(dir.join("convo.md")).unwrap();
    assert_eq!(spine.matches("link=DOC-20261001-plan.md").count(), 1, "{spine}");
    assert!(fs::read_to_string(dir.join("DOC-20261001-plan.md")).unwrap().contains("status: adopted"));
}
