//! Local sentence embeddings (Tier 2 of the organization feature).
//!
//! The actual embedding backend (`fastembed`, ONNX runtime) is heavy and pulls a
//! native dependency, so it lives behind the off-by-default `embeddings` Cargo
//! feature. Everything in this module is written against the [`Embedder`] trait
//! and a process-wide provider that returns `None` when the backend is not
//! available — so the default build compiles, tests, and runs fully offline, and
//! all semantic features degrade gracefully to Tier 1.
//!
//! See `docs/ORGANIZATION_ML_PLAN.md`.

use crate::config::MlConfig;
use crate::error::AppResult;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

/// A text → vector embedding backend. Implementations must produce
/// deterministic, fixed-dimension vectors for a given model.
pub trait Embedder: Send + Sync {
    /// Identifier of the underlying model (e.g. `bge-small-en-v1.5`).
    fn model_name(&self) -> &str;
    /// Output vector dimensionality.
    fn dim(&self) -> usize;
    /// Embed a batch of texts. Returns one vector per input, in order.
    fn embed(&self, texts: &[String]) -> AppResult<Vec<Vec<f32>>>;
}

/// Process-wide embedder, initialized once from the active config. `None` means
/// the embeddings tier is inactive, the backend was not compiled in, or the
/// model could not be loaded.
static EMBEDDER: OnceLock<Option<Arc<dyn Embedder>>> = OnceLock::new();

/// Return the shared embedder, lazily initializing it from `config` on first
/// call. Callers must treat `None` as "fall back to Tier 1".
pub fn embedder(config: &MlConfig) -> Option<Arc<dyn Embedder>> {
    EMBEDDER.get_or_init(|| init_embedder(config)).clone()
}

/// Return the shared embedder only if it has already been initialized (e.g.
/// primed at startup via [`embedder`]). Used on paths that lack a config handle,
/// such as the file-watcher reindex loop. Returns `None` when not yet primed or
/// when the backend is unavailable.
pub fn embedder_if_ready() -> Option<Arc<dyn Embedder>> {
    EMBEDDER.get().cloned().flatten()
}

#[cfg(test)]
/// Install a specific embedder for tests. Returns `false` if one was already
/// initialized (the `OnceLock` is process-global).
pub fn set_embedder_for_test(e: Option<Arc<dyn Embedder>>) -> bool {
    EMBEDDER.set(e).is_ok()
}

fn init_embedder(config: &MlConfig) -> Option<Arc<dyn Embedder>> {
    if !config.enabled || !config.tier.uses_embeddings() {
        return None;
    }

    #[cfg(feature = "embeddings")]
    {
        match fastembed_backend::FastEmbedder::load(config) {
            Ok(e) => {
                tracing::info!(model = %e.model_name(), "ML embeddings backend ready");
                Some(Arc::new(e) as Arc<dyn Embedder>)
            }
            Err(e) => {
                tracing::warn!(
                    "ML embeddings unavailable ({e}); falling back to Tier 1 (classical)"
                );
                None
            }
        }
    }

    #[cfg(not(feature = "embeddings"))]
    {
        let _ = config;
        tracing::warn!(
            "ML tier is 'embeddings' but this server was built without the `embeddings` \
             feature; falling back to Tier 1 (classical). Rebuild with \
             `--features embeddings` to enable local semantic features."
        );
        None
    }
}

/// Build the text fed to the embedder for a note: the note path's stem as a
/// lightweight title hint, followed by a bounded slice of the body. bge-style
/// models cap around 512 tokens, so we truncate to keep latency predictable.
pub fn embedding_input(file_path: &str, content: &str) -> String {
    const MAX_CHARS: usize = 2000;
    let stem = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut text = String::with_capacity(MAX_CHARS + stem.len() + 1);
    if !stem.is_empty() {
        text.push_str(stem);
        text.push('\n');
    }
    for ch in content.chars() {
        if text.len() >= MAX_CHARS {
            break;
        }
        text.push(ch);
    }
    text
}

