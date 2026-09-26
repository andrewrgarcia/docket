use std::fs;
use std::path::Path;

/// What `add <path>` can work out on its own. Everything here is a guess the
/// user can overwrite afterwards; nothing is re-derived later, because a card
/// that rewrites itself is a card you stop trusting.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Derived {
    /// The project's own name, when a manifest states one. A repository whose
    /// crate lives in `./cli` is called by its package name, not `cli`.
    pub name: String,
    pub what: String,
    /// Relative path of an agent instruction file, if the project has one.
    pub agents: String,
    /// The README, whole. Stored in the card so the model reads what the
    /// project says about itself, not a hundred-character summary of it.
    pub readme: String,
}

/// Manifest, whether it is TOML, and the section a package's own fields live
/// in. The section matters: `name` appears again under `[dependencies]` and
/// `[[bin]]`, and the first match would be the wrong one.
const MANIFESTS: [(&str, Syntax, &str); 5] = [
    ("Cargo.toml", Syntax::Toml, "package"),
    ("pyproject.toml", Syntax::Toml, "project"),
    ("package.json", Syntax::Json, ""),
    ("composer.json", Syntax::Json, ""),
    ("deno.json", Syntax::Json, ""),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Toml,
    Json,
}

const README_FILES: [&str; 4] = ["README.md", "readme.md", "README.rst", "README.txt"];

const AGENT_FILES: [&str; 3] = ["AGENTS.md", "CLAUDE.md", ".cursorrules"];

pub fn inspect(dir: &Path) -> Derived {
    let manifest = MANIFESTS
        .iter()
        .find_map(|(file, syntax, section)| read(&dir.join(file), *syntax, section));

    let (name, what) = manifest.unwrap_or_default();
    // With no manifest, the directory is the best name we have for deciding
    // whether a README's first line is just the project's own title.
    let label = if name.is_empty() {
        dir.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string()
    } else {
        name.clone()
    };

    Derived {
        name,
        what: if what.is_empty() {
            readme_tagline(dir, &label).unwrap_or_default()
        } else {
            what
        },
        agents: AGENT_FILES
            .iter()
            .find(|f| dir.join(f).is_file())
            .map(|f| (*f).to_string())
            .unwrap_or_default(),
        readme: readme(dir).unwrap_or_default(),
    }
}

/// The README verbatim. A README past this size is generated, vendored or a
/// book, and pasting it into every brief would crowd out the cards.
const README_LIMIT: usize = 100_000;

fn readme(dir: &Path) -> Option<String> {
    let text = README_FILES
        .iter()
        .find_map(|name| fs::read_to_string(dir.join(name)).ok())?;
    if text.trim().is_empty() {
        return None;
    }
    if text.len() > README_LIMIT {
        let kept: String = text.chars().take(README_LIMIT).collect();
        return Some(format!(
            "{kept}\n\n[truncated by docket at {README_LIMIT} characters]"
        ));
    }
    Some(text)
}

/// `(name, description)` from one manifest. Hand-rolled rather than three
/// parser crates: this is a starting guess, and a wrong one costs an edit.
fn read(path: &Path, syntax: Syntax, section: &str) -> Option<(String, String)> {
    let text = fs::read_to_string(path).ok()?;
    let (name, description) = match syntax {
        Syntax::Toml => {
            let body = toml_section(&text, section)?;
            (field(body, "name", '='), field(body, "description", '='))
        }
        Syntax::Json => (
            field(&text, "\"name\"", ':'),
            field(&text, "\"description\"", ':'),
        ),
    };
    if name.is_none() && description.is_none() {
        return None;
    }
    Some((name.unwrap_or_default(), description.unwrap_or_default()))
}

/// The lines of `[section]`, up to the next section header.
fn toml_section<'a>(text: &'a str, section: &str) -> Option<&'a str> {
    let header = format!("[{section}]");
    let start = text.find(&header)? + header.len();
    let rest = &text[start..];
    let end = rest.find("\n[").map_or(rest.len(), |at| at + 1);
    Some(&rest[..end])
}

fn field(text: &str, key: &str, separator: char) -> Option<String> {
    let line = text.lines().find(|l| {
        let trimmed = l.trim_start();
        trimmed.starts_with(key)
            && trimmed[key.len()..]
                .trim_start()
                .starts_with(separator)
    })?;
    let (_, value) = line.split_once(separator)?;
    clean(value)
}

