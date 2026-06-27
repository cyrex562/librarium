//! Vault-wide organization plan (LIB-063).
//!
//! Produces a reviewable [`OrganizationPlan`] of per-note proposals — suggested
//! tags, a canonical name, and a target folder — without mutating anything.
//! Application is a separate, explicit step (LIB-064).
//!
//! When the vault has cached note embeddings (Tier 2), notes are grouped by a
//! cosine-threshold clusterer and each cluster is labelled from its top terms
//! (c-TF-IDF), turning clusters into folders. Otherwise a Tier-1 TF-IDF
//! nearest-folder placement is used. The clusterer and labeller are pure and
//! unit-tested directly.

use crate::config::MlConfig;
use crate::db::Database;
use crate::error::AppResult;
use crate::models::{
    FolderCandidate, OrganizationPlan, OrganizationPlanRow, OrganizationSuggestionKind,
};
use crate::services::embedding_service::{blob_to_vector, cosine_similarity};
use crate::services::{frontmatter_service, FileService, MlService};
use std::collections::HashMap;

/// Notes whose cosine similarity is at least this are grouped together.
const CLUSTER_THRESHOLD: f32 = 0.6;
/// Minimum number of embedded notes before clustering is attempted.
const MIN_CLUSTERED_NOTES: usize = 4;

struct NoteCtx {
    rel: String,
    folder: String,
    frontmatter: Option<serde_json::Value>,
    body: String,
    tokens: Vec<String>,
    vector: Option<Vec<f32>>,
}

