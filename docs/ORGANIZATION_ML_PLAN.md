# Local-ML Document Organization Plan (LIB-053)

## Overview

This document specifies the design for the **Organization** feature: a system that
**parses**, **tags**, **renames**, and **organizes** the Markdown documents in a vault.
The defining constraint is that all intelligence must run on **local compute only** —
no calls to online LLMs or any external service. This satisfies the air-gap stance
already established for the auth stack (see `LIB-043`): a Librarium install on an
isolated network must remain fully functional.

The feature is an evolution of the existing rule-based "AI Insights" surface
(`MlService`, `routes/ml.rs`, `MlInsightsPanel.vue`), not a greenfield replacement.
That surface already gives us the right *interaction model* — suggest-only,
dry-run, apply, and per-action undo receipts (`ml_undo_receipts`). We keep that
model and upgrade the *intelligence* behind it, then add the missing **rename**
verb and a **vault-wide batch** mode.

## Goals

- Suggest (and, on explicit apply, perform) tagging, renaming, and folder
  organization for Markdown notes.
- Run entirely on local CPU. No online LLM, no telemetry, nothing leaves the host.
- Degrade gracefully: a useful baseline with **zero model downloads**, with neural
  embeddings as an **opt-in** enhancement that can be side-loaded for air-gapped hosts.
- Preserve the existing safety model: suggest-only by default, dry-run preview,
  reversible apply.
- Keep link integrity: renaming/moving a note must rewrite inbound `[[wiki-links]]`.

## Non-Goals

- No online LLM integration of any kind (explicitly out of scope per the request).
- No automatic, unattended mutation of the vault. Every change is user-reviewed.
- Not a full RAG / chat-over-notes system. (Embeddings we build here can later be
  reused for that, but it is not part of this plan.)

## Research: Local Techniques Considered

All of the following run locally on CPU. Citations are to the libraries/algorithms
evaluated for the Rust backend.

### Keyphrase / keyword extraction (no model required)

