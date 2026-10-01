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
fn resume_inlines_the_codex_ygg_writes_from_the_project_directory() {
    let s = Sandbox::new();
    let project = s.project("moxi-proj", &[("WHITE.md", "src/main.rs\n")]);
    resume_card(&s, "moxi", "", Some(&project));
    let bin = fake_ygg(&s, RECORDING_YGG);

    let out = run_with_path(&s, &["resume", "moxi"], &bin);
    assert!(out.status.success(), "{}", err(&out));

    let text = fs::read_to_string(s.scratch.join("RESUME.md")).unwrap();
    assert!(text.contains("FAKE CODEX"), "{text}");
    assert!(text.contains("--white"), "{text}");
    assert!(text.contains("--contents"), "{text}");
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