/// Build a (non-mutating) organization plan for an entire vault.
pub async fn build_plan(
    db: &Database,
    config: &MlConfig,
    vault_id: &str,
    vault_path: &str,
    plan_id: String,
    generated_at: chrono::DateTime<chrono::Utc>,
    max_files: Option<usize>,
) -> AppResult<OrganizationPlan> {
    let vec_map: HashMap<String, Vec<f32>> = db
        .list_note_embeddings(vault_id)
        .await?
        .into_iter()
        .map(|(path, blob, _tags)| (path, blob_to_vector(&blob)))
        .collect();

    let mut files = FileService::list_markdown_files(vault_path)?;
    if let Some(limit) = max_files {
        files.truncate(limit);
    }

    let mut notes: Vec<NoteCtx> = Vec::with_capacity(files.len());
    for (rel, raw) in files {
        let rel = rel.trim_start_matches('/').to_string();
        let (frontmatter, body) = frontmatter_service::parse_frontmatter(&raw)?;
        let tokens = MlService::tokenize(&body);
        let vector = vec_map.get(&rel).cloned();
        notes.push(NoteCtx {
            folder: parent_dir(&rel),
            rel,
            frontmatter,
            body,
            tokens,
            vector,
        });
    }

    // Cluster the embedded subset, if there's enough of it.
    let embedded_idx: Vec<usize> = notes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.vector.is_some())
        .map(|(i, _)| i)
        .collect();

    let mut note_label: HashMap<usize, String> = HashMap::new();
    let mut cluster_count = 0usize;

    if embedded_idx.len() >= MIN_CLUSTERED_NOTES {
        let vectors: Vec<Vec<f32>> = embedded_idx
            .iter()
            .map(|&i| notes[i].vector.clone().unwrap())
            .collect();
        let clusters = cluster_by_threshold(&vectors, CLUSTER_THRESHOLD);
        cluster_count = clusters.iter().copied().max().map(|m| m + 1).unwrap_or(0);

        let cluster_tokens: Vec<Vec<String>> = embedded_idx
            .iter()
            .map(|&i| notes[i].tokens.clone())
            .collect();
        let labels = c_tf_idf_labels(&clusters, &cluster_tokens, 2);

        for (pos, &note_i) in embedded_idx.iter().enumerate() {
            if let Some(label) = labels.get(clusters[pos]) {
                if !label.is_empty() {
                    note_label.insert(note_i, label.clone());
                }
            }
        }
    }

    // Folder corpus (folder, content) used to rank existing folders by content
    // similarity, and the set of folders that already exist (for is_new).
    let tfidf_corpus: Vec<(String, String)> = notes
        .iter()
        .map(|n| (n.folder.clone(), n.body.clone()))
        .collect();
    let existing_folders: std::collections::HashSet<String> = notes
        .iter()
        .map(|n| n.folder.clone())
        .filter(|f| !f.is_empty())
        .collect();

    let scheme = config.naming_scheme.clone();
    let mut rows: Vec<OrganizationPlanRow> = Vec::with_capacity(notes.len());

    for (i, note) in notes.iter().enumerate() {
        let keyphrases = MlService::keyphrases_for_tier(&note.body, config.tier.as_str(), 8);

        // Suggested tags: reuse the single-note suggestion engine, top 3 tags.
        let suggestion_resp = MlService::suggest_organization(
            &note.rel,
            &note.body,
            note.frontmatter.as_ref(),
            &keyphrases,
            8,
        );
        let suggested_tags: Vec<String> = suggestion_resp
            .suggestions
            .iter()
            .filter(|s| matches!(s.kind, OrganizationSuggestionKind::Tag))
            .filter_map(|s| s.tag.clone())
            .take(3)
            .collect();

        let suggested_name = MlService::suggest_rename(
            &note.rel,
            &note.body,
            note.frontmatter.as_ref(),
            &keyphrases,
            &scheme,
        );

        // Folder candidates: existing folders ranked by content similarity
        // (preferred), plus one proposed new folder from the note's cluster
        // label (Tier 2) or its top suggested tag (Tier 1). Self-moves are
        // filtered out so a note is never told to move where it already is.
        let mut folder_candidates: Vec<FolderCandidate> =
            MlService::rank_folders_tfidf(&note.body, &tfidf_corpus, 3)
                .into_iter()
                .filter(|(folder, score)| {
                    *folder != note.folder && !folder.is_empty() && *score > 0.0
                })
                .map(|(folder, score)| FolderCandidate {
                    path: folder,
                    is_new: false,
                    confidence: score,
                })
                .collect();

        let new_folder = note_label
            .get(&i)
            .map(|label| slugify_folder(label))
            .or_else(|| suggested_tags.first().map(|t| slugify_folder(t)))
            .filter(|f| !f.is_empty() && *f != note.folder);

        if let Some(nf) = new_folder {
            if !folder_candidates.iter().any(|c| c.path == nf) {
                let is_new = !existing_folders.contains(&nf);
                let confidence = if note_label.contains_key(&i) { 0.7 } else { 0.55 };
                folder_candidates.push(FolderCandidate {
                    path: nf,
                    is_new,
                    confidence,
                });
            }
        }

        let cluster = note_label.get(&i).cloned();

        // Recommended target (policy D): prefer a confident existing folder;
        // otherwise fall back to the best candidate (the proposed new folder
        // when no existing folder is a good fit).
        let target_folder = folder_candidates
            .iter()
            .find(|c| !c.is_new && c.confidence >= config.min_confidence)
            .or_else(|| folder_candidates.first())
            .map(|c| c.path.clone());
        let confidence = target_folder
            .as_ref()
            .and_then(|t| folder_candidates.iter().find(|c| &c.path == t))
            .map(|c| c.confidence)
            .unwrap_or(0.5);

        rows.push(OrganizationPlanRow {
            file_path: note.rel.clone(),
            suggested_tags,
            suggested_name,
            target_folder,
            folder_candidates,
            cluster,
            confidence,
        });
    }

    Ok(OrganizationPlan {
        plan_id,
        vault_id: vault_id.to_string(),
        rows,
        cluster_count,
        generated_at,
    })
}

