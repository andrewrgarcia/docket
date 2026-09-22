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

    Derived {
        name,
        what: if what.is_empty() {
            readme_tagline(dir).unwrap_or_default()
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

/// The first prose line of a README, skipping headings, badges and blank space.
fn readme_tagline(dir: &Path) -> Option<String> {
    let text = README_FILES
        .iter()
        .find_map(|name| fs::read_to_string(dir.join(name)).ok())?;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with('#') && !l.starts_with('<') && !l.starts_with("[!["))
        .map(|l| l.trim_matches('*').trim())
        .find(|l| l.len() > 10)
        .map(|l| truncate(l, 100))
}

fn clean(value: &str) -> Option<String> {
    let v = value
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches('"')
        .trim();
    if v.is_empty() {
        None
    } else {
        Some(truncate(v, 100))
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
