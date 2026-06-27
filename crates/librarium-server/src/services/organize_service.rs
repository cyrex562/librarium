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
use crate::services::{frontmatter_service, EntityService, FileService, MlService};
use std::collections::HashMap;

/// Notes whose cosine similarity is at least this are grouped together into a
/// top-level folder.
const CLUSTER_THRESHOLD: f32 = 0.6;
/// Minimum number of embedded notes before clustering is attempted.
const MIN_CLUSTERED_NOTES: usize = 4;
/// Tighter cosine threshold used to split a top-level cluster into subfolders
/// (LIB-076). Higher than [`CLUSTER_THRESHOLD`] so subfolders are more cohesive.
const SUBCLUSTER_THRESHOLD: f32 = 0.78;
/// A top-level cluster is only split into subfolders when it has at least this
/// many notes — small clusters stay flat.
const MIN_SUBCLUSTER_PARENT: usize = 6;
/// A subfolder must hold at least this many notes; singletons stay in the parent.
const MIN_SUBFOLDER_SIZE: usize = 2;

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
        let top_labels = c_tf_idf_labels(&clusters, &cluster_tokens, 2);

        // Split large top-level clusters into nested `parent/child` folders.
        let labels = nested_cluster_labels(
            &clusters,
            &vectors,
            &cluster_tokens,
            &top_labels,
            SUBCLUSTER_THRESHOLD,
        );

        for (pos, &note_i) in embedded_idx.iter().enumerate() {
            if let Some(label) = labels.get(pos) {
                if !label.is_empty() {
                    note_label.insert(note_i, label.clone());
                }
            }
        }
    }

    // Map raw cluster term-labels onto a controlled vocabulary (LIB-077) so
    // folders get human, consistent names. The taxonomy is sourced from the
    // vault's own entity types + tags (deterministic, air-gap friendly) plus any
    // config-provided categories; unmatched labels are left untouched.
    let taxonomy = build_taxonomy(db, vault_id, config, &notes).await;
    if !taxonomy.is_empty() {
        for label in note_label.values_mut() {
            *label = canonicalize_label_path(label, &taxonomy);
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

    // Reinforcement signal (LIB-075): past accept/reject counts reweight and
    // demote folder/tag candidates. Empty when the vault has no history yet.
    let feedback = db.get_org_feedback_map(vault_id).await.unwrap_or_default();

    // Local-LM tier (LIB-074): optional zero-shot scorer that sharpens folder
    // ranking. `None` when the tier is off or no local model is present.
    let scorer = crate::services::local_lm_service::label_scorer(config);

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
        // Take a slightly larger pool, then let reinforcement reorder/demote it
        // before trimming to the top 3 (LIB-075).
        let mut suggested_tags: Vec<String> = suggestion_resp
            .suggestions
            .iter()
            .filter(|s| matches!(s.kind, OrganizationSuggestionKind::Tag))
            .filter_map(|s| s.tag.clone())
            .take(6)
            .collect();
        suggested_tags.retain(|t| !is_demoted(&feedback, "tag", &tag_key(t)));
        // Stable sort keeps the original ranking for equal (neutral) weights.
        suggested_tags.sort_by(|a, b| {
            feedback_multiplier(&feedback, "tag", &tag_key(b))
                .partial_cmp(&feedback_multiplier(&feedback, "tag", &tag_key(a)))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        suggested_tags.truncate(3);

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
            .map(|label| slugify_folder_path(label))
            .or_else(|| suggested_tags.first().map(|t| slugify_folder_path(t)))
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

        // Reinforcement (LIB-075): scale each candidate's confidence by the
        // folder's historical acceptance rate, drop folders the user has
        // repeatedly rejected (never emptying the list), then re-rank.
        for c in &mut folder_candidates {
            c.confidence *= feedback_multiplier(&feedback, "folder", &c.path);
        }
        if folder_candidates
            .iter()
            .any(|c| !is_demoted(&feedback, "folder", &c.path))
        {
            folder_candidates.retain(|c| !is_demoted(&feedback, "folder", &c.path));
        }
        folder_candidates.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Local-LM refinement (LIB-074): blend zero-shot fit scores into the
        // candidate confidences, then re-rank. No-op when no scorer is active.
        if let Some(scorer) = &scorer {
            if !folder_candidates.is_empty() {
                let labels: Vec<String> =
                    folder_candidates.iter().map(|c| c.path.clone()).collect();
                let scores = scorer.score(&note.body, &labels);
                blend_label_scores(&mut folder_candidates, &scores);
                folder_candidates.sort_by(|a, b| {
                    b.confidence
                        .partial_cmp(&a.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
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

/// Build two-level folder labels for a set of embedded notes (LIB-076). Notes
/// are first grouped into top-level clusters (`clusters`/`top_labels`); each
/// sufficiently large top-level cluster is then sub-clustered at the tighter
/// `sub_threshold`, and each cohesive subgroup labelled (via c-TF-IDF over the
/// subgroup, where the parent's shared term is naturally down-weighted) to form
/// a nested `parent/child` folder. Returns one label per input note, in the
/// same order as `clusters`/`vectors`/`docs_tokens`. A note whose subgroup is
/// too small, unlabelled, or redundant with its parent keeps just the parent
/// label, so the result is never *less* organized than the flat clustering.
pub fn nested_cluster_labels(
    clusters: &[usize],
    vectors: &[Vec<f32>],
    docs_tokens: &[Vec<String>],
    top_labels: &[String],
    sub_threshold: f32,
) -> Vec<String> {
    let n = clusters.len();
    let mut out: Vec<String> = (0..n)
        .map(|i| top_labels.get(clusters[i]).cloned().unwrap_or_default())
        .collect();

    let k = clusters.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    for c in 0..k {
        let members: Vec<usize> = (0..n).filter(|&i| clusters[i] == c).collect();
        if members.len() < MIN_SUBCLUSTER_PARENT {
            continue;
        }
        let parent = match top_labels.get(c) {
            Some(p) if !p.is_empty() => p.clone(),
            _ => continue,
        };

        let sub_vectors: Vec<Vec<f32>> = members.iter().map(|&i| vectors[i].clone()).collect();
        let sub_clusters = cluster_by_threshold(&sub_vectors, sub_threshold);
        let sub_k = sub_clusters.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        if sub_k < 2 {
            continue; // the cluster did not split — keep it flat
        }

        let mut sub_size = vec![0usize; sub_k];
        for &sc in &sub_clusters {
            sub_size[sc] += 1;
        }

        let sub_tokens: Vec<Vec<String>> =
            members.iter().map(|&i| docs_tokens[i].clone()).collect();
        let sub_labels = c_tf_idf_labels(&sub_clusters, &sub_tokens, 2);
        let parent_slug = slugify_folder(&parent);

        for (pos, &note_i) in members.iter().enumerate() {
            let sc = sub_clusters[pos];
            if sub_size[sc] < MIN_SUBFOLDER_SIZE {
                continue;
            }
            let child = match sub_labels.get(sc) {
                Some(ch) if !ch.is_empty() => ch,
                _ => continue,
            };
            // A child that just repeats the parent term adds no structure.
            if slugify_folder(child) == parent_slug {
                continue;
            }
            out[note_i] = format!("{parent}/{child}");
        }
    }

    out
}

/// Accumulated organize feedback keyed by `(kind, target)` -> `(accepts, rejects)`.
type FeedbackMap = HashMap<(String, String), (i64, i64)>;

/// Blend local-LM zero-shot label scores (each in `[0, 1]`) into candidate
/// confidences as a `0.5`–`1.5` multiplier (LIB-074): a confident model boosts a
/// fitting folder and demotes a poorly-matching one without erasing the base
/// signal. Candidates with no corresponding score are left untouched.
fn blend_label_scores(candidates: &mut [FolderCandidate], scores: &[f32]) {
    for (c, &s) in candidates.iter_mut().zip(scores.iter()) {
        c.confidence *= 0.5 + s.clamp(0.0, 1.0);
    }
}

/// Canonical reinforcement key for a tag: trimmed, no leading `#`, lowercased —
/// matching how the apply/undo routes record signals.
fn tag_key(tag: &str) -> String {
    tag.trim().trim_start_matches('#').to_lowercase()
}

/// Reinforcement multiplier (LIB-075) for a candidate target. Laplace-smoothed
/// acceptance rate scaled so a target with no history stays neutral (1.0);
/// steady acceptance approaches 2.0 and steady rejection approaches 0.0.
fn feedback_multiplier(feedback: &FeedbackMap, kind: &str, target: &str) -> f32 {
    match feedback.get(&(kind.to_string(), target.to_string())) {
        Some(&(a, r)) => (a as f32 + 1.0) / (a as f32 + r as f32 + 2.0) * 2.0,
        None => 1.0,
    }
}

/// Whether a target has been rejected enough (and accepted little enough) to be
/// dropped from suggestions outright.
fn is_demoted(feedback: &FeedbackMap, kind: &str, target: &str) -> bool {
    match feedback.get(&(kind.to_string(), target.to_string())) {
        Some(&(a, r)) => r >= 3 && (a as f32 + 1.0) / (a as f32 + r as f32 + 2.0) < 0.35,
        None => false,
    }
}

/// A controlled-vocabulary folder category (LIB-077): a human display name and
/// the lowercased keyword tokens that route a cluster's terms to it.
struct TaxonomyCategory {
    name: String,
    keywords: Vec<String>,
}

/// Assemble the folder taxonomy from (in order) config-provided categories, the
/// vault's `librarium_type` entity types, and the tags already present on notes.
/// Deduplicated by slug and sorted by name so canonicalization is deterministic.
/// Fully offline — no model is consulted.
async fn build_taxonomy(
    db: &Database,
    vault_id: &str,
    config: &MlConfig,
    notes: &[NoteCtx],
) -> Vec<TaxonomyCategory> {
    let mut names: Vec<String> = config.folder_taxonomy.clone();

    if let Ok(entities) = EntityService::list_all_in_vault(db, vault_id).await {
        for e in entities {
            names.push(e.entity_type);
        }
    }
    for n in notes {
        for tag in frontmatter_service::extract_tags(n.frontmatter.as_ref(), &n.body) {
            names.push(tag);
        }
    }

    let mut by_slug: HashMap<String, TaxonomyCategory> = HashMap::new();
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        let slug = slugify_folder(trimmed);
        if slug.is_empty() {
            continue;
        }
        let keywords = MlService::tokenize(trimmed);
        if keywords.is_empty() {
            continue;
        }
        by_slug.entry(slug).or_insert_with(|| TaxonomyCategory {
            name: trimmed.to_string(),
            keywords,
        });
    }

    let mut cats: Vec<TaxonomyCategory> = by_slug.into_values().collect();
    cats.sort_by(|a, b| a.name.cmp(&b.name));
    cats
}

/// True when two terms should be considered the same vocabulary token: an exact
/// match, or one is a prefix of the other (cheap stemming, min length 4 to avoid
/// short-token noise like `cat` ~ `category`).
fn term_matches(term: &str, keyword: &str) -> bool {
    if term == keyword {
        return true;
    }
    let shared = term.len().min(keyword.len());
    shared >= 4 && (term.starts_with(keyword) || keyword.starts_with(term))
}

/// Map one raw cluster term-label (e.g. `rust-async`) to the controlled-
/// vocabulary category it best overlaps, returning that category's slug. Falls
/// back to the raw label unchanged when nothing matches.
fn canonicalize_label(raw: &str, taxonomy: &[TaxonomyCategory]) -> String {
    let terms: Vec<&str> = raw.split('-').filter(|t| !t.is_empty()).collect();
    if terms.is_empty() || taxonomy.is_empty() {
        return raw.to_string();
    }

    let mut best: Option<(usize, &TaxonomyCategory)> = None;
    for cat in taxonomy {
        let score = cat
            .keywords
            .iter()
            .filter(|kw| terms.iter().any(|t| term_matches(t, kw)))
            .count();
        if score == 0 {
            continue;
        }
        if best.map(|(bs, _)| score > bs).unwrap_or(true) {
            best = Some((score, cat));
        }
    }

    match best {
        Some((_, cat)) => slugify_folder(&cat.name),
        None => raw.to_string(),
    }
}

/// Canonicalize each `/`-separated segment of a (possibly nested) folder label
/// against the taxonomy, dropping a child that collapses to the same category as
/// its parent so we never emit `programming/programming`.
fn canonicalize_label_path(label: &str, taxonomy: &[TaxonomyCategory]) -> String {
    let mut segments: Vec<String> = Vec::new();
    for seg in label.split('/') {
        if seg.is_empty() {
            continue;
        }
        let canon = canonicalize_label(seg, taxonomy);
        if segments.last().map(|p| *p == canon).unwrap_or(false) {
            continue;
        }
        segments.push(canon);
    }
    segments.join("/")
}

/// Slugify a single folder-name segment: lowercase alphanumerics, every other
/// run collapsed to a single `-`. `/` is treated as an ordinary separator (so it
/// collapses to `-`) — use [`slugify_folder_path`] to preserve nesting.
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

/// Slugify a (possibly nested) folder path, preserving `/` boundaries by
/// slugifying each segment independently. Empty segments are dropped.
fn slugify_folder_path(label: &str) -> String {
    label
        .split('/')
        .map(slugify_folder)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
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

    #[test]
    fn slugify_folder_path_preserves_nesting() {
        assert_eq!(slugify_folder_path("Rust Async"), "rust-async");
        assert_eq!(slugify_folder_path("Tech/Rust Async"), "tech/rust-async");
        // Empty / noise segments are dropped, not left as `//`.
        assert_eq!(slugify_folder_path("tech//  /web"), "tech/web");
    }

    // Two tight bunches (A near 0°, B near 45°): every cross pair is in
    // [0.6, 0.78) so they merge at the top level but split into subfolders.
    fn two_bunch_vectors() -> Vec<Vec<f32>> {
        fn dir(deg: f32) -> Vec<f32> {
            let r = deg.to_radians();
            vec![r.cos(), r.sin()]
        }
        vec![
            dir(-1.0),
            dir(0.0),
            dir(1.0), // A
            dir(44.0),
            dir(45.0),
            dir(46.0), // B
        ]
    }

    #[test]
    fn nested_labels_split_large_cluster_into_subfolders() {
        let vectors = two_bunch_vectors();
        let clusters = vec![0, 0, 0, 0, 0, 0];
        let docs = vec![
            vec!["rust".to_string(), "rust".to_string()],
            vec!["rust".to_string(), "async".to_string()],
            vec!["rust".to_string(), "trait".to_string()],
            vec!["bread".to_string(), "bread".to_string()],
            vec!["bread".to_string(), "flour".to_string()],
            vec!["bread".to_string(), "yeast".to_string()],
        ];
        let top_labels = vec!["tech".to_string()];
        let out = nested_cluster_labels(&clusters, &vectors, &docs, &top_labels, 0.78);

        // A members nest under one subfolder, B members under another; both keep
        // the shared `tech` parent.
        assert!(out[0].starts_with("tech/"), "got {}", out[0]);
        assert!(out[3].starts_with("tech/"), "got {}", out[3]);
        assert_eq!(out[0], out[1]);
        assert_eq!(out[1], out[2]);
        assert_eq!(out[3], out[4]);
        assert_eq!(out[4], out[5]);
        assert_ne!(out[0], out[3]);
    }

    fn taxo(pairs: &[(&str, &[&str])]) -> Vec<TaxonomyCategory> {
        pairs
            .iter()
            .map(|(name, kws)| TaxonomyCategory {
                name: name.to_string(),
                keywords: kws.iter().map(|s| s.to_string()).collect(),
            })
            .collect()
    }

    #[test]
    fn canonicalize_maps_terms_to_category() {
        let tx = taxo(&[
            ("Programming", &["rust", "python", "code"]),
            ("Cooking", &["bread", "flour"]),
        ]);
        assert_eq!(canonicalize_label("rust-async", &tx), "programming");
        assert_eq!(canonicalize_label("bread-sourdough", &tx), "cooking");
        // Prefix stemming: `projects` keyword matches a `project` term.
        let tx2 = taxo(&[("Projects", &["projects"])]);
        assert_eq!(canonicalize_label("project-plan", &tx2), "projects");
        // No overlap -> raw label is preserved.
        assert_eq!(canonicalize_label("zzz-qqq", &tx), "zzz-qqq");
    }

    #[test]
    fn canonicalize_path_dedups_and_preserves_nesting() {
        let tx = taxo(&[
            ("Programming", &["rust", "async"]),
            ("Web", &["http", "web"]),
        ]);
        assert_eq!(
            canonicalize_label_path("rust-async/web-http", &tx),
            "programming/web"
        );
        // Both segments collapse to the same category -> single segment.
        assert_eq!(canonicalize_label_path("rust/async", &tx), "programming");
    }

    #[test]
    fn blend_label_scores_reranks_by_fit() {
        let mut cands = vec![
            FolderCandidate {
                path: "archive".into(),
                is_new: false,
                confidence: 1.0,
            },
            FolderCandidate {
                path: "rust".into(),
                is_new: false,
                confidence: 0.9,
            },
        ];
        // The model strongly prefers the second candidate despite its lower base.
        blend_label_scores(&mut cands, &[0.1, 1.0]);
        assert!(cands[0].confidence < cands[1].confidence);
        // A missing score leaves that candidate untouched.
        let mut one = vec![FolderCandidate {
            path: "x".into(),
            is_new: false,
            confidence: 2.0,
        }];
        blend_label_scores(&mut one, &[]);
        assert_eq!(one[0].confidence, 2.0);
    }

    #[test]
    fn feedback_multiplier_and_demotion() {
        let mut fb: FeedbackMap = HashMap::new();
        fb.insert(("folder".into(), "good".into()), (8, 0));
        fb.insert(("folder".into(), "bad".into()), (0, 6));
        fb.insert(("folder".into(), "mixed".into()), (3, 3));

        // No history -> neutral.
        assert!((feedback_multiplier(&fb, "folder", "unknown") - 1.0).abs() < 1e-6);
        // Heavily accepted -> boosted above 1, heavily rejected -> below 1.
        assert!(feedback_multiplier(&fb, "folder", "good") > 1.5);
        assert!(feedback_multiplier(&fb, "folder", "bad") < 0.5);
        // Apply-then-undo style mix nets to roughly neutral.
        assert!((feedback_multiplier(&fb, "folder", "mixed") - 1.0).abs() < 1e-6);

        // Demotion only fires on repeated rejection with low acceptance.
        assert!(is_demoted(&fb, "folder", "bad"));
        assert!(!is_demoted(&fb, "folder", "good"));
        assert!(!is_demoted(&fb, "folder", "mixed"));
        assert!(!is_demoted(&fb, "folder", "unknown"));
    }

    #[test]
    fn nested_labels_keep_small_clusters_flat() {
        // Only 3 notes in the cluster: below MIN_SUBCLUSTER_PARENT, stays flat.
        let vectors = vec![vec![1.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]];
        let clusters = vec![0, 0, 0];
        let docs = vec![
            vec!["rust".to_string()],
            vec!["rust".to_string()],
            vec!["bread".to_string()],
        ];
        let top_labels = vec!["tech".to_string()];
        let out = nested_cluster_labels(&clusters, &vectors, &docs, &top_labels, 0.78);
        assert_eq!(out, vec!["tech", "tech", "tech"]);
    }
}