- **TF-IDF**, **RAKE**, **YAKE!**, **TextRank**, and co-occurrence are all available as
  pure-Rust crates. The widely-cited
  [`keyword_extraction`](https://crates.io/crates/keyword_extraction) crate bundles
  several, but it is **LGPL-3.0** — incompatible with this project's **MIT** license and
  its statically-linked single-file binaries (LIB-045). We therefore use the standalone
  **MIT-licensed** [`yake-rust`](https://docs.rs/yake-rust) crate instead (the
  [`keyphrases`](https://github.com/jjoeldaniel/keyphrases.rs) RAKE crate, WTFPL, is a
  permissive fallback). **Implemented:** `yake-rust` with only the English stopword list
  enabled (`default-features = false, features = ["en"]`) to keep the binary small.
- YAKE! is statistical, unsupervised, and needs no training corpus — a strong default for
  per-note tag candidates. Its raw score is "lower = more important"; we invert it into a
  relevance in `(0, 1]`. See the
  [algorithm walkthrough](https://dev.to/tugascript/rust-keyword-extraction-creating-the-yake-algorithm-from-scratch-4n2l).
- These power **Tier 1** below: good tag/keyphrase suggestions with **no model download**.

### Local neural sentence embeddings (opt-in model)

- [`fastembed-rs`](https://github.com/anush008/fastembed-rs): synchronous local ONNX
  inference, no Tokio requirement, default model **BGE-small-en-v1.5** (384-dim),
  quantized variants available. Models download once to a cache dir
  (`FASTEMBED_CACHE_DIR` / `with_cache_dir`) and **run offline thereafter** — and the
  cache can be pre-seeded for air-gapped installs. See the
  [crate docs](https://docs.rs/fastembed/latest/fastembed/).
- [`candle`](https://github.com/huggingface/candle) (pure-Rust) can run BERT
  sentence-transformers such as
  [`all-MiniLM-L6-v2`](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)
  (~22 MB, 384-dim, ~14k sentences/min on CPU). Either backend works; **`fastembed-rs`
  is the recommended default** because it bundles tokenizer + pooling and avoids
  hand-rolling mean-pooling (a known [candle footgun](https://github.com/huggingface/candle/issues/380)).
- Embeddings power **Tier 2**: semantic tag classification, semantic folder
  placement, clustering, and near-duplicate detection.

### Clustering (for vault-wide folder proposals)

- [`linfa-clustering`](https://docs.rs/linfa-clustering/) (scikit-learn-style, pure
  Rust): **K-Means** (rayon-parallel) and **DBSCAN**.
- [`hdbscan`](https://crates.io/crates/hdbscan): density-based, finds a variable
  number of clusters and labels outliers as noise — a better fit for "how many topics
  are in this vault?" than fixed-k K-Means. See the
  [Rust ML book](https://rust-ml.github.io/book/4_dbscan.html).
- Recommendation: **HDBSCAN** for the auto-taxonomy proposal (unknown cluster count),
  with K-Means available when the user wants a fixed number of folders.

### Naming / rename

- Title source priority: frontmatter `title` → first `# H1` → top keyphrases.
- Slugify to a configurable scheme (`kebab-case`, `Title Case`, date-prefixed
  `YYYY-MM-DD-slug`, or `category/slug`). No model strictly required; embeddings only
  help when choosing a category prefix.

## Architecture: Tiered Intelligence

The single most important design decision. Capability is layered so the baseline is
always available offline and the heavy path is opt-in.

| Tier | Name | Dependencies | Always offline? | Powers |
|------|------|--------------|-----------------|--------|
| **0** | Heuristics (exists today) | none | yes | keyword-rule tags, path/category inference, outline |
| **1** | Classical NLP | `keyword_extraction` (pure Rust) | yes | keyphrase tags, TF-IDF similarity, slug/rename, near-dup |
| **2** | Local embeddings | `fastembed-rs` + a model in cache | yes *after* one-time model provisioning | semantic tagging vs. vault vocabulary, semantic folder placement, clustering, related-notes |

`config.toml` selects the active tier (`[ml].tier = "heuristic" | "classical" | "embeddings"`).
Default is **`classical`** — useful, fully offline, no download. `embeddings` is opt-in.
If `embeddings` is selected but no model is present and `allow_model_download = false`
(the air-gap-safe default), the service logs once and **falls back to Tier 1** instead
of failing requests.

### The Parse foundation

All tiers consume a shared `NoteAnalysis` produced by a single parse pass:

```
NoteAnalysis {
  file_path, title, frontmatter,
  outline: Vec<OutlineSection>,        // exists today
  body_text, word_count, language_hint,
  inline_tags: Vec<String>,            // #tags per TAG_SYSTEM_SPEC
  wiki_links: Vec<String>, tasks, dates,
  keyphrases: Vec<(String, f32)>,      // Tier 1
  embedding: Option<Vec<f32>>,         // Tier 2, from store
}
```

`MlService` is refactored from a bag of static functions into a small pipeline that
builds `NoteAnalysis` once, then runs tag/rename/organize "suggesters" over it.

### Embedding store

New SQLite table (added in `db::run_migrations`, mirroring `ml_undo_receipts`):

```sql
CREATE TABLE IF NOT EXISTS note_embeddings (
  vault_id     TEXT NOT NULL,
  file_path    TEXT NOT NULL,
  model        TEXT NOT NULL,
  dim          INTEGER NOT NULL,
  vector       BLOB NOT NULL,        -- f32 little-endian
  content_hash TEXT NOT NULL,        -- skip recompute when unchanged
  updated_at   TEXT NOT NULL,
  PRIMARY KEY (vault_id, file_path)
);
```

Embeddings are computed **off the request path**, hooked into
`reindex_service::index_file` (alongside the existing Tantivy indexing), batched and
parallelized with `rayon` exactly like the batched-commit work in `5a9ea70`. A note is
re-embedded only when its `content_hash` changes. This makes clustering, related-notes,
and semantic search effectively free at query time.

### Safety model (unchanged, extended)

- **Suggest-only** surface; the "suggest-only" chip in `MlInsightsPanel` stays.
- **Dry-run** returns the would-be change set without writing.
- **Apply** writes and records an `MlUndoReceipt`. Rename adds a new `ReverseAction`
  variant; batch apply records a receipt group for **bulk undo**.
- Every mutation routes through `FileService` so existing path-safety checks hold.

## The Four Verbs

### 1. Parse
Single pass → `NoteAnalysis` (above). Reuses existing frontmatter/outline parsing;
adds keyphrases (Tier 1) and embedding lookup (Tier 2). Foundation for the rest.

### 2. Tag
Tag candidates merged from three sources, deduped, normalized per
`docs/TAG_SYSTEM_SPEC.md` (lowercase-canonical, nested `/` allowed), confidence-scored:
- Tier 0: existing keyword rules.
- Tier 1: top YAKE/TextRank keyphrases mapped to tag-shaped tokens.
- Tier 2: **controlled-vocabulary classification** — embed the note, compare to
  prototype vectors of the vault's *existing* tags (mean of notes carrying each tag),
  suggest the nearest tags above `min_confidence`. This makes suggestions consistent
  with the vocabulary the user already uses, instead of inventing new tags.
Apply reuses the existing `frontmatter_tag` write + `RemoveTag` undo path.

### 3. Rename (new)
- Propose a canonical filename from title/keyphrases under the configured
  `naming_scheme`.
- **Link integrity is mandatory**: before renaming, find inbound `[[wiki-links]]`
  (and `![[embeds]]`) via `wiki_link_service` / search index, and rewrite them in the
  same operation. Report the count of links to be updated in the dry-run.
- Apply uses `FileService::rename`; undo extends `ReverseAction::MoveBack` to also
  restore rewritten links (new `ReverseAction::RenameWithLinks { ... }`).

### 4. Organize
- **Single note (upgrade):** replace string-match `infer_category` with kNN over
  existing folders' embeddings (Tier 2) or TF-IDF nearest folder (Tier 1) → suggest
  target folder; reuse `MoveToFolder` apply/undo.
- **Vault-wide (new):** cluster all note embeddings (HDBSCAN) → derive a candidate
  folder per cluster (label via cluster-top keyphrases, c-TF-IDF style) → emit a
  **plan**: a reviewable list of `{file, suggested_tags, suggested_name, target_folder,
  confidence}`. Nothing moves until the user selects rows and applies. Applied as a
  batch with a single undo group.

## API Surface

Extends `routes/ml.rs` (all under `/api/vaults/{vault_id}/ml/...`, auth + vault-scoped):

- `POST /ml/analyze` — return `NoteAnalysis` for one note (parse + keyphrases + tier info).
- `POST /ml/suggestions` — existing; upgraded to include keyphrase- and embedding-based tags.
- `POST /ml/rename-suggestion` — propose a filename + inbound-link impact.
- `POST /ml/apply-suggestion` — existing; gains a `rename` suggestion kind.
- `POST /ml/organize-vault` — kick a batch analysis job; returns a `plan_id` + plan.
- `POST /ml/apply-plan` — apply selected plan rows; returns a receipt group id.
- `POST /ml/undo` — existing; supports single receipts and receipt groups.

Long-running batch jobs report progress over the existing WebSocket channel
(same pattern as `ReindexComplete`), filtered by vault authorization.

## Configuration

New `[ml]` section (defaults chosen for air-gap safety):

```toml
[ml]
enabled = true
tier = "classical"            # heuristic | classical | embeddings
model = "bge-small-en-v1.5"   # used only when tier = "embeddings"
cache_dir = ""                # defaults to {data_dir}/ml-models
allow_model_download = false  # air-gap default: never fetch at runtime
auto_suggest_on_open = false
naming_scheme = "kebab-case"  # kebab-case | title-case | date-prefixed | category-slug
min_confidence = 0.55
max_suggestions = 8
```

For air-gapped hosts: pre-seed `cache_dir` with the ONNX model (manual copy or bundled
in the installer) and set `tier = "embeddings"` with `allow_model_download = false`.

## Privacy & Air-Gap Guarantees

- Tiers 0/1 require no network ever.
- Tier 2 requires a model *file*; with `allow_model_download = false` the server never
  performs network I/O for ML — the model must be present locally or the tier falls
  back. This is asserted by an offline-mode integration test (mirrors `LIB-043`).
- No content is logged or transmitted. Embeddings stay in the vault's SQLite DB.

## Performance

- Model loaded lazily, once, behind a `OnceCell`/`Mutex`; reused across requests.
- Embeddings computed in background batches during reindex (`rayon`), never on the
  interactive request path; skipped when `content_hash` is unchanged.
- Per-note suggestion (Tier 1) is microseconds; (Tier 2) is one cached vector lookup
  plus a few hundred cosine comparisons against tag prototypes.
- Vault clustering is an explicit, progress-reported batch job, not on save.

## Frontend

- Upgrade `MlInsightsPanel.vue`: show extracted keyphrases, tag suggestions with
  confidence + source (rule/keyphrase/semantic), and a **rename suggestion** card
  showing the proposed name and "N inbound links will be updated".
- New **Organize Vault** modal: runs the batch job, shows the plan in a table with
  per-row checkboxes (tag / rename / move columns), an "Apply selected" action, and a
  "Undo last organize" affordance backed by the receipt group.
- Keep all of it behind the existing suggest-only framing.

## Testing

- Unit: keyphrase extraction, slug/naming schemes, tag normalization vs.
  `TAG_SYSTEM_SPEC`, cosine/prototype classification, clustering on a fixed fixture.
- Integration: dry-run vs. apply parity, rename rewrites inbound links and undo
  restores them, batch plan apply + bulk undo, **offline-mode** test proving no
  network I/O when `allow_model_download = false`.
- Bench: embedding throughput and clustering on a synthetic 5k-note vault
  (extend `benches/markdown_benchmarks.rs`).

## Phasing

1. **Foundations** — `NoteAnalysis` parse layer, `[ml]` config, tier plumbing (Tier 0/1).
2. **Tag upgrade** — Tier 1 keyphrase tags wired into existing suggestions surface.
3. **Rename** — naming schemes + inbound-link rewrite + undo.
4. **Embeddings** — `fastembed-rs`, embedding store, incremental compute in reindex.
5. **Organize** — semantic single-note placement, then vault-wide clustering + batch plan.
6. **Frontend + air-gap provisioning + tests/benches.**

See `TODO.md` (LIB-053 … LIB-066) for the task breakdown.