/// The first line of a README that reads like a description of the project.
///
/// A README opens with anything: a centred logo, a badge row, an HTML block,
/// a title, a blockquote warning. The first line that is none of those, is
/// prose rather than markup, and is not just the project's own name is the
/// closest thing to a one-line summary the file has. Markdown is stripped,
/// because this lands in a table, and `[Next.js](https://nextjs.org)` in a
/// table column is noise.
fn readme_tagline(dir: &Path, name: &str) -> Option<String> {
    let text = README_FILES
        .iter()
        .find_map(|name| fs::read_to_string(dir.join(name)).ok())?;

    let mut fence: Option<usize> = None;
    for raw in text.lines() {
        let line = raw.trim();

        // Code blocks are never a description.
        let ticks = line.chars().take_while(|c| *c == '`').count();
        if ticks >= 3 {
            fence = if fence.is_some() { None } else { Some(ticks) };
            continue;
        }
        if fence.is_some() || line.is_empty() {
            continue;
        }
        // Headings, HTML, badges, quotes, lists, tables, rules, front matter.
        if line.starts_with('#')
            || line.starts_with('<')
            || line.starts_with('>')
            || line.starts_with('|')
            || line.starts_with("- ")
            || line.starts_with("* ")
            || line.starts_with("---")
            || line.starts_with("===")
        {
            continue;
        }

        let plain = strip_markdown(line);
        // Too short to be a sentence, or the project's name again.
        if plain.chars().count() < 12 || plain.to_ascii_lowercase() == name.to_ascii_lowercase() {
            continue;
        }
        // What is left after stripping links and images is what we keep; a
        // line that was mostly markup leaves little behind and is skipped.
        if plain.chars().count() * 2 < line.chars().count() {
            continue;
        }
        return Some(truncate(&plain, 100));
    }
    None
}

/// Inline markdown to plain text: link and image text without their targets,
/// no emphasis marks, no inline code ticks. Deliberately not a parser — this
/// is a table cell, and the worst case is a slightly odd sentence.
fn strip_markdown(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // `![alt](src)` — drop the image entirely, alt text included.
            '!' if chars.peek() == Some(&'[') => {
                chars.next();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                }
                skip_target(&mut chars);
            }
            // `[text](url)` — keep the text, drop the url.
            '[' => {
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    out.push(c);
                }
                skip_target(&mut chars);
            }
            '*' | '_' | '`' => {}
            c => out.push(c),
        }
    }

    // Collapse the whitespace the stripping leaves behind.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Consume a `(...)` target, if one follows.
fn skip_target(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if chars.peek() != Some(&'(') {
        return;
    }
    chars.next();
    let mut depth = 1;
    for c in chars.by_ref() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
}