/// Stable content hash used to skip recompute when a note is unchanged. Keyed on
/// the embedding input plus the model id (so swapping models forces recompute).
pub fn content_hash(model: &str, input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update([0u8]);
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Serialize a vector to little-endian f32 bytes for BLOB storage.
pub fn vector_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Deserialize a little-endian f32 BLOB back into a vector. Trailing bytes that
/// do not form a full f32 are ignored.
pub fn blob_to_vector(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Cosine similarity in [-1, 1]. Returns 0 if either vector is zero/empty or the
/// dimensions differ.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// A vault note paired with its embedding and its tags, used to build
/// controlled-vocabulary prototypes.
pub struct TaggedNote {
    pub vector: Vec<f32>,
    pub tags: Vec<String>,
}

/// Build a prototype vector per tag = the mean (then L2-normalized) embedding of
/// all notes carrying that tag. Tags are matched case-insensitively and returned
/// in their normalized lowercase form.
pub fn build_tag_prototypes(notes: &[TaggedNote]) -> HashMap<String, Vec<f32>> {
    let mut sums: HashMap<String, (Vec<f32>, usize)> = HashMap::new();
    for note in notes {
        if note.vector.is_empty() {
            continue;
        }
        for tag in &note.tags {
            let key = tag.trim().trim_start_matches('#').to_lowercase();
            if key.is_empty() {
                continue;
            }
            let entry = sums
                .entry(key)
                .or_insert_with(|| (vec![0.0; note.vector.len()], 0));
            if entry.0.len() != note.vector.len() {
                // Mixed dimensions (e.g. model change mid-vault); skip the odd one.
                continue;
            }
            for (acc, v) in entry.0.iter_mut().zip(&note.vector) {
                *acc += v;
            }
            entry.1 += 1;
        }
    }

    sums.into_iter()
        .filter_map(|(tag, (mut sum, count))| {
            if count == 0 {
                return None;
            }
            let inv = 1.0 / count as f32;
            for v in &mut sum {
                *v *= inv;
            }
            normalize(&mut sum);
            Some((tag, sum))
        })
        .collect()
}

/// Nearest tags to `target` whose cosine similarity meets `min_confidence`,
/// excluding any already in `existing` (case-insensitive). Sorted by descending
/// similarity and capped at `max`.
pub fn semantic_tag_suggestions(
    target: &[f32],
    prototypes: &HashMap<String, Vec<f32>>,
    existing: &HashSet<String>,
    min_confidence: f32,
    max: usize,
) -> Vec<(String, f32)> {
    let mut scored: Vec<(String, f32)> = prototypes
        .iter()
        .filter(|(tag, _)| !existing.contains(*tag))
        .map(|(tag, proto)| (tag.clone(), cosine_similarity(target, proto)))
        .filter(|(_, score)| *score >= min_confidence)
        .collect();

    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    scored.truncate(max);
    scored
}

fn normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

// ── Incremental compute + storage (LIB-060) ───────────────────────────────────

use crate::db::Database;
use crate::error::AppError;
use crate::services::frontmatter_service;

/// Compute and store the embedding for one note, skipping work when the content
/// hash is unchanged. No-op (returns `Ok`) when no embedder is ready, so it is
/// safe to call unconditionally from the reindex/watcher path. `raw_content` is
/// the full file text (frontmatter included).
pub async fn embed_note(
    db: &Database,
    vault_id: &str,
    rel_path: &str,
    raw_content: &str,
) -> AppResult<()> {
    let Some(emb) = embedder_if_ready() else {
        return Ok(());
    };

    let (frontmatter, body) = frontmatter_service::parse_frontmatter(raw_content)?;
    let input = embedding_input(rel_path, &body);
    let hash = content_hash(emb.model_name(), &input);

    if db.get_note_embedding_hash(vault_id, rel_path).await? == Some(hash.clone()) {
        return Ok(()); // unchanged since last embed
    }

    let vector = embed_one(&emb, input).await?;
    store_embedding(db, &emb, vault_id, rel_path, &vector, &hash, frontmatter.as_ref(), &body)
        .await
}

/// Embed every markdown note in a vault that is missing or stale, in bounded
/// batches. No-op when no embedder is ready. Returns the number of notes
/// (re)embedded.
pub async fn backfill_vault(db: &Database, vault_id: &str, vault_path: &str) -> AppResult<usize> {
    let Some(emb) = embedder_if_ready() else {
        return Ok(0);
    };

    let files = crate::services::FileService::list_markdown_files(vault_path)?;
    const BATCH: usize = 32;
    let mut embedded = 0usize;

    let mut pending: Vec<(String, String, String, Option<serde_json::Value>, String)> = Vec::new();
    for (rel, raw) in files {
        let rel = rel.trim_start_matches('/').to_string();
        let (frontmatter, body) = frontmatter_service::parse_frontmatter(&raw)?;
        let input = embedding_input(&rel, &body);
        let hash = content_hash(emb.model_name(), &input);
        if db.get_note_embedding_hash(vault_id, &rel).await? == Some(hash.clone()) {
            continue;
        }
        pending.push((rel, input, hash, frontmatter, body));

        if pending.len() >= BATCH {
            embedded += flush_batch(db, &emb, vault_id, std::mem::take(&mut pending)).await?;
        }
    }
    if !pending.is_empty() {
        embedded += flush_batch(db, &emb, vault_id, pending).await?;
    }
    Ok(embedded)
}

/// Remove a note's cached embedding (called when a file is deleted).
pub async fn remove_note(db: &Database, vault_id: &str, rel_path: &str) -> AppResult<()> {
    db.delete_note_embedding(vault_id, rel_path).await
}

/// Controlled-vocabulary semantic tag suggestions (LIB-061): build prototype
/// vectors from the vault's existing tag→note assignments and return the nearest
/// tags to the target note above `config.min_confidence`, excluding `existing`.
///
/// Returns an empty vec (not an error) when the embedder is unavailable or the
/// vault has no cached embeddings yet, so callers fall back to Tier 1 cleanly.
pub async fn suggest_semantic_tags(
    db: &Database,
    config: &MlConfig,
    vault_id: &str,
    file_path: &str,
    body: &str,
    existing: &HashSet<String>,
    max: usize,
) -> AppResult<Vec<(String, f32)>> {
    let Some(emb) = embedder(config) else {
        return Ok(Vec::new());
    };

    let rows = db.list_note_embeddings(vault_id).await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let notes: Vec<TaggedNote> = rows
        .into_iter()
        .map(|(_, blob, tags_json)| TaggedNote {
            vector: blob_to_vector(&blob),
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        })
        .collect();

    let prototypes = build_tag_prototypes(&notes);
    if prototypes.is_empty() {
        return Ok(Vec::new());
    }

    let input = embedding_input(file_path, body);
    let target = embed_one(&emb, input).await?;
    Ok(semantic_tag_suggestions(
        &target,
        &prototypes,
        existing,
        config.min_confidence,
        max,
    ))
}

/// Tier-2 semantic folder placement (LIB-062): embed the target note, find its
/// nearest neighbours among the vault's cached embeddings, and vote on the
/// folder those neighbours live in (weighted by cosine similarity). Returns the
/// winning folder and its vote share, or `None` when the embedder is
/// unavailable, there is too little signal, or the winner is the note's current
/// folder / below `config.min_confidence`.
pub async fn suggest_folder(
    db: &Database,
    config: &MlConfig,
    vault_id: &str,
    file_path: &str,
    body: &str,
) -> AppResult<Option<(String, f32)>> {
    const K: usize = 10;

    let Some(emb) = embedder(config) else {
        return Ok(None);
    };

    let rows = db.list_note_embeddings(vault_id).await?;
    // Need a few neighbours other than the note itself for a meaningful vote.
    if rows.len() < 4 {
        return Ok(None);
    }

    let input = embedding_input(file_path, body);
    let target = embed_one(&emb, input).await?;

    let neighbours: Vec<(String, Vec<f32>)> = rows
        .iter()
        .filter(|(p, _, _)| p != file_path)
        .map(|(p, blob, _)| (parent_dir(p), blob_to_vector(blob)))
        .collect();

    Ok(knn_folder_vote(
        &target,
        &neighbours,
        K,
        config.min_confidence,
        &parent_dir(file_path),
    ))
}

/// Pure kNN folder vote: rank `neighbours` by cosine similarity to `target`,
/// keep the top `k`, and pick the folder with the highest similarity-weighted
/// vote share. Returns `None` when the winner is `current` or its share is below
/// `min_confidence`.
pub(crate) fn knn_folder_vote(
    target: &[f32],
    neighbours: &[(String, Vec<f32>)],
    k: usize,
    min_confidence: f32,
    current: &str,
) -> Option<(String, f32)> {
    let mut scored: Vec<(&str, f32)> = neighbours
        .iter()
        .map(|(folder, vec)| (folder.as_str(), cosine_similarity(target, vec)))
        .filter(|(_, sim)| *sim > 0.0)
        .collect();
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);

    let total: f32 = scored.iter().map(|(_, s)| s).sum();
    if total <= 0.0 {
        return None;
    }

    let mut folder_score: HashMap<String, f32> = HashMap::new();
    for (folder, sim) in &scored {
        *folder_score.entry((*folder).to_string()).or_insert(0.0) += sim;
    }

    let (best_folder, score) = folder_score
        .into_iter()
        .max_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.0.cmp(&a.0))
        })
        .unwrap();

    let confidence = score / total;
    if best_folder == current || confidence < min_confidence {
        return None;
    }
    Some((best_folder, confidence))
}

