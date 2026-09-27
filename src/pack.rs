//! Writing the selection as a zip: one markdown file per card.
//!
//! `p` writes one document because that is what you paste. `z` writes an
//! archive because that is what you attach — several models take a zip and
//! read the files inside it, and one card per file keeps them apart.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::card::Card;
use crate::error::{Error, Result};

/// `(name, markdown)` per card, plus an index. Returns the number of entries.
pub fn write(path: &Path, index: &str, cards: &[(String, String)]) -> Result<usize> {
    let file = File::create(path).map_err(|e| Error::io("create", path, e))?;
    let mut zip = ZipWriter::new(file);
    // Deflate: a brief is text, and the archive is meant to be attached.
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let fail = |e: zip::result::ZipError| Error::other(format!("zip: {e}"));

    zip.start_file("INDEX.md", options).map_err(fail)?;
    zip.write_all(index.as_bytes()).map_err(|e| Error::io("write", path, e))?;

    for (name, body) in cards {
        zip.start_file(format!("{name}.md"), options).map_err(fail)?;
        zip.write_all(body.as_bytes()).map_err(|e| Error::io("write", path, e))?;
    }

    zip.finish().map_err(fail)?;
    Ok(cards.len() + 1)
}

/// One card's markdown: its header, then the chosen headings in document
/// order. The same shape `brief` writes, minus the index.
pub fn card_markdown(card: &Card, chosen: &[usize]) -> String {
    let mut out = String::new();
    out.push_str(card.header().trim_end());
    out.push_str("\n\n");

    let outline = card.full_outline();
    for index in 0..outline.nodes.len() {
        if chosen.contains(&index) {
            out.push_str(&outline.text_of(index));
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cards_markdown_is_its_header_and_chosen_headings() {
        let card = Card::parse(
            "moxi",
            "# moxi\nid: a43b\nstatus: active\n\n## now\nparser\n\n## next\nspans\n",
            0,
        );
        let text = card_markdown(&card, &[0]);
        assert!(text.starts_with("# moxi\nid: a43b"));
        assert!(text.contains("## now\nparser"));
        assert!(!text.contains("spans"));
    }

    #[test]
    fn a_zip_holds_an_index_and_one_file_per_card() {
        let path = std::env::temp_dir().join(format!("dk-pack-{}.zip", std::process::id()));
        let cards = vec![
            ("moxi".to_string(), "# moxi\n".to_string()),
            ("kol".to_string(), "# kol\n".to_string()),
        ];
        assert_eq!(write(&path, "# DOCKET\n", &cards).unwrap(), 3);

        let file = File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(names, vec!["INDEX.md", "moxi.md", "kol.md"]);

        let _ = std::fs::remove_file(&path);
    }
}
