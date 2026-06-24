use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use librarium::services::organize_service::{c_tf_idf_labels, cluster_by_threshold};
use librarium::services::MarkdownService;
use librarium::services::MlService;

fn benchmark_simple_markdown(c: &mut Criterion) {
    let markdown =
        "# Hello World\n\nThis is a **simple** markdown document with *some* formatting.";

    c.bench_function("simple_markdown", |b| {
        b.iter(|| MarkdownService::to_html_with_highlighting(black_box(markdown), false))
    });
}

fn benchmark_complex_markdown(c: &mut Criterion) {
    let markdown = r#"# Complex Document

This is a **complex** document with *various* elements.

## Lists

- Item 1
- Item 2
  - Nested item
- Item 3

## Code

```rust
fn main() {
    println!("Hello, world!");
}
```

## Blockquote

> This is a quote
> with multiple lines

## Links

[Link](https://example.com) and ![Image](image.jpg)
"#;

    c.bench_function("complex_markdown", |b| {
        b.iter(|| MarkdownService::to_html_with_highlighting(black_box(markdown), false))
    });
}

fn benchmark_code_highlighting(c: &mut Criterion) {
    let markdown = r#"```rust
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn main() {
    for i in 0..10 {
        println!("fib({}) = {}", i, fibonacci(i));
    }
}
```"#;

    c.bench_function("code_highlighting", |b| {
        b.iter(|| MarkdownService::to_html(black_box(markdown)))
    });
}

fn benchmark_large_document(c: &mut Criterion) {
    let mut markdown = String::new();
    for i in 1..=50 {
        markdown.push_str(&format!("## Section {}\n\n", i));
        markdown.push_str("This is a paragraph with **bold** and *italic* text.\n\n");
        markdown.push_str("- List item 1\n- List item 2\n- List item 3\n\n");
    }

    c.bench_function("large_document_50_sections", |b| {
        b.iter(|| MarkdownService::to_html_with_highlighting(black_box(&markdown), false))
    });
}

fn benchmark_plain_text_extraction(c: &mut Criterion) {
    let markdown = r#"# Title

This is a **complex** document with *various* formatting elements, [links](url), and `code`.

## Section 1

Content here with more **bold** and *italic* text.
"#;

    c.bench_function("plain_text_extraction", |b| {
        b.iter(|| MarkdownService::to_plain_text(black_box(markdown)))
    });
}

fn benchmark_excerpt_generation(c: &mut Criterion) {
    let markdown = "# Long Article\n\nThis is a very long article with lots of content that should be truncated properly when generating an excerpt. It contains multiple paragraphs and various formatting elements.";

    c.bench_function("excerpt_generation", |b| {
        b.iter(|| MarkdownService::get_excerpt(black_box(markdown), 100))
    });
}

fn benchmark_document_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_sizes");

    for size in [10, 50, 100, 200].iter() {
        let mut markdown = String::new();
        for i in 1..=*size {
            markdown.push_str(&format!(
                "## Section {}\n\nContent for section {}.\n\n",
                i, i
            ));
        }

        group.bench_with_input(BenchmarkId::from_parameter(size), &markdown, |b, md| {
            b.iter(|| MarkdownService::to_html_with_highlighting(black_box(md), false))
        });
    }

    group.finish();
}

// ── LIB-066: organization (Tier 1/2) benchmarks on a synthetic large vault ────

/// Deterministic LCG so benchmarks are reproducible without `rand`/`Math::random`.
fn lcg(state: &mut u64) -> f32 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*state >> 40) as f32 / (1u64 << 24) as f32) - 0.5
}

/// `n` synthetic embedding vectors of dimension `dim` drawn around `k` centroids.
fn synthetic_vectors(n: usize, dim: usize, k: usize) -> Vec<Vec<f32>> {
    let mut s = 0x9E3779B97F4A7C15u64;
    let centroids: Vec<Vec<f32>> = (0..k)
        .map(|_| (0..dim).map(|_| lcg(&mut s)).collect())
        .collect();
    (0..n)
        .map(|i| {
            let c = &centroids[i % k];
            c.iter().map(|&x| x + 0.03 * lcg(&mut s)).collect()
        })
        .collect()
}

/// A synthetic vault of `n` notes spread across three topical folders.
fn synthetic_vault(n: usize) -> Vec<(String, String)> {
    const TOPICS: [(&str, &str); 3] = [
        ("projects", "rust async tokio milestone roadmap deliverable sprint planning backend"),
        ("journal", "today morning coffee weather mood reflection evening walk thoughts"),
        ("recipes", "flour sugar butter oven bake dough knead proof yeast sourdough"),
    ];
    (0..n)
        .map(|i| {
            let (folder, words) = TOPICS[i % 3];
            (folder.to_string(), format!("note {i} {words}"))
        })
        .collect()
}

fn benchmark_clustering(c: &mut Criterion) {
    let mut group = c.benchmark_group("vault_clustering");
    for n in [100usize, 300, 600] {
        let vectors = synthetic_vectors(n, 384, 6);
        group.bench_with_input(BenchmarkId::from_parameter(n), &vectors, |b, v| {
            b.iter(|| cluster_by_threshold(black_box(v), 0.6))
        });
    }
    group.finish();
}

fn benchmark_ctfidf_labels(c: &mut Criterion) {
    let n = 600usize;
    let vectors = synthetic_vectors(n, 384, 6);
    let clusters = cluster_by_threshold(&vectors, 0.6);
    let docs: Vec<Vec<String>> = (0..n)
        .map(|i| {
            format!("note {i} rust async tokio milestone roadmap")
                .split_whitespace()
                .map(str::to_string)
                .collect()
        })
        .collect();

    c.bench_function("ctfidf_labels_600", |b| {
        b.iter(|| c_tf_idf_labels(black_box(&clusters), black_box(&docs), 2))
    });
}

fn benchmark_tfidf_folder_placement(c: &mut Criterion) {
    let mut group = c.benchmark_group("tfidf_folder_placement");
    let target = "writing a rust async function with tokio for the backend roadmap";
    for n in [100usize, 500, 1000] {
        let vault = synthetic_vault(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &vault, |b, notes| {
            b.iter(|| MlService::nearest_folder_tfidf(black_box(target), black_box(notes), 0.0))
        });
    }
    group.finish();
}

fn benchmark_keyphrase_extraction(c: &mut Criterion) {
    let mut content = String::new();
    for i in 0..40 {
        content.push_str(&format!(
            "Section {i}: machine learning models and semantic search over markdown notes. "
        ));
    }
    c.bench_function("keyphrase_extraction_classical", |b| {
        b.iter(|| MlService::keyphrases_for_tier(black_box(&content), "classical", 10))
    });
}

/// Embedding throughput. Registers a real benchmark only when the server is
/// built `--features embeddings` AND a model is available; otherwise it is a
/// no-op so the default build still compiles and runs the suite.
fn benchmark_embedding_throughput(c: &mut Criterion) {
    use librarium::config::{AppConfig, MlTier};
    use librarium::services::embedding_service;

    let mut config = AppConfig::default();
    config.ml.enabled = true;
    config.ml.tier = MlTier::Embeddings;

    let Some(embedder) = embedding_service::embedder(&config.ml) else {
        // No backend/model (default build or offline) — nothing to benchmark.
        return;
    };

    let docs: Vec<String> = (0..64)
        .map(|i| format!("sentence number {i} about rust embeddings and semantic organization"))
        .collect();

    c.bench_function("embedding_throughput_64", |b| {
        b.iter(|| {
            let _ = embedder.embed(black_box(&docs));
        })
    });
}

criterion_group!(
    benches,
    benchmark_simple_markdown,
    benchmark_complex_markdown,
    benchmark_code_highlighting,
    benchmark_large_document,
    benchmark_plain_text_extraction,
    benchmark_excerpt_generation,
    benchmark_document_sizes,
    benchmark_clustering,
    benchmark_ctfidf_labels,
    benchmark_tfidf_folder_placement,
    benchmark_keyphrase_extraction,
    benchmark_embedding_throughput
);

criterion_main!(benches);