/// Vault-relative parent folder of a path (forward slashes, no trailing slash;
/// empty string for the vault root).
fn parent_dir(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
        .trim_matches('/')
        .to_string()
}

type PendingNote = (String, String, String, Option<serde_json::Value>, String);

/// Embed a batch of notes in one embedder call and persist each. The embedder is
/// synchronous and CPU-bound, so it runs on the blocking pool.
async fn flush_batch(
    db: &Database,
    emb: &Arc<dyn Embedder>,
    vault_id: &str,
    notes: Vec<PendingNote>,
) -> AppResult<usize> {
    let inputs: Vec<String> = notes.iter().map(|n| n.1.clone()).collect();
    let emb2 = emb.clone();
    let vectors = tokio::task::spawn_blocking(move || emb2.embed(&inputs))
        .await
        .map_err(|e| AppError::InternalError(format!("embed task failed: {e}")))??;

    let mut count = 0usize;
    for ((rel, _input, hash, frontmatter, body), vector) in notes.into_iter().zip(vectors) {
        store_embedding(db, emb, vault_id, &rel, &vector, &hash, frontmatter.as_ref(), &body)
            .await?;
        count += 1;
    }
    Ok(count)
}

async fn embed_one(emb: &Arc<dyn Embedder>, input: String) -> AppResult<Vec<f32>> {
    let emb2 = emb.clone();
    let mut vectors = tokio::task::spawn_blocking(move || emb2.embed(&[input]))
        .await
        .map_err(|e| AppError::InternalError(format!("embed task failed: {e}")))??;
    vectors
        .pop()
        .ok_or_else(|| AppError::InternalError("embedder returned no vector".to_string()))
}

