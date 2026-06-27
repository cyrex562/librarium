//! LIB-074: optional "local LM" organization tier.
//!
//! A small, fully-local label scorer that ranks candidate folders/categories for
//! a note using zero-shot semantic similarity over the side-loaded ONNX
//! embedding model. It is an *additive refinement* on top of the embeddings
//! tier: when no local model is available the provider returns `None` and the
//! organizer falls back to the existing tiers unchanged. Nothing here makes an
//! outbound call, so the air-gap default (no model download) is preserved.
//!
//! The scorer reuses the same ONNX runtime as [`crate::services::embedding_service`];
//! enabling it is purely a matter of selecting `tier = "local_lm"` and having a
//! local embedding model present (built with `--features embeddings` and a
//! pre-seeded model). See `docs/ORGANIZATION_ML_PLAN.md`.

use crate::config::{MlConfig, MlTier};
use crate::services::embedding_service::{cosine_similarity, embedder, Embedder};
use std::sync::{Arc, OnceLock};

/// Scores how well each candidate label fits a note's text. Each returned score
/// is in `[0, 1]` (higher = better fit) and the result has one entry per label.
pub trait LabelScorer: Send + Sync {
    fn score(&self, text: &str, labels: &[String]) -> Vec<f32>;
}

/// Process-wide label scorer, initialized once from the active config. `None`
/// means the local-LM tier is inactive or no local model is available — callers
/// must treat that as "leave candidate ranking as-is".
static SCORER: OnceLock<Option<Arc<dyn LabelScorer>>> = OnceLock::new();

/// Return the shared label scorer, lazily initializing it from `config`.
pub fn label_scorer(config: &MlConfig) -> Option<Arc<dyn LabelScorer>> {
    SCORER.get_or_init(|| init_scorer(config)).clone()
}

#[cfg(test)]
/// Install a specific scorer for tests. Returns `false` if one was already set
/// (the `OnceLock` is process-global).
pub fn set_label_scorer_for_test(s: Option<Arc<dyn LabelScorer>>) -> bool {
    SCORER.set(s).is_ok()
}

fn init_scorer(config: &MlConfig) -> Option<Arc<dyn LabelScorer>> {
    if !config.enabled || config.tier != MlTier::LocalLm {
        return None;
    }

    match embedder(config) {
        Some(e) => {
            tracing::info!(
                model = %e.model_name(),
                "Local-LM label scorer ready (embedding zero-shot)"
            );
            Some(Arc::new(EmbeddingZeroShotScorer { embedder: e }) as Arc<dyn LabelScorer>)
        }
        None => {
            tracing::warn!(
                "ML tier is 'local_lm' but no local embedding model is available; \
                 falling back to lower tiers for organization"
            );
            None
        }
    }
}

/// Zero-shot label scorer: embeds the note and each candidate label with the
/// local model and returns cosine similarity remapped from `[-1, 1]` to `[0, 1]`.
struct EmbeddingZeroShotScorer {
    embedder: Arc<dyn Embedder>,
}

impl LabelScorer for EmbeddingZeroShotScorer {
    fn score(&self, text: &str, labels: &[String]) -> Vec<f32> {
        if labels.is_empty() {
            return Vec::new();
        }

        // One embedding pass over the note followed by each humanized label.
        let mut batch = Vec::with_capacity(labels.len() + 1);
        batch.push(text.to_string());
        batch.extend(labels.iter().map(|l| humanize_label(l)));

        let vectors = match self.embedder.embed(&batch) {
            Ok(v) if v.len() == labels.len() + 1 => v,
            // On any failure, stay neutral so ranking is unchanged.
            _ => return vec![0.5; labels.len()],
        };

        let note = &vectors[0];
        vectors[1..]
            .iter()
            .map(|lv| ((cosine_similarity(note, lv) + 1.0) / 2.0).clamp(0.0, 1.0))
            .collect()
    }
}

/// Turn a slug-ish folder label (`rust-async/web`) into readable text
/// (`rust async web`) so it embeds closer to natural-language note content.
fn humanize_label(label: &str) -> String {
    label
        .split(['/', '-', '_'])
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake embedder: bag-of-words over a fixed vocab, so cosine reflects token
    /// overlap. Lets us test the scorer without the ONNX backend.
    struct BowEmbedder;
    impl Embedder for BowEmbedder {
        fn model_name(&self) -> &str {
            "bow-test"
        }
        fn dim(&self) -> usize {
            4
        }
        fn embed(&self, texts: &[String]) -> crate::error::AppResult<Vec<Vec<f32>>> {
            // Vocab axes: rust, async, bread, flour.
            let vocab = ["rust", "async", "bread", "flour"];
            Ok(texts
                .iter()
                .map(|t| {
                    let lower = t.to_lowercase();
                    vocab
                        .iter()
                        .map(|w| if lower.contains(w) { 1.0 } else { 0.0 })
                        .collect()
                })
                .collect())
        }
    }

    #[test]
    fn humanize_label_splits_separators() {
        assert_eq!(humanize_label("rust-async/web_dev"), "rust async web dev");
    }

    #[test]
    fn zero_shot_scores_favor_overlapping_label() {
        let scorer = EmbeddingZeroShotScorer {
            embedder: Arc::new(BowEmbedder),
        };
        let labels = vec!["rust-async".to_string(), "bread-flour".to_string()];
        let scores = scorer.score("a note about rust and async runtimes", &labels);
        assert_eq!(scores.len(), 2);
        // The rust/async label should outscore the cooking one.
        assert!(scores[0] > scores[1], "scores: {scores:?}");
    }

    #[test]
    fn empty_labels_give_empty_scores() {
        let scorer = EmbeddingZeroShotScorer {
            embedder: Arc::new(BowEmbedder),
        };
        assert!(scorer.score("anything", &[]).is_empty());
    }
}
