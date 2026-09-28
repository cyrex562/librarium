//! Tag listing, backed by `librarium_core::frontmatter_service`. Mirrors
//! `GET /api/vaults/{id}/tags` (`routes/tags.rs::list_tags`) exactly — same
//! scan, same frontmatter + inline `#tag` extraction, same sort.
//!
//! `tag_files` has no separate REST endpoint (the server's `TagEntry` already
//! embeds `files` per tag), but is listed explicitly in the issue as its own
//! command — useful on mobile where a caller may want just one tag's files
//! without listing every tag's frontmatter payload first.
//!
//! Tag deletion (`DELETE /api/vaults/{id}/tags/{tag}`) is out of scope here —
//! not in the issue's command list.

use librarium_core::error::AppResult;
use librarium_core::frontmatter_service;
use serde::Serialize;
use std::collections::HashMap;
use walkdir::WalkDir;

/// Mirrors `TagEntry` in `routes/tags.rs`.
#[derive(Debug, Clone, Serialize)]
pub struct TagEntry {
    pub tag: String,
    pub count: usize,
    pub files: Vec<String>,
}

fn scan_tags(vault_path: &str) -> HashMap<String, Vec<String>> {
    let mut tag_map: HashMap<String, Vec<String>> = HashMap::new();

    for entry in WalkDir::new(vault_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && librarium_core::note_links::is_note_file(e.path()))
    {
        let rel_path = entry
            .path()
            .strip_prefix(vault_path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");

        if let Ok(raw) = std::fs::read_to_string(entry.path()) {
            let (fm, body) =
                frontmatter_service::parse_note_frontmatter(&rel_path, &raw).unwrap_or((None, raw));
            let tags = frontmatter_service::note_tags(&rel_path, fm.as_ref(), &body);
            for tag in tags {
                tag_map.entry(tag).or_default().push(rel_path.clone());
            }
        }
    }

    tag_map
}

/// Mirrors `GET /api/vaults/{id}/tags`.
pub async fn tags_list(vault_path: &str) -> AppResult<Vec<TagEntry>> {
    let vault_path = vault_path.to_string();
    tokio::task::spawn_blocking(move || {
        let mut entries: Vec<TagEntry> = scan_tags(&vault_path)
            .into_iter()
            .map(|(tag, mut files)| {
                files.sort();
                let count = files.len();
                TagEntry { tag, count, files }
            })
            .collect();
        entries.sort_by_key(|e| e.tag.to_lowercase());
        entries
    })
    .await
    .map_err(|e| {
        librarium_core::error::AppError::InternalError(format!("tags_list task join error: {e}"))
    })
}

/// Vault-relative paths of every file tagged `tag` (case-sensitive, matching
/// `extract_tags`'s own comparison). Empty (not an error) when the tag is
/// unused.
pub async fn tag_files(vault_path: &str, tag: &str) -> AppResult<Vec<String>> {
    let vault_path = vault_path.to_string();
    let tag = tag.to_string();
    tokio::task::spawn_blocking(move || {
        let mut files = scan_tags(&vault_path).remove(&tag).unwrap_or_default();
        files.sort();
        files
    })
    .await
    .map_err(|e| {
        librarium_core::error::AppError::InternalError(format!("tag_files task join error: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn vault_with_tags() -> TempDir {
        let vault = TempDir::new().unwrap();
        std::fs::write(
            vault.path().join("a.md"),
            "---\ntags: [rust, backend]\n---\nbody #inline",
        )
        .unwrap();
        std::fs::write(vault.path().join("b.md"), "no tags here").unwrap();
        std::fs::write(vault.path().join("c.md"), "tagged #rust only").unwrap();
        vault
    }

    #[tokio::test]
    async fn tags_list_aggregates_frontmatter_and_inline_tags() {
        let vault = vault_with_tags();
        let entries = tags_list(vault.path().to_str().unwrap()).await.unwrap();

        let rust = entries.iter().find(|e| e.tag == "rust").unwrap();
        assert_eq!(rust.count, 2);
        assert!(rust.files.contains(&"a.md".to_string()));
        assert!(rust.files.contains(&"c.md".to_string()));

        let backend = entries.iter().find(|e| e.tag == "backend").unwrap();
        assert_eq!(backend.count, 1);

        let inline = entries.iter().find(|e| e.tag == "inline").unwrap();
        assert_eq!(inline.files, vec!["a.md".to_string()]);
    }

    #[tokio::test]
    async fn tags_list_sorted_case_insensitively() {
        let vault = TempDir::new().unwrap();
        std::fs::write(
            vault.path().join("a.md"),
            "---\ntags: [Zebra, apple]\n---\n",
        )
        .unwrap();
        let entries = tags_list(vault.path().to_str().unwrap()).await.unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.tag.as_str()).collect();
        assert_eq!(names, vec!["apple", "Zebra"]);
    }

    #[tokio::test]
    async fn tag_files_returns_matching_files_only() {
        let vault = vault_with_tags();
        let files = tag_files(vault.path().to_str().unwrap(), "rust")
            .await
            .unwrap();
        assert_eq!(files, vec!["a.md".to_string(), "c.md".to_string()]);
    }

    #[tokio::test]
    async fn tag_files_empty_for_unused_tag() {
        let vault = vault_with_tags();
        let files = tag_files(vault.path().to_str().unwrap(), "nonexistent")
            .await
            .unwrap();
        assert!(files.is_empty());
    }
}

#[cfg(test)]
mod scan_time_measurement {
    use super::*;
    use std::time::Instant;

    /// Not a correctness test — a one-off measurement for the issue's
    /// acceptance criterion ("measured backlinks/tags scan time on a ~1k-note
    /// vault recorded in a comment"). Run with `cargo test -p librarium-mobile
    /// -- --ignored --nocapture measure_scan_time_on_1k_notes`.
    #[tokio::test]
    #[ignore = "manual measurement, not a correctness check"]
    async fn measure_scan_time_on_1k_notes() {
        let vault = tempfile::TempDir::new().unwrap();
        for i in 0..1000 {
            let content = format!(
                "---\ntags: [note{}, shared]\n---\n# Note {i}\n\nSee [[Note {}]] for more. #inline{}\n",
                i % 50,
                (i + 1) % 1000,
                i % 20,
            );
            std::fs::write(vault.path().join(format!("note-{i:04}.md")), content).unwrap();
        }

        let start = Instant::now();
        let entries = tags_list(vault.path().to_str().unwrap()).await.unwrap();
        let tags_elapsed = start.elapsed();

        let start = Instant::now();
        let _ = crate::links::backlinks(vault.path().to_str().unwrap(), "note-0000.md")
            .await
            .unwrap();
        let backlinks_elapsed = start.elapsed();

        println!(
            "1000-note vault: tags_list = {:?} ({} tags), backlinks = {:?}",
            tags_elapsed,
            entries.len(),
            backlinks_elapsed
        );
    }
}
