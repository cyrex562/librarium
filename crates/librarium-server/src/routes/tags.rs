use crate::error::{AppError, AppResult};
use crate::models::{MlUndoReceipt, ReverseAction};
use crate::routes::vaults::AppState;
use crate::services::frontmatter_service;
use actix_web::{delete, get, web, HttpResponse};
use chrono::Utc;
use regex::Regex;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct TagEntry {
    pub tag: String,
    pub count: usize,
    pub files: Vec<String>,
}

/// GET /api/vaults/{vault_id}/tags
/// Scans all .md files and returns a list of tags with file counts.
#[get("/api/vaults/{vault_id}/tags")]
async fn list_tags(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let vault = state.db.get_vault(&vault_id).await?;

    // Map tag -> list of files containing that tag
    let mut tag_map: HashMap<String, Vec<String>> = HashMap::new();

    for entry in WalkDir::new(&vault.path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
        })
    {
        let rel_path = entry
            .path()
            .strip_prefix(&vault.path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");

        if let Ok(raw) = std::fs::read_to_string(entry.path()) {
            let (fm, body) =
                frontmatter_service::parse_frontmatter(&raw).unwrap_or((None, raw.clone()));
            let tags = frontmatter_service::extract_tags(fm.as_ref(), &body);
            for tag in tags {
                tag_map.entry(tag).or_default().push(rel_path.clone());
            }
        }
    }

    let mut entries: Vec<TagEntry> = tag_map
        .into_iter()
        .map(|(tag, mut files)| {
            files.sort();
            let count = files.len();
            TagEntry { tag, count, files }
        })
        .collect();

    entries.sort_by(|a, b| a.tag.to_lowercase().cmp(&b.tag.to_lowercase()));

    Ok(HttpResponse::Ok().json(entries))
}