#[allow(clippy::too_many_arguments)]
async fn store_embedding(
    db: &Database,
    emb: &Arc<dyn Embedder>,
    vault_id: &str,
    rel_path: &str,
    vector: &[f32],
    hash: &str,
    frontmatter: Option<&serde_json::Value>,
    body: &str,
) -> AppResult<()> {
    let tags = frontmatter_service::extract_tags(frontmatter, body);
    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    let blob = vector_to_blob(vector);
    let now = chrono::Utc::now().to_rfc3339();
    db.upsert_note_embedding(
        vault_id,
        rel_path,
        emb.model_name(),
        vector.len(),
        &blob,
        hash,
        &tags_json,
        &now,
    )
    .await
}

#[cfg(feature = "embeddings")]
mod fastembed_backend {
    use super::Embedder;
    use crate::config::MlConfig;
    use crate::error::{AppError, AppResult};
    use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// fastembed-rs backend. The model is loaded once at construction; `embed`
    /// serializes calls through a mutex since `TextEmbedding` needs `&mut self`.
    pub struct FastEmbedder {
        model_name: String,
        dim: usize,
        inner: Mutex<TextEmbedding>,
    }

    impl FastEmbedder {
        pub fn load(config: &MlConfig) -> AppResult<Self> {
            let model = resolve_model(&config.model)?;
            let dim = model_dim(&model);

            let cache_dir = if config.cache_dir.is_empty() {
                default_cache_dir()
            } else {
                PathBuf::from(&config.cache_dir)
            };

            // Air-gap safety: when downloads are disabled, refuse to construct the
            // model unless its files are already present in the cache directory.
            if !config.allow_model_download && !model_is_cached(&cache_dir, &config.model) {
                return Err(AppError::InternalError(format!(
                    "embedding model '{}' is not present in cache '{}' and \
                     allow_model_download is false",
                    config.model,
                    cache_dir.display()
                )));
            }

            let options = InitOptions::new(model)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(false);

            let embedding = TextEmbedding::try_new(options)
                .map_err(|e| AppError::InternalError(format!("failed to load embedder: {e}")))?;

            Ok(Self {
                model_name: config.model.clone(),
                dim,
                inner: Mutex::new(embedding),
            })
        }
    }