fn clean(value: &str) -> Option<String> {
    let v = value
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches('"')
        .trim();
    let v = strip_markdown(v);
    if v.is_empty() {
        None
    } else {
        Some(truncate(&v, 100))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("docket-derive-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn takes_the_package_name_not_the_section_that_follows() {
        let dir = scratch("cargo");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\n\
             name = \"fur-cli\"\n\
             description = \"a local-first diary\"\n\
             \n\
             [dependencies]\n\
             name = \"wrong\"\n\
             \n\
             [[bin]]\n\
             name = \"fur\"\n",
        )
        .unwrap();
        let found = inspect(&dir);
        assert_eq!(found.name, "fur-cli");
        assert_eq!(found.what, "a local-first diary");
    }

    #[test]
    fn reads_package_json() {
        let dir = scratch("npm");
        fs::write(
            dir.join("package.json"),
            "{\n  \"name\": \"my-app\",\n  \"description\": \"a web thing\"\n}\n",
        )
        .unwrap();
        let found = inspect(&dir);
        assert_eq!(found.name, "my-app");
        assert_eq!(found.what, "a web thing");
    }

    #[test]
    fn reads_pyproject_project_section() {
        let dir = scratch("py");
        fs::write(
            dir.join("pyproject.toml"),
            "[project]\nname = \"macrogym\"\ndescription = \"model selection\"\n",
        )
        .unwrap();
        assert_eq!(inspect(&dir).name, "macrogym");
    }

    #[test]
    fn a_manifest_with_no_description_still_yields_the_name() {
        let dir = scratch("nodesc");
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"bare\"\n").unwrap();
        fs::write(dir.join("README.md"), "# bare\n\nDoes one useful thing.\n").unwrap();
        let found = inspect(&dir);
        assert_eq!(found.name, "bare");
        assert_eq!(found.what, "Does one useful thing.");
    }

    #[test]
    fn falls_back_to_the_readme_skipping_badges() {
        let dir = scratch("readme");
        fs::write(
            dir.join("README.md"),
            "# project\n\n[![badge](x)](y)\n\nA tool that does the thing.\n",
        )
        .unwrap();
        let found = inspect(&dir);
        assert!(found.name.is_empty());
        assert_eq!(found.what, "A tool that does the thing.");
    }

    #[test]
    fn links_and_emphasis_are_stripped_from_the_description() {
        let dir = scratch("markdown");
        fs::write(
            dir.join("README.md"),
            "# prince\n\nThis is a [Next.js](https://nextjs.org/) project, **bootstrapped** fast.\n",
        )
        .unwrap();
        assert_eq!(
            inspect(&dir).what,
            "This is a Next.js project, bootstrapped fast."
        );
    }

    #[test]
    fn an_html_masthead_and_a_blockquote_are_skipped() {
        let dir = scratch("html");
        fs::write(
            dir.join("README.md"),
            "<p align=\"center\">\n<img src=\"logo.png\"/>\n</p>\n\n<h1>FUR</h1>\n\n\
             > **Security notice.** Something urgent.\n\n\
             FUR is a command-line system for archiving AI chats.\n",
        )
        .unwrap();
        assert_eq!(
            inspect(&dir).what,
            "FUR is a command-line system for archiving AI chats."
        );
    }

    #[test]
    fn a_line_that_is_only_the_project_name_is_not_a_description() {
        let dir = scratch("name-echo");
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"macrogym\"\n").unwrap();
        fs::write(
            dir.join("README.md"),
            "macrogym\n\nCounterfactual model selection for macroeconomics.\n",
        )
        .unwrap();
        assert_eq!(
            inspect(&dir).what,
            "Counterfactual model selection for macroeconomics."
        );
    }

    #[test]
    fn a_code_fence_is_never_the_description() {
        let dir = scratch("fence");
        fs::write(
            dir.join("README.md"),
            "# thing\n\n```bash\ncargo install thing --locked --force\n```\n\nDoes the thing well.\n",
        )
        .unwrap();
        assert_eq!(inspect(&dir).what, "Does the thing well.");
    }

    #[test]
    fn a_mostly_markup_line_is_skipped() {
        let dir = scratch("markup");
        fs::write(
            dir.join("README.md"),
            "[docs](https://example.com/very/long/path/to/documentation/page)\n\n\
             A real sentence about the project.\n",
        )
        .unwrap();
        assert_eq!(inspect(&dir).what, "A real sentence about the project.");
    }

    #[test]
    fn bullet_lists_and_tables_are_not_descriptions() {
        let dir = scratch("lists");
        fs::write(
            dir.join("README.md"),
            "# thing\n\n- first bullet point here\n| a | b |\n\nThe actual summary line.\n",
        )
        .unwrap();
        assert_eq!(inspect(&dir).what, "The actual summary line.");
    }

    #[test]
    fn a_manifest_description_is_stripped_too() {
        let dir = scratch("manifest-md");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"x\"\ndescription = \"A **fast** [tool](https://x.dev) for things\"\n",
        )
        .unwrap();
        assert_eq!(inspect(&dir).what, "A fast tool for things");
    }

    #[test]
    fn notices_an_agent_instruction_file() {
        let dir = scratch("agents");
        fs::write(dir.join("AGENTS.md"), "be careful\n").unwrap();
        assert_eq!(inspect(&dir).agents, "AGENTS.md");
    }

    #[test]
    fn long_descriptions_are_cut_at_a_hundred_characters() {
        let dir = scratch("long");
        let long = "x".repeat(300);
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"x\"\ndescription = \"{long}\"\n"),
        )
        .unwrap();
        assert_eq!(inspect(&dir).what.chars().count(), 100);
    }

    #[test]
    fn the_whole_readme_is_captured() {
        let dir = scratch("full-readme");
        let body = "# project\n\nA tool that does the thing.\n\n## Install\n\ncargo install x\n";
        fs::write(dir.join("README.md"), body).unwrap();
        let found = inspect(&dir);
        assert_eq!(found.readme, body);
        assert_eq!(found.what, "A tool that does the thing.");
    }

    #[test]
    fn an_enormous_readme_is_truncated_with_a_marker() {
        let dir = scratch("big-readme");
        fs::write(dir.join("README.md"), "x".repeat(README_LIMIT + 5_000)).unwrap();
        let found = inspect(&dir);
        assert!(found.readme.ends_with("characters]"));
        assert!(found.readme.chars().count() < README_LIMIT + 100);
    }

    #[test]
    fn an_empty_directory_derives_nothing() {
        assert_eq!(inspect(&scratch("empty")), Derived::default());
    }
}