/// GET /api/vaults/{vault_id}/backlinks?path=notes/hello.md
/// Returns all .md files that contain a wiki-link or markdown link pointing at the given path.
#[get("/api/vaults/{vault_id}/backlinks")]
async fn list_backlinks(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    query: web::Query<BacklinksQuery>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let vault = state.db.get_vault(&vault_id).await?;
    let target_path = query.path.trim();

    // Derive the stem (filename without extension) for wiki-link matching
    let stem = std::path::Path::new(target_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(target_path);

    // Patterns to search for
    let wiki_stem_lower = format!("[[{}]]", stem.to_lowercase());
    let path_lower = target_path.to_lowercase();
    let path_no_ext = target_path.trim_end_matches(".md").to_lowercase();

    #[derive(Serialize)]
    struct BacklinkEntry {
        path: String,
        title: String,
    }

    let mut results: Vec<BacklinkEntry> = Vec::new();

    for entry in WalkDir::new(&vault.path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
        })
    {
        let rel_path = entry
            .path()
            .strip_prefix(&vault.path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");

        // Don't include the file linking to itself
        if rel_path.to_lowercase() == path_lower {
            continue;
        }

        if let Ok(raw) = std::fs::read_to_string(entry.path()) {
            let lower = raw.to_lowercase();
            // Check for [[stem]] style wiki-link or path-based markdown link
            let found = lower.contains(&wiki_stem_lower)
                || lower.contains(&format!("({})", path_lower))
                || lower.contains(&format!("({})", path_no_ext));
            if found {
                let title = std::path::Path::new(&rel_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&rel_path)
                    .to_string();
                results.push(BacklinkEntry {
                    path: rel_path,
                    title,
                });
            }
        }
    }

    results.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(HttpResponse::Ok().json(results))
}

#[derive(serde::Deserialize)]
struct BacklinksQuery {
    path: String,
}

/// DELETE /api/vaults/{vault_id}/tags/{tag}
/// Remove a tag from every note in the vault — both the frontmatter `tags`
/// list and inline `#tag` occurrences. Returns the number of files changed.
/// The file watcher re-indexes the rewritten files in the background.
#[derive(serde::Deserialize)]
struct DeleteTagQuery {
    /// When true, report the files that would change without modifying anything.
    #[serde(default)]
    dry_run: bool,
}

#[delete("/api/vaults/{vault_id}/tags/{tag}")]
async fn delete_tag(
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
    query: web::Query<DeleteTagQuery>,
) -> AppResult<HttpResponse> {
    let (vault_id, tag) = path.into_inner();
    let vault = state.db.get_vault(&vault_id).await?;
    let tag = tag.trim().trim_start_matches('#').to_string();
    if tag.is_empty() {
        return Err(AppError::InvalidInput("Tag must not be empty".to_string()));
    }
    let dry_run = query.dry_run;

    // Require a start-of-text/whitespace boundary before `#` so we never touch
    // `page#section` URL fragments, `#define`-style content, etc. The full tag
    // token is captured so deleting `#foo` leaves `#foobar` alone.
    let inline_re = Regex::new(r"(^|\s)#([A-Za-z0-9_-]+)")
        .map_err(|e| AppError::InternalError(format!("tag regex error: {e}")))?;
    let mut files_modified = 0usize;
    // Vault-relative paths of files that changed (or would change, in dry-run).
    let mut affected: Vec<String> = Vec::new();
    // Shared undo group: each rewritten file gets a snapshot receipt so the
    // whole vault-wide delete can be reverted together.
    let group_id = Uuid::new_v4().to_string();

    for entry in WalkDir::new(&vault.path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
        })
    {
        let Ok(raw) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let (fm, body) =
            frontmatter_service::parse_frontmatter(&raw).unwrap_or((None, raw.clone()));

        if !frontmatter_service::extract_tags(fm.as_ref(), &body)
            .iter()
            .any(|t| t == &tag)
        {
            continue;
        }

        // Drop the tag from frontmatter `tags`, tracking whether it changed.
        let mut changed = false;
        let new_fm = fm.map(|mut v| {
            if remove_tag_from_frontmatter(&mut v, &tag) {
                changed = true;
            }
            v
        });

        // Drop exact inline `#tag` occurrences outside code (fenced + inline),
        // collapsing the leading whitespace and leaving other tags untouched.
        let new_body = remove_inline_tag(&body, &tag, &inline_re);
        if new_body != body {
            changed = true;
        }

        // Skip files where the tag only appeared inside a code block / URL
        // (false positives from the cheap pre-filter) — no rewrite, no churn.
        if !changed {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(&vault.path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .trim_start_matches(['/', '\\'])
            .replace('\\', "/");
        affected.push(rel.clone());

        if dry_run {
            continue;
        }

        let new_content =
            frontmatter_service::serialize_frontmatter(new_fm.as_ref(), &new_body)?;
        if std::fs::write(entry.path(), new_content).is_ok() {
            files_modified += 1;
            // Snapshot the pre-delete content so the operation can be undone.
            let receipt = MlUndoReceipt {
                receipt_id: Uuid::new_v4().to_string(),
                vault_id: vault_id.clone(),
                file_path: rel,
                description: format!("Delete tag '#{tag}'"),
                reverse_action: ReverseAction::RestoreContent { content: raw },
                applied_at: Utc::now(),
                group_id: Some(group_id.clone()),
            };
            let _ = state.db.save_ml_undo_receipt(&receipt).await;
        }
    }

    Ok(HttpResponse::Ok().json(json!({
        "tag": tag,
        "dry_run": dry_run,
        "count": affected.len(),
        "files_modified": files_modified,
        "files": affected,
        "group_id": (!dry_run && files_modified > 0).then(|| group_id.clone()),
    })))
}

/// Remove `tag` from a frontmatter object's `tags` field (array or scalar).
/// Returns whether anything was removed.
fn remove_tag_from_frontmatter(fm: &mut serde_json::Value, tag: &str) -> bool {
    let Some(obj) = fm.as_object_mut() else {
        return false;
    };
    match obj.get_mut("tags") {
        Some(serde_json::Value::Array(arr)) => {
            let before = arr.len();
            arr.retain(|v| v.as_str() != Some(tag));
            arr.len() != before
        }
        Some(serde_json::Value::String(s)) if s == tag => {
            obj.remove("tags");
            true
        }
        _ => false,
    }
}

/// Remove exact inline `#tag` occurrences from markdown `body`, leaving fenced
/// (``` / ~~~) and inline (`` `…` ``) code spans untouched. `re` must capture
/// the leading boundary in group 1 and the tag token in group 2.
fn remove_inline_tag(body: &str, tag: &str, re: &Regex) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_fence = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            out.push_str(line);
        } else if in_fence {
            out.push_str(line);
        } else {
            out.push_str(&strip_tag_outside_inline_code(line, tag, re));
        }
    }
    out
}