    impl Embedder for FastEmbedder {
        fn model_name(&self) -> &str {
            &self.model_name
        }

        fn dim(&self) -> usize {
            self.dim
        }

        fn embed(&self, texts: &[String]) -> AppResult<Vec<Vec<f32>>> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            // `embed` takes `&self`; the mutex only guards interior session state
            // and guarantees `Sync` regardless of the backend's own bounds.
            let model = self
                .inner
                .lock()
                .map_err(|_| AppError::InternalError("embedder mutex poisoned".to_string()))?;
            model
                .embed(texts.to_vec(), None)
                .map_err(|e| AppError::InternalError(format!("embedding failed: {e}")))
        }
    }

    fn resolve_model(name: &str) -> AppResult<EmbeddingModel> {
        match name {
            "bge-small-en-v1.5" | "BGESmallENV15" => Ok(EmbeddingModel::BGESmallENV15),
            "bge-base-en-v1.5" | "BGEBaseENV15" => Ok(EmbeddingModel::BGEBaseENV15),
            "all-MiniLM-L6-v2" | "AllMiniLML6V2" => Ok(EmbeddingModel::AllMiniLML6V2),
            other => Err(AppError::InternalError(format!(
                "unsupported embedding model '{other}'"
            ))),
        }
    }

    fn model_dim(model: &EmbeddingModel) -> usize {
        match model {
            EmbeddingModel::BGEBaseENV15 => 768,
            _ => 384,
        }
    }

    fn default_cache_dir() -> PathBuf {
        dirs_cache().join("librarium").join("ml-models")
    }

    fn dirs_cache() -> PathBuf {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .unwrap_or_else(|| PathBuf::from(".cache"))
    }

    /// Heuristic check that a model is already side-loaded: fastembed lays models
    /// out under `<cache>/models--*` (HF layout) or a flat folder. We treat a
    /// non-empty cache directory as "present" to avoid network fetches.
    fn model_is_cached(cache_dir: &std::path::Path, _model: &str) -> bool {
        std::fs::read_dir(cache_dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic stand-in embedder so the math can be tested without the
    /// native backend. Maps text to a small vector by hashing characters into
    /// buckets — similar strings get similar vectors.
    struct MockEmbedder;

    impl Embedder for MockEmbedder {
        fn model_name(&self) -> &str {
            "mock"
        }
        fn dim(&self) -> usize {
            8
        }
        fn embed(&self, texts: &[String]) -> AppResult<Vec<Vec<f32>>> {
            Ok(texts
                .iter()
                .map(|t| {
                    let mut v = vec![0.0f32; 8];
                    for (i, b) in t.bytes().enumerate() {
                        v[(b as usize + i) % 8] += 1.0;
                    }
                    v
                })
                .collect())
        }
    }

    #[test]
    fn blob_roundtrips() {
        let v = vec![0.0f32, 1.5, -2.25, 3.125];
        assert_eq!(blob_to_vector(&vector_to_blob(&v)), v);
    }

    #[test]
    fn content_hash_is_model_sensitive_and_stable() {
        let a = content_hash("m1", "hello");
        assert_eq!(a, content_hash("m1", "hello"));
        assert_ne!(a, content_hash("m2", "hello"));
        assert_ne!(a, content_hash("m1", "world"));
    }

    #[test]
    fn embedding_input_prefixes_stem_and_truncates() {
        let input = embedding_input("notes/Project Plan.md", &"x".repeat(5000));
        assert!(input.starts_with("Project Plan\n"));
        assert!(input.len() <= 2000 + "Project Plan\n".len());
    }

    #[test]
    fn cosine_basics() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn mock_embedder_is_used() {
        // Exercises the trait + batch path so the mock isn't dead code.
        let out = MockEmbedder.embed(&["a".to_string(), "b".to_string()]).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), 8);
    }

    #[test]
    fn prototypes_average_and_suggest_nearest() {
        // Two "programming" notes cluster near the x-axis, one "cooking" note
        // near the y-axis. The prototype mean + nearest-tag logic is tested with
        // explicit vectors so the assertion is deterministic.
        let notes = vec![
            TaggedNote {
                vector: vec![1.0, 0.1, 0.0],
                tags: vec!["programming".to_string()],
            },
            TaggedNote {
                vector: vec![0.9, -0.1, 0.0],
                tags: vec!["programming".to_string()],
            },
            TaggedNote {
                vector: vec![0.0, 1.0, 0.0],
                tags: vec!["cooking".to_string()],
            },
        ];
        let protos = build_tag_prototypes(&notes);
        assert_eq!(protos.len(), 2);
        // The programming prototype is the L2-normalized mean of its two notes.
        let prog = &protos["programming"];
        assert!((prog.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-5);

        let target = vec![0.95, 0.0, 0.0];
        let existing = HashSet::new();
        let suggestions = semantic_tag_suggestions(&target, &protos, &existing, 0.0, 5);
        assert_eq!(suggestions[0].0, "programming");
        assert!(suggestions[0].1 > suggestions[1].1);
    }

    /// Deterministic embedder keyed on keywords, with an embed-call counter so we
    /// can assert the content-hash skip path avoids recompute.
    struct KeywordEmbedder(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl Embedder for KeywordEmbedder {
        fn model_name(&self) -> &str {
            "keyword-mock"
        }
        fn dim(&self) -> usize {
            3
        }
        fn embed(&self, texts: &[String]) -> AppResult<Vec<Vec<f32>>> {
            self.0
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(texts
                .iter()
                .map(|t| {
                    if t.contains("rust") {
                        vec![1.0, 0.0, 0.0]
                    } else if t.contains("bread") {
                        vec![0.0, 1.0, 0.0]
                    } else {
                        vec![0.0, 0.0, 1.0]
                    }
                })
                .collect())
        }
    }

    #[tokio::test]
    async fn embed_store_skip_and_semantic_suggest_end_to_end() {
        use crate::config::{MlConfig, MlTier};
        use crate::db::Database;
        use std::sync::atomic::Ordering;
        use std::sync::{atomic::AtomicUsize, Arc};

        let calls = Arc::new(AtomicUsize::new(0));
        // The embedder global is process-wide; this test owns it.
        assert!(
            set_embedder_for_test(Some(Arc::new(KeywordEmbedder(calls.clone())))),
            "embedder global already initialized by another test"
        );

        let tmp = tempfile::tempdir().unwrap();
        let db = Database::new(&format!("sqlite://{}", tmp.path().join("emb.db").display()))
            .await
            .unwrap();
        let vault = db
            .create_vault("V".to_string(), tmp.path().to_string_lossy().to_string())
            .await
            .unwrap();
        let vid = &vault.id;

        // First embed stores a vector; a second identical embed is skipped.
        let ml_note = "---\ntags: [programming]\n---\nrust async tokio runtime";
        embed_note(&db, vid, "ml.md", ml_note).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        embed_note(&db, vid, "ml.md", ml_note).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "unchanged note must skip");

        // A second tagged note in a different topic.
        embed_note(
            &db,
            vid,
            "cook.md",
            "---\ntags: [cooking]\n---\nbread sourdough recipe",
        )
        .await
        .unwrap();

        let config = MlConfig {
            enabled: true,
            tier: MlTier::Embeddings,
            ..MlConfig::default()
        };
        let existing = HashSet::new();
        let suggestions =
            suggest_semantic_tags(&db, &config, vid, "new.md", "rust trait generics", &existing, 5)
                .await
                .unwrap();

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].0, "programming");
        assert!(suggestions[0].1 > 0.9);

        // Deleting the embedding removes it from the store.
        remove_note(&db, vid, "ml.md").await.unwrap();
        assert!(db.get_note_embedding_hash(vid, "ml.md").await.unwrap().is_none());
    }

    #[test]
    fn knn_folder_vote_picks_dominant_neighbour_folder() {
        let target = vec![1.0, 0.0, 0.0];
        let neighbours = vec![
            ("projects".to_string(), vec![1.0, 0.0, 0.0]),
            ("projects".to_string(), vec![0.9, 0.1, 0.0]),
            ("journal".to_string(), vec![0.0, 1.0, 0.0]),
            ("journal".to_string(), vec![0.0, 0.9, 0.1]),
        ];
        let out = knn_folder_vote(&target, &neighbours, 10, 0.3, "inbox");
        assert_eq!(out.unwrap().0, "projects");
    }

    #[test]
    fn knn_folder_vote_skips_current_and_low_confidence() {
        let target = vec![1.0, 0.0, 0.0];
        // Winner folder is the note's current folder -> no move suggested.
        let same = vec![
            ("inbox".to_string(), vec![1.0, 0.0, 0.0]),
            ("inbox".to_string(), vec![0.9, 0.0, 0.0]),
        ];
        assert!(knn_folder_vote(&target, &same, 10, 0.3, "inbox").is_none());

        // A perfectly split vote (0.5 share each) is below a 0.6 threshold.
        let split = vec![
            ("a".to_string(), vec![1.0, 0.0, 0.0]),
            ("b".to_string(), vec![1.0, 0.0, 0.0]),
        ];
        assert!(knn_folder_vote(&target, &split, 10, 0.6, "inbox").is_none());
    }

    #[test]
    fn suggestions_exclude_existing_and_respect_threshold() {
        let mut protos = HashMap::new();
        protos.insert("keep".to_string(), vec![1.0, 0.0, 0.0]);
        protos.insert("already".to_string(), vec![1.0, 0.0, 0.0]);
        protos.insert("toofar".to_string(), vec![0.0, 1.0, 0.0]);

        let mut existing = HashSet::new();
        existing.insert("already".to_string());

        let target = vec![1.0, 0.0, 0.0];
        let out = semantic_tag_suggestions(&target, &protos, &existing, 0.5, 5);
        // "already" excluded, "toofar" below threshold -> only "keep".
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "keep");
    }
}
