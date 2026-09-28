//! Standalone frontmatter read/write, backed by
//! `librarium_core::frontmatter_service`. No REST equivalent exists — the
//! server only ever exposes frontmatter bundled inside a full file
//! read/write (`FileContent.frontmatter`, see `crate::file`). These commands
//! let a caller update just the frontmatter without resending the whole
//! note body, which matters more on mobile where re-sending a large note's
//! body for a one-field metadata edit is wasted work on a battery-powered
//! device.

use chrono::Utc;
use librarium_core::error::{AppError, AppResult};
use librarium_core::file_service::FileService;
use librarium_core::frontmatter_service;
use librarium_types::FileContent;

/// Frontmatter object for `file_path`, or `None` when the file has no
/// frontmatter block. Errors the same way `frontmatter_read` on a missing
/// file would — via `FileService::resolve_path`/read failing — rather than
/// silently returning `None`, so a caller can tell "no frontmatter" apart
/// from "no such file".
pub async fn frontmatter_read(
    vault_path: &str,
    file_path: &str,
) -> AppResult<Option<serde_json::Value>> {
    let vault_path = vault_path.to_string();
    let file_path = file_path.to_string();
    tokio::task::spawn_blocking(move || {
        let full_path = FileService::resolve_path(&vault_path, &file_path)?;
        if !full_path.is_file() {
            return Err(AppError::NotFound(format!("File not found: {file_path}")));
        }
        let raw = std::fs::read_to_string(&full_path)?;
        let (frontmatter, _body) = frontmatter_service::parse_note_frontmatter(&file_path, &raw)?;
        Ok(frontmatter)
    })
    .await
    .map_err(|e| AppError::InternalError(format!("frontmatter_read task join error: {e}")))?
}

/// Replaces `file_path`'s frontmatter with `frontmatter` (`None` strips it
/// entirely), leaving the body untouched. Returns the updated file the same
/// way `file_write` does, so a caller can refresh its view without a
/// separate `file_read`.
pub async fn frontmatter_write(
    vault_path: &str,
    file_path: &str,
    frontmatter: Option<serde_json::Value>,
) -> AppResult<FileContent> {
    let vault_path = vault_path.to_string();
    let file_path = file_path.to_string();
    tokio::task::spawn_blocking(move || {
        let full_path = FileService::resolve_path(&vault_path, &file_path)?;
        if !full_path.is_file() {
            return Err(AppError::NotFound(format!("File not found: {file_path}")));
        }
        let raw = std::fs::read_to_string(&full_path)?;
        let (_old_frontmatter, body) =
            frontmatter_service::parse_note_frontmatter(&file_path, &raw)?;
        let new_content = frontmatter_service::serialize_note_frontmatter(
            &file_path,
            frontmatter.as_ref(),
            &body,
        )?;
        std::fs::write(&full_path, &new_content)?;

        let modified = full_path
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos()))
            .unwrap_or_else(Utc::now);

        Ok(FileContent {
            path: file_path,
            content: body,
            modified,
            frontmatter,
        })
    })
    .await
    .map_err(|e| AppError::InternalError(format!("frontmatter_write task join error: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn frontmatter_read_returns_parsed_object() {
        let vault = TempDir::new().unwrap();
        std::fs::write(
            vault.path().join("note.md"),
            "---\ntitle: Hi\n---\nbody text",
        )
        .unwrap();

        let fm = frontmatter_read(vault.path().to_str().unwrap(), "note.md")
            .await
            .unwrap();
        assert_eq!(fm.unwrap()["title"], serde_json::json!("Hi"));
    }

    #[tokio::test]
    async fn frontmatter_read_none_when_absent() {
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("note.md"), "just body text").unwrap();

        let fm = frontmatter_read(vault.path().to_str().unwrap(), "note.md")
            .await
            .unwrap();
        assert!(fm.is_none());
    }

    #[tokio::test]
    async fn frontmatter_read_missing_file_is_not_found() {
        let vault = TempDir::new().unwrap();
        let err = frontmatter_read(vault.path().to_str().unwrap(), "nope.md")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn frontmatter_write_replaces_frontmatter_keeps_body() {
        let vault = TempDir::new().unwrap();
        std::fs::write(
            vault.path().join("note.md"),
            "---\ntitle: Old\n---\nbody unchanged",
        )
        .unwrap();

        let new_fm = serde_json::json!({"title": "New", "tags": ["a"]});
        let updated = frontmatter_write(
            vault.path().to_str().unwrap(),
            "note.md",
            Some(new_fm.clone()),
        )
        .await
        .unwrap();
        assert_eq!(updated.frontmatter, Some(new_fm));
        assert!(updated.content.contains("body unchanged"));

        let on_disk = std::fs::read_to_string(vault.path().join("note.md")).unwrap();
        assert!(on_disk.contains("title: New"));
        assert!(on_disk.contains("body unchanged"));
    }

    #[tokio::test]
    async fn frontmatter_write_none_strips_frontmatter() {
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("note.md"), "---\ntitle: Old\n---\nbody").unwrap();

        let updated = frontmatter_write(vault.path().to_str().unwrap(), "note.md", None)
            .await
            .unwrap();
        assert_eq!(updated.frontmatter, None);
        let on_disk = std::fs::read_to_string(vault.path().join("note.md")).unwrap();
        assert!(!on_disk.contains("---"));
    }

    #[tokio::test]
    async fn frontmatter_write_rejects_path_traversal() {
        let vault = TempDir::new().unwrap();
        let err = frontmatter_write(
            vault.path().to_str().unwrap(),
            "../outside.md",
            Some(serde_json::json!({"a": 1})),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }
}