/// Apply the tag-stripping regex only to the parts of `line` that are NOT
/// inside a backtick inline-code span.
fn strip_tag_outside_inline_code(line: &str, tag: &str, re: &Regex) -> String {
    let mut result = String::with_capacity(line.len());
    let mut in_code = false;
    let mut segment = String::new();
    for ch in line.chars() {
        if ch == '`' {
            push_segment(&mut result, &segment, in_code, tag, re);
            segment.clear();
            result.push('`');
            in_code = !in_code;
        } else {
            segment.push(ch);
        }
    }
    push_segment(&mut result, &segment, in_code, tag, re);
    result
}

fn push_segment(out: &mut String, seg: &str, in_code: bool, tag: &str, re: &Regex) {
    if in_code {
        out.push_str(seg);
        return;
    }
    // Replacing the whole match (boundary + `#tag`) collapses the preceding
    // whitespace; non-matching tags (e.g. `#foobar` when deleting `#foo`) are
    // emitted unchanged.
    let replaced = re.replace_all(seg, |caps: &regex::Captures| {
        if &caps[2] == tag {
            String::new()
        } else {
            caps[0].to_string()
        }
    });
    out.push_str(&replaced);
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_tags)
        .service(list_backlinks)
        .service(delete_tag);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re() -> Regex {
        Regex::new(r"(^|\s)#([A-Za-z0-9_-]+)").unwrap()
    }

    #[test]
    fn strips_plain_inline_tag_and_collapses_space() {
        assert_eq!(remove_inline_tag("see #foo here", "foo", &re()), "see here");
    }

    #[test]
    fn leaves_longer_tag_untouched() {
        assert_eq!(
            remove_inline_tag("#foobar stays", "foo", &re()),
            "#foobar stays"
        );
    }

    #[test]
    fn ignores_url_fragment() {
        let s = "visit http://x/page#foo now";
        assert_eq!(remove_inline_tag(s, "foo", &re()), s);
    }

    #[test]
    fn skips_fenced_code_block() {
        let s = "before\n```\n#foo in code\n```\nafter #foo";
        let out = remove_inline_tag(s, "foo", &re());
        assert!(out.contains("#foo in code"), "fenced tag must survive: {out}");
        assert!(out.ends_with("after"), "inline tag after fence removed: {out}");
    }

    #[test]
    fn skips_inline_code_span() {
        let s = "use `#foo` literally";
        assert_eq!(remove_inline_tag(s, "foo", &re()), s);
    }

    #[test]
    fn keeps_other_tags() {
        let out = remove_inline_tag("#foo #bar", "foo", &re());
        assert!(out.contains("#bar"));
        assert!(!out.contains("#foo"));
    }

    #[test]
    fn frontmatter_removal_reports_change() {
        let mut v = serde_json::json!({ "tags": ["a", "foo", "b"], "title": "x" });
        assert!(remove_tag_from_frontmatter(&mut v, "foo"));
        assert_eq!(v["tags"], serde_json::json!(["a", "b"]));
        assert_eq!(v["title"], "x");

        let mut absent = serde_json::json!({ "tags": ["a"] });
        assert!(!remove_tag_from_frontmatter(&mut absent, "foo"));
    }
}