/// Single-link clustering by cosine threshold via union-find. Returns a
/// contiguous cluster id (`0..k`) per input vector; isolated vectors form their
/// own singleton clusters, so the cluster count is data-driven (variable).
pub fn cluster_by_threshold(vectors: &[Vec<f32>], threshold: f32) -> Vec<usize> {
    let n = vectors.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if cosine_similarity(&vectors[i], &vectors[j]) >= threshold {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    // Relabel roots to contiguous ids in first-seen order.
    let mut label_of: HashMap<usize, usize> = HashMap::new();
    let mut next = 0usize;
    (0..n)
        .map(|i| {
            let root = find(&mut parent, i);
            *label_of.entry(root).or_insert_with(|| {
                let id = next;
                next += 1;
                id
            })
        })
        .collect()
}

/// Label each cluster from its most distinctive terms using a class-based
/// TF-IDF (c-TF-IDF): term frequency within the cluster weighted by how few
/// clusters the term appears in. Returns a `top_k`-term, kebab-joined label per
/// cluster id.
pub fn c_tf_idf_labels(
    clusters: &[usize],
    docs_tokens: &[Vec<String>],
    top_k: usize,
) -> Vec<String> {
    let k = clusters.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    if k == 0 {
        return Vec::new();
    }

    // Per-cluster term frequencies.
    let mut cluster_tf: Vec<HashMap<String, f32>> = vec![HashMap::new(); k];
    for (doc_i, &cid) in clusters.iter().enumerate() {
        if let Some(tokens) = docs_tokens.get(doc_i) {
            let tf = &mut cluster_tf[cid];
            for tok in tokens {
                *tf.entry(tok.clone()).or_insert(0.0) += 1.0;
            }
        }
    }

    // Cluster frequency per term (how many clusters contain it).
    let mut cf: HashMap<String, f32> = HashMap::new();
    for tf in &cluster_tf {
        for tok in tf.keys() {
            *cf.entry(tok.clone()).or_insert(0.0) += 1.0;
        }
    }
    let k_f = k as f32;

    cluster_tf
        .iter()
        .map(|tf| {
            let mut scored: Vec<(&String, f32)> = tf
                .iter()
                .map(|(tok, &freq)| {
                    let idf = (k_f / cf.get(tok).copied().unwrap_or(1.0)).ln() + 1.0;
                    (tok, freq * idf)
                })
                .collect();
            scored.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(b.0))
            });
            scored
                .into_iter()
                .take(top_k)
                .map(|(t, _)| t.clone())
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect()
}

fn slugify_folder(label: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn parent_dir(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
        .trim_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clusters_separate_by_threshold() {
        // Two tight groups on orthogonal axes -> two clusters.
        let vectors = vec![
            vec![1.0, 0.0],
            vec![0.98, 0.02],
            vec![0.0, 1.0],
            vec![0.02, 0.98],
        ];
        let clusters = cluster_by_threshold(&vectors, 0.6);
        assert_eq!(clusters[0], clusters[1]);
        assert_eq!(clusters[2], clusters[3]);
        assert_ne!(clusters[0], clusters[2]);
    }

    #[test]
    fn isolated_vectors_form_singletons() {
        let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![-1.0, 0.0]];
        let clusters = cluster_by_threshold(&vectors, 0.9);
        // All mutually dissimilar -> 3 distinct clusters.
        let distinct: std::collections::HashSet<_> = clusters.iter().collect();
        assert_eq!(distinct.len(), 3);
    }

    #[test]
    fn c_tf_idf_labels_pick_distinctive_terms() {
        // Cluster 0 about rust, cluster 1 about bread; "note" is shared noise.
        let clusters = vec![0, 0, 1, 1];
        let docs = vec![
            vec!["rust".to_string(), "rust".to_string(), "note".to_string()],
            vec!["rust".to_string(), "async".to_string(), "note".to_string()],
            vec!["bread".to_string(), "bread".to_string(), "note".to_string()],
            vec!["bread".to_string(), "flour".to_string(), "note".to_string()],
        ];
        let labels = c_tf_idf_labels(&clusters, &docs, 1);
        assert_eq!(labels.len(), 2);
        // Shared "note" is downweighted; distinctive term wins.
        assert_eq!(labels[0], "rust");
        assert_eq!(labels[1], "bread");
    }

    #[test]
    fn slugify_folder_is_kebab() {
        assert_eq!(slugify_folder("Rust Async"), "rust-async");
        assert_eq!(slugify_folder("a / b"), "a-b");
    }
}
