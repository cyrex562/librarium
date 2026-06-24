use crate::models::{
    Keyphrase, NoteAnalysis, NoteOutlineResponse, NoteTask, OrganizationSuggestion,
    OrganizationSuggestionKind, OrganizationSuggestionsResponse, OutlineSection,
};
use crate::services::frontmatter_service;
use chrono::Utc;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct MlService;

impl MlService {
    pub fn generate_outline(
        file_path: &str,
        content: &str,
        max_sections: usize,
    ) -> NoteOutlineResponse {
        let capped_max_sections = max_sections.clamp(1, 100);

        let mut sections = Self::collect_sections(content);
        sections.truncate(capped_max_sections);

        let summary = Self::build_summary(content, sections.first().map(|s| s.title.as_str()));

        NoteOutlineResponse {
            file_path: file_path.to_string(),
            summary,
            sections,
            generated_at: Utc::now(),
        }
    }

    /// Parse-first analysis: build a single [`NoteAnalysis`] that the tag,
    /// rename, and organize suggesters all consume. Structural extraction only;
    /// keyphrases/embeddings are layered in by higher tiers.
    pub fn analyze(
        file_path: &str,
        content: &str,
        frontmatter: Option<&Value>,
        tier: &str,
    ) -> NoteAnalysis {
        let sections = Self::collect_sections(content);
        let summary = Self::build_summary(content, sections.first().map(|s| s.title.as_str()));
        let title = Self::extract_title(file_path, frontmatter, &sections);

        let frontmatter_tags = frontmatter_service::extract_tags(frontmatter, "")
            .into_iter()
            .map(|t| Self::normalize_tag(&t))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>();
        let inline_tags = Self::extract_inline_tags(content);
        let wiki_links = Self::extract_wiki_links(content);
        let tasks = Self::extract_tasks(content);
        let word_count = Self::count_words(content);
        let keyphrases = Self::keyphrases_for_tier(content, tier, 10);

        NoteAnalysis {
            file_path: file_path.to_string(),
            title,
            summary,
            sections,
            word_count,
            inline_tags,
            frontmatter_tags,
            wiki_links,
            tasks,
            keyphrases,
            tier: tier.to_string(),
            generated_at: Utc::now(),
        }
    }

    /// Extract keyphrases when the tier supports it (`classical` or
    /// `embeddings`); the `heuristic` tier returns an empty list so no
    /// statistical work is done.
    pub fn keyphrases_for_tier(content: &str, tier: &str, max: usize) -> Vec<Keyphrase> {
        match tier {
            "classical" | "embeddings" => Self::extract_keyphrases(content, max),
            _ => Vec::new(),
        }
    }

    /// Statistical keyphrase extraction via YAKE! (pure-Rust, no model). YAKE
    /// assigns lower raw scores to more important phrases; we invert that into a
    /// `relevance` in `(0, 1]` (higher = more important) stored on [`Keyphrase`].
    pub fn extract_keyphrases(content: &str, max: usize) -> Vec<Keyphrase> {
        if max == 0 || content.trim().is_empty() {
            return Vec::new();
        }

        let stop_words = match yake_rust::StopWords::predefined("en") {
            Some(sw) => sw,
            None => return Vec::new(),
        };
        let config = yake_rust::Config::default();

        yake_rust::get_n_best(max, content, &stop_words, &config)
            .into_iter()
            .map(|item| Keyphrase {
                phrase: item.raw,
                // Invert YAKE's "lower is better" score into (0, 1].
                score: (1.0 / (1.0 + item.score)) as f32,
            })
            .collect()
    }

    pub fn suggest_organization(
        file_path: &str,
        content: &str,
        frontmatter: Option<&Value>,
        keyphrases: &[Keyphrase],
        max_suggestions: usize,
    ) -> OrganizationSuggestionsResponse {
        let capped_max_suggestions = max_suggestions.clamp(1, 25);
        let content_lower = content.to_lowercase();

        let existing_tags = frontmatter_service::extract_tags(frontmatter, content);
        let existing_tag_set: HashSet<String> = existing_tags
            .iter()
            .map(|t| t.trim().trim_start_matches('#').to_lowercase())
            .collect();

        let mut suggestions: Vec<OrganizationSuggestion> = Vec::new();
        // Tags already covered by an existing frontmatter/inline tag or an
        // earlier (higher-priority) suggestion, so keyphrase tags don't repeat.
        let mut covered_tags: HashSet<String> = existing_tag_set.clone();

        let tag_rules: [(&[&str], &str, &str, f32); 7] = [
            (
                &["meeting", "agenda", "minutes"],
                "meeting",
                "Detected meeting-related terms in the note content.",
                0.86,
            ),
            (
                &["todo", "task", "action item", "checklist"],
                "tasks",
                "Detected task-oriented language suggesting task tracking.",
                0.89,
            ),
            (
                &["project", "milestone", "roadmap", "deliverable"],
                "project",
                "Detected project planning terminology in this note.",
                0.84,
            ),
            (
                &["bug", "issue", "fix", "regression"],
                "bug",
                "Detected bug/issue language in this note.",
                0.83,
            ),
            (
                &["idea", "brainstorm", "concept", "proposal"],
                "idea",
                "Detected ideation terms suggesting an idea note.",
                0.8,
            ),
            (
                &["daily", "journal", "reflection", "log"],
                "daily",
                "Detected journaling language in this note.",
                0.78,
            ),
            (
                &["research", "experiment", "analysis", "hypothesis"],
                "research",
                "Detected research terminology in this note.",
                0.81,
            ),
        ];

        for (keywords, tag, rationale, confidence) in tag_rules {
            let has_keyword = keywords.iter().any(|k| content_lower.contains(k));
            if !has_keyword {
                continue;
            }

            if existing_tag_set.contains(tag) {
                continue;
            }

            suggestions.push(OrganizationSuggestion {
                id: format!("tag:{}", tag),
                kind: OrganizationSuggestionKind::Tag,
                confidence,
                rationale: rationale.to_string(),
                tag: Some(tag.to_string()),
                category: None,
                target_folder: None,
                new_name: None,
                source: Some("rule".to_string()),
            });
            covered_tags.insert(tag.to_string());
        }

        // Keyphrase-derived tags (Tier 1). Convert each extracted phrase into a
        // tag token, skip ones already covered, and score below the rule tags so
        // the curated vocabulary stays on top.
        for kp in keyphrases {
            let Some(tag) = Self::phrase_to_tag(&kp.phrase) else {
                continue;
            };
            if !covered_tags.insert(tag.clone()) {
                continue;
            }

            // Map keyphrase relevance (0, 1] into a 0.50–0.74 confidence band.
            let confidence = (0.50 + 0.24 * kp.score.clamp(0.0, 1.0)).clamp(0.50, 0.74);
            suggestions.push(OrganizationSuggestion {
                id: format!("tag:{}", tag),
                kind: OrganizationSuggestionKind::Tag,
                confidence,
                rationale: format!("Key phrase \"{}\" stands out in this note.", kp.phrase),
                tag: Some(tag),
                category: None,
                target_folder: None,
                new_name: None,
                source: Some("keyphrase".to_string()),
            });
        }

        let inferred_category = Self::infer_category(file_path, &content_lower);
        if let Some(category) = inferred_category {
            let existing_category = frontmatter
                .and_then(|fm| fm.get("category"))
                .and_then(Value::as_str)
                .map(|s| s.to_lowercase());

            if existing_category.as_deref() != Some(category) {
                suggestions.push(OrganizationSuggestion {
                    id: format!("category:{}", category),
                    kind: OrganizationSuggestionKind::Category,
                    confidence: 0.76,
                    rationale: "Suggested from note path and content semantics.".to_string(),
                    tag: None,
                    category: Some(category.to_string()),
                    target_folder: None,
                    new_name: None,
                    source: Some("rule".to_string()),
                });
            }

            let target_folder = Self::folder_for_category(category);
            let normalized_path = file_path.replace('\\', "/").to_lowercase();
            if !normalized_path.starts_with(&target_folder.to_lowercase()) {
                suggestions.push(OrganizationSuggestion {
                    id: format!("move:{}", target_folder),
                    kind: OrganizationSuggestionKind::MoveToFolder,
                    confidence: 0.7,
                    rationale: "Path appears inconsistent with inferred category; move is suggested for organization only.".to_string(),
                    tag: None,
                    category: Some(category.to_string()),
                    target_folder: Some(target_folder),
                    new_name: None,
                    source: Some("rule".to_string()),
                });
            }
        }

        suggestions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        suggestions.truncate(capped_max_suggestions);

        OrganizationSuggestionsResponse {
            file_path: file_path.to_string(),
            suggestions,
            existing_tags,
            generated_at: Utc::now(),
        }
    }

    fn parse_heading(line: &str) -> Option<(u8, &str)> {
        let trimmed = line.trim_start();
        let hashes_len = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes_len == 0 || hashes_len > 6 {
            return None;
        }

        let remainder = trimmed[hashes_len..].trim();
        if remainder.is_empty() {
            return None;
        }

        Some((hashes_len as u8, remainder))
    }

    /// Collect the heading outline. Lines inside fenced code blocks (``` / ~~~)
    /// are skipped so `# comments` in code don't masquerade as headings.
    fn collect_sections(content: &str) -> Vec<OutlineSection> {
        let mut sections = Vec::new();
        let mut in_fence = false;
        let mut fence_marker = ' ';

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if let Some(marker) = Self::fence_marker(trimmed) {
                if in_fence && marker == fence_marker {
                    in_fence = false;
                } else if !in_fence {
                    in_fence = true;
                    fence_marker = marker;
                }
                continue;
            }
            if in_fence {
                continue;
            }
            if let Some((level, title)) = Self::parse_heading(line) {
                sections.push(OutlineSection {
                    level,
                    title: title.to_string(),
                    line_number: idx + 1,
                });
            }
        }

        sections
    }

    fn fence_marker(trimmed: &str) -> Option<char> {
        if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        }
    }

    /// Title resolution: frontmatter `title` -> first H1 -> first heading ->
    /// filename stem.
    fn extract_title(
        file_path: &str,
        frontmatter: Option<&Value>,
        sections: &[OutlineSection],
    ) -> Option<String> {
        if let Some(title) = frontmatter
            .and_then(|fm| fm.get("title"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            return Some(title.to_string());
        }

        if let Some(h1) = sections.iter().find(|s| s.level == 1) {
            return Some(h1.title.clone());
        }
        if let Some(first) = sections.first() {
            return Some(first.title.clone());
        }

        Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    }

    fn normalize_tag(raw: &str) -> String {
        raw.trim().trim_start_matches('#').to_lowercase()
    }

    /// Convert a free-text keyphrase into a tag token per
    /// `docs/TAG_SYSTEM_SPEC.md`: lowercase, spaces -> `-`, keep only
    /// `[a-z0-9_-]`, collapse/trim hyphens. Returns `None` when the result is
    /// too short, too long, or has no letters.
    fn phrase_to_tag(phrase: &str) -> Option<String> {
        let mut tag = String::new();
        let mut prev_hyphen = false;
        for ch in phrase.trim().chars() {
            let mapped = if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '_' {
                Some('_')
            } else if ch.is_whitespace() || ch == '-' || ch == '/' {
                Some('-')
            } else {
                None
            };

            match mapped {
                Some('-') => {
                    if !prev_hyphen && !tag.is_empty() {
                        tag.push('-');
                        prev_hyphen = true;
                    }
                }
                Some(c) => {
                    tag.push(c);
                    prev_hyphen = false;
                }
                None => {}
            }
        }

        let tag = tag.trim_matches('-').to_string();
        let has_alpha = tag.chars().any(|c| c.is_ascii_alphabetic());
        if tag.len() < 2 || tag.len() > 40 || !has_alpha {
            return None;
        }
        Some(tag)
    }

    /// Propose a canonical filename (including extension) for `file_path` under
    /// the given naming `scheme`. Returns `None` when no good name can be derived
    /// or the current name is already canonical.
    ///
    /// Schemes: `kebab-case` (default), `title-case`, `date-prefixed` (uses a
    /// frontmatter `date`/`created` value when present), `category-slug` (prefixes
    /// the frontmatter/inferred category).
    pub fn suggest_rename(
        file_path: &str,
        content: &str,
        frontmatter: Option<&Value>,
        keyphrases: &[Keyphrase],
        scheme: &str,
    ) -> Option<String> {
        let current_name = Path::new(file_path).file_name().and_then(|n| n.to_str())?;
        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md");

        // Title from frontmatter/H1 only (not the filename stem — renaming to the
        // current stem would be a no-op).
        let sections = Self::collect_sections(content);
        let base = Self::title_from_frontmatter_or_heading(frontmatter, &sections)
            .or_else(|| Self::keyphrase_base(keyphrases))?;

        let slug = Self::slugify_kebab(&base);
        if slug.is_empty() {
            return None;
        }

        let stem = match scheme {
            "title-case" => Self::slugify_title_case(&base),
            "date-prefixed" => match Self::frontmatter_date(frontmatter) {
                Some(date) => format!("{}-{}", date, slug),
                None => slug,
            },
            "category-slug" => {
                match Self::category_for_rename(file_path, content, frontmatter) {
                    Some(cat) => format!("{}-{}", Self::slugify_kebab(&cat), slug),
                    None => slug,
                }
            }
            // "kebab-case" and anything unrecognized.
            _ => slug,
        };

        if stem.trim().is_empty() {
            return None;
        }
        let proposed = format!("{}.{}", stem, ext);
        if proposed.eq_ignore_ascii_case(current_name) {
            return None;
        }
        Some(proposed)
    }

    fn title_from_frontmatter_or_heading(
        frontmatter: Option<&Value>,
        sections: &[OutlineSection],
    ) -> Option<String> {
        if let Some(title) = frontmatter
            .and_then(|fm| fm.get("title"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            return Some(title.to_string());
        }
        sections
            .iter()
            .find(|s| s.level == 1)
            .or_else(|| sections.first())
            .map(|s| s.title.clone())
    }

    fn keyphrase_base(keyphrases: &[Keyphrase]) -> Option<String> {
        keyphrases
            .iter()
            .map(|k| k.phrase.trim())
            .find(|p| !p.is_empty())
            .map(str::to_string)
    }

    fn frontmatter_date(frontmatter: Option<&Value>) -> Option<String> {
        let raw = frontmatter.and_then(|fm| {
            fm.get("date")
                .or_else(|| fm.get("created"))
                .and_then(Value::as_str)
        })?;
        // Accept a leading ISO date (YYYY-MM-DD); reject anything else.
        let candidate: String = raw.chars().take(10).collect();
        let valid = candidate.len() == 10
            && candidate.as_bytes()[4] == b'-'
            && candidate.as_bytes()[7] == b'-'
            && candidate
                .bytes()
                .enumerate()
                .all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() });
        valid.then_some(candidate)
    }

    fn category_for_rename(
        file_path: &str,
        content: &str,
        frontmatter: Option<&Value>,
    ) -> Option<String> {
        if let Some(cat) = frontmatter
            .and_then(|fm| fm.get("category"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            return Some(cat.to_string());
        }
        Self::infer_category(file_path, &content.to_lowercase()).map(str::to_string)
    }

    /// `Some Note Title!` -> `some-note-title`.
    fn slugify_kebab(text: &str) -> String {
        let mut out = String::new();
        let mut prev_hyphen = false;
        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_hyphen = false;
            } else if ch == '_' {
                out.push('_');
                prev_hyphen = false;
            } else if !out.is_empty() && !prev_hyphen {
                out.push('-');
                prev_hyphen = true;
            }
        }
        out.trim_matches('-').to_string()
    }

    /// `some note title` -> `Some Note Title`, stripping characters that are
    /// invalid in filenames.
    fn slugify_title_case(text: &str) -> String {
        text.split_whitespace()
            .map(|word| {
                let cleaned: String = word
                    .chars()
                    .filter(|c| {
                        c.is_alphanumeric() || matches!(c, '-' | '_' | '&' | '(' | ')')
                    })
                    .collect();
                let mut chars = cleaned.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + chars.as_str()
                    }
                    None => String::new(),
                }
            })
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Extract inline `#tags` from the body per `docs/TAG_SYSTEM_SPEC.md`
    /// (allowed chars `[a-zA-Z0-9_/-]`, nested `/`), skipping fenced code and
    /// returning lowercase-canonical tags in first-seen order without duplicates.
    fn extract_inline_tags(content: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut tags = Vec::new();
        let mut in_fence = false;
        let mut fence_marker = ' ';

        for line in content.lines() {
            let trimmed = line.trim_start();
            if let Some(marker) = Self::fence_marker(trimmed) {
                if in_fence && marker == fence_marker {
                    in_fence = false;
                } else if !in_fence {
                    in_fence = true;
                    fence_marker = marker;
                }
                continue;
            }
            if in_fence {
                continue;
            }

            let bytes = line.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'#' {
                    let prev_ok = i == 0 || bytes[i - 1].is_ascii_whitespace();
                    if prev_ok {
                        let start = i + 1;
                        let mut j = start;
                        while j < bytes.len() {
                            let c = bytes[j];
                            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'/') {
                                j += 1;
                            } else {
                                break;
                            }
                        }
                        // Require at least one letter so `#123` (issue refs) is ignored.
                        let candidate = &line[start..j];
                        if j > start && candidate.chars().any(|c| c.is_ascii_alphabetic()) {
                            let normalized = candidate.to_lowercase();
                            if seen.insert(normalized.clone()) {
                                tags.push(normalized);
                            }
                        }
                        i = j;
                        continue;
                    }
                }
                i += 1;
            }
        }

        tags
    }

    /// Extract `[[wiki-link]]` and `![[embed]]` targets, dropping any `#heading`
    /// or `|alias` suffix. Returns first-seen order without duplicates.
    fn extract_wiki_links(content: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut links = Vec::new();
        let bytes = content.as_bytes();
        let mut i = 0;

        while i + 1 < bytes.len() {
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                if let Some(end) = content[i + 2..].find("]]") {
                    let inner = &content[i + 2..i + 2 + end];
                    let target = inner
                        .split('|')
                        .next()
                        .unwrap_or(inner)
                        .split('#')
                        .next()
                        .unwrap_or(inner)
                        .trim();
                    if !target.is_empty() && seen.insert(target.to_string()) {
                        links.push(target.to_string());
                    }
                    i = i + 2 + end + 2;
                    continue;
                }
            }
            i += 1;
        }

        links
    }

    /// Extract checklist items (`- [ ]` / `- [x]`, also `*`/`+` bullets).
    fn extract_tasks(content: &str) -> Vec<NoteTask> {
        let mut tasks = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            let rest = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("+ "));
            let Some(rest) = rest else { continue };

            let (marker, after) = if let Some(after) = rest.strip_prefix("[ ]") {
                (false, after)
            } else if let Some(after) = rest
                .strip_prefix("[x]")
                .or_else(|| rest.strip_prefix("[X]"))
            {
                (true, after)
            } else {
                continue;
            };

            tasks.push(NoteTask {
                text: after.trim().to_string(),
                done: marker,
                line_number: idx + 1,
            });
        }
        tasks
    }

    /// Count whitespace-delimited word tokens in the body.
    fn count_words(content: &str) -> usize {
        content.split_whitespace().count()
    }

    fn build_summary(content: &str, first_heading: Option<&str>) -> String {
        let mut fragments = Vec::new();

        if let Some(title) = first_heading {
            fragments.push(format!("Focus: {}.", title));
        }

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
                continue;
            }

            fragments.push(trimmed.to_string());
            if fragments.len() >= 3 {
                break;
            }
        }

        let mut summary = fragments.join(" ");
        if summary.is_empty() {
            summary =
                "No substantial prose found; add content for a richer outline summary.".to_string();
        }

        if summary.len() > 260 {
            summary.truncate(257);
            summary.push_str("...");
        }

        summary
    }

    fn infer_category(file_path: &str, content_lower: &str) -> Option<&'static str> {
        let normalized_path = file_path.replace('\\', "/").to_lowercase();

        if normalized_path.contains("daily") || normalized_path.contains("journal") {
            return Some("journal");
        }
        if normalized_path.contains("meeting") {
            return Some("meetings");
        }
        if normalized_path.contains("project") {
            return Some("projects");
        }
        if normalized_path.contains("task") || normalized_path.contains("todo") {
            return Some("tasks");
        }

        if content_lower.contains("meeting") || content_lower.contains("agenda") {
            return Some("meetings");
        }
        if content_lower.contains("daily") || content_lower.contains("journal") {
            return Some("journal");
        }
        if content_lower.contains("project") || content_lower.contains("milestone") {
            return Some("projects");
        }
        if content_lower.contains("todo") || content_lower.contains("checklist") {
            return Some("tasks");
        }

        None
    }

    fn folder_for_category(category: &str) -> String {
        match category {
            "journal" => "journal/".to_string(),
            "meetings" => "meetings/".to_string(),
            "projects" => "projects/".to_string(),
            "tasks" => "tasks/".to_string(),
            other => format!("{}/", other),
        }
    }

    /// Tier-1 folder placement (LIB-062): TF-IDF nearest folder. Each note is a
    /// `(folder, content)` pair; notes are aggregated into per-folder documents,
    /// weighted by inverse document frequency across folders, and the target
    /// content is matched by cosine similarity. Returns the best folder and its
    /// score when it is at least `min_score`.
    ///
    /// This is the offline fallback used when no embedding backend is available;
    /// the heuristic `infer_category` remains the final fallback.
    pub fn nearest_folder_tfidf(
        target: &str,
        notes: &[(String, String)],
        min_score: f32,
    ) -> Option<(String, f32)> {
        if notes.is_empty() {
            return None;
        }

        // Aggregate token counts per folder.
        let mut folder_tf: HashMap<String, HashMap<String, f32>> = HashMap::new();
        for (folder, content) in notes {
            let counts = folder_tf.entry(folder.clone()).or_default();
            for tok in Self::tokenize(content) {
                *counts.entry(tok).or_insert(0.0) += 1.0;
            }
        }
        if folder_tf.len() < 2 {
            // With a single folder there is nothing to choose between.
            return None;
        }

        // Document frequency across folders, then idf.
        let n_folders = folder_tf.len() as f32;
        let mut df: HashMap<String, f32> = HashMap::new();
        for counts in folder_tf.values() {
            for tok in counts.keys() {
                *df.entry(tok.clone()).or_insert(0.0) += 1.0;
            }
        }
        let idf = |tok: &str| -> f32 {
            let d = df.get(tok).copied().unwrap_or(0.0);
            ((n_folders + 1.0) / (d + 1.0)).ln() + 1.0
        };

        // Target tf-idf vector.
        let mut target_vec: HashMap<String, f32> = HashMap::new();
        for tok in Self::tokenize(target) {
            *target_vec.entry(tok).or_insert(0.0) += 1.0;
        }
        if target_vec.is_empty() {
            return None;
        }
        for (tok, v) in target_vec.iter_mut() {
            *v *= idf(tok);
        }
        let target_norm = vec_norm(&target_vec);
        if target_norm == 0.0 {
            return None;
        }

        let mut best: Option<(String, f32)> = None;
        for (folder, counts) in &folder_tf {
            let mut dot = 0.0f32;
            let mut fnorm_sq = 0.0f32;
            for (tok, &tf) in counts {
                let w = tf * idf(tok);
                fnorm_sq += w * w;
                if let Some(tv) = target_vec.get(tok) {
                    dot += w * tv;
                }
            }
            if fnorm_sq == 0.0 {
                continue;
            }
            let score = dot / (target_norm * fnorm_sq.sqrt());
            if best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
                best = Some((folder.clone(), score));
            }
        }

        best.filter(|(_, s)| *s >= min_score)
    }

    /// Lowercase alphanumeric word tokens of length >= 3, used by the TF-IDF
    /// folder matcher. Deliberately simple and dependency-free.
    fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut cur = String::new();
        for ch in text.chars() {
            if ch.is_alphanumeric() {
                cur.extend(ch.to_lowercase());
            } else if !cur.is_empty() {
                if cur.len() >= 3 {
                    tokens.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
            }
        }
        if cur.len() >= 3 {
            tokens.push(cur);
        }
        tokens
    }
}

fn vec_norm(v: &HashMap<String, f32>) -> f32 {
    v.values().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::MlService;
    use crate::models::OrganizationSuggestionKind;

    #[test]
    fn outline_extracts_headings_and_summary() {
        let content = "# Project Alpha\n\nA short intro line.\n\n## Goals\n- item\n\n### Notes\nMore details.";
        let outline = MlService::generate_outline("projects/alpha.md", content, 10);

        assert_eq!(outline.file_path, "projects/alpha.md");
        assert_eq!(outline.sections.len(), 3);
        assert_eq!(outline.sections[0].level, 1);
        assert_eq!(outline.sections[0].title, "Project Alpha");
        assert!(outline.summary.to_lowercase().contains("project alpha"));
    }

    #[test]
    fn nearest_folder_tfidf_matches_topical_folder() {
        let notes = vec![
            (
                "programming".to_string(),
                "rust async tokio runtime borrow checker".to_string(),
            ),
            (
                "programming".to_string(),
                "rust trait generics lifetimes compiler".to_string(),
            ),
            (
                "cooking".to_string(),
                "sourdough bread baking flour yeast oven".to_string(),
            ),
            (
                "cooking".to_string(),
                "pasta sauce tomato garlic simmer recipe".to_string(),
            ),
        ];

        let target = "writing a rust function with generics and traits";
        let (folder, score) = MlService::nearest_folder_tfidf(target, &notes, 0.0).unwrap();
        assert_eq!(folder, "programming");
        assert!(score > 0.0);

        // A high threshold rejects a weak match.
        assert!(MlService::nearest_folder_tfidf("xyzzy plugh", &notes, 0.9).is_none());
        // A single folder gives nothing to choose between.
        assert!(MlService::nearest_folder_tfidf(target, &notes[..1], 0.0).is_none());
    }

    #[test]
    fn suggestions_include_tag_and_folder_move() {
        let content =
            "# Sprint Meeting\n\nAgenda:\n- Discuss project roadmap\n- Action items and todo list";
        let suggestions =
            MlService::suggest_organization("inbox/sprint-meeting.md", content, None, &[], 10);

        assert!(!suggestions.suggestions.is_empty());
        assert!(suggestions
            .suggestions
            .iter()
            .any(|s| matches!(s.kind, OrganizationSuggestionKind::Tag)
                && s.tag.as_deref() == Some("meeting")));
        assert!(suggestions
            .suggestions
            .iter()
            .any(|s| matches!(s.kind, OrganizationSuggestionKind::MoveToFolder)));
    }

    #[test]
    fn analyze_extracts_structure() {
        let content = "# Weekly Sync\n\nNotes about #project/ui and [[Roadmap]].\n\n## Action items\n- [ ] Email the team\n- [x] Draft the deck\n\nSee also [[Backlog|the backlog]] and #idea.";
        let analysis = MlService::analyze("meetings/weekly.md", content, None, "classical");

        assert_eq!(analysis.file_path, "meetings/weekly.md");
        assert_eq!(analysis.title.as_deref(), Some("Weekly Sync"));
        assert_eq!(analysis.sections.len(), 2);
        assert_eq!(analysis.tier, "classical");

        assert!(analysis.inline_tags.contains(&"project/ui".to_string()));
        assert!(analysis.inline_tags.contains(&"idea".to_string()));

        assert!(analysis.wiki_links.contains(&"Roadmap".to_string()));
        // Alias and heading suffixes are stripped to the bare target.
        assert!(analysis.wiki_links.contains(&"Backlog".to_string()));

        assert_eq!(analysis.tasks.len(), 2);
        let done: Vec<bool> = analysis.tasks.iter().map(|t| t.done).collect();
        assert_eq!(done, vec![false, true]);
        assert_eq!(analysis.tasks[0].text, "Email the team");

        assert!(analysis.word_count > 0);
        // Keyphrases are populated under the classical tier (see keyphrases_gated_by_tier).
    }

    #[test]
    fn analyze_ignores_headings_and_tags_in_code_fences() {
        let content =
            "# Real Heading\n\n```\n# fake heading\n#not-a-tag\n```\n\nBody with #realtag.";
        let analysis = MlService::analyze("notes/code.md", content, None, "heuristic");

        assert_eq!(analysis.sections.len(), 1);
        assert_eq!(analysis.sections[0].title, "Real Heading");
        assert!(analysis.inline_tags.contains(&"realtag".to_string()));
        assert!(!analysis.inline_tags.contains(&"not-a-tag".to_string()));
    }

    #[test]
    fn analyze_prefers_frontmatter_title_then_falls_back() {
        let fm = serde_json::json!({ "title": "Front Title" });
        let with_fm = MlService::analyze("a/b.md", "# Heading Title\n\nx", Some(&fm), "classical");
        assert_eq!(with_fm.title.as_deref(), Some("Front Title"));

        let no_heading = MlService::analyze("a/my-note.md", "just prose, no heading", None, "classical");
        assert_eq!(no_heading.title.as_deref(), Some("my-note"));
    }

    #[test]
    fn phrase_to_tag_normalizes_and_rejects() {
        assert_eq!(
            MlService::phrase_to_tag("Project Roadmap").as_deref(),
            Some("project-roadmap")
        );
        assert_eq!(
            MlService::phrase_to_tag("  Q3   OKRs!! ").as_deref(),
            Some("q3-okrs")
        );
        // No letters -> rejected.
        assert_eq!(MlService::phrase_to_tag("2024"), None);
        // Too short -> rejected.
        assert_eq!(MlService::phrase_to_tag("a"), None);
    }

    #[test]
    fn keyphrases_gated_by_tier() {
        let content = "Quarterly revenue growth strategy and market expansion planning for the new product line.";
        assert!(MlService::keyphrases_for_tier(content, "heuristic", 5).is_empty());
        let classical = MlService::keyphrases_for_tier(content, "classical", 5);
        assert!(!classical.is_empty());
        // Relevance is normalized into (0, 1].
        assert!(classical.iter().all(|k| k.score > 0.0 && k.score <= 1.0));
    }

    #[test]
    fn suggestions_include_keyphrase_tags_with_source() {
        let content = "Quarterly revenue growth strategy and market expansion planning for the new product line. Revenue growth strategy is the focus.";
        let keyphrases = MlService::extract_keyphrases(content, 8);
        let suggestions =
            MlService::suggest_organization("inbox/strategy.md", content, None, &keyphrases, 12);

        let keyphrase_tags: Vec<&str> = suggestions
            .suggestions
            .iter()
            .filter(|s| s.source.as_deref() == Some("keyphrase"))
            .filter_map(|s| s.tag.as_deref())
            .collect();
        assert!(
            !keyphrase_tags.is_empty(),
            "expected at least one keyphrase-derived tag, got {:?}",
            suggestions.suggestions
        );
        // Keyphrase tags are tag-shaped (no spaces) and confidence-banded below rules.
        assert!(suggestions
            .suggestions
            .iter()
            .filter(|s| s.source.as_deref() == Some("keyphrase"))
            .all(|s| !s.tag.as_deref().unwrap_or("").contains(' ')
                && s.confidence >= 0.50
                && s.confidence <= 0.74));
    }

    #[test]
    fn keyphrase_tags_skip_existing_tags() {
        let content = "Revenue growth strategy and revenue growth planning.";
        let keyphrases = MlService::extract_keyphrases(content, 8);
        let fm = serde_json::json!({ "tags": ["revenue-growth-strategy"] });
        let suggestions = MlService::suggest_organization(
            "n.md",
            content,
            Some(&fm),
            &keyphrases,
            12,
        );
        // The already-present tag must not be re-suggested.
        assert!(!suggestions
            .suggestions
            .iter()
            .any(|s| s.tag.as_deref() == Some("revenue-growth-strategy")));
    }

    #[test]
    fn slugify_schemes() {
        assert_eq!(MlService::slugify_kebab("  My Great Note! "), "my-great-note");
        assert_eq!(MlService::slugify_kebab("a/b:c"), "a-b-c");
        assert_eq!(
            MlService::slugify_title_case("my great note"),
            "My Great Note"
        );
        // Filename-invalid characters are dropped in title case.
        assert_eq!(
            MlService::slugify_title_case("plan: phase/one"),
            "Plan Phaseone"
        );
    }

    #[test]
    fn suggest_rename_kebab_from_h1() {
        let content = "# Quarterly Planning Notes\n\nbody";
        let proposed =
            MlService::suggest_rename("inbox/Untitled 1.md", content, None, &[], "kebab-case");
        assert_eq!(proposed.as_deref(), Some("quarterly-planning-notes.md"));
    }

    #[test]
    fn suggest_rename_prefers_frontmatter_title_and_schemes() {
        let fm = serde_json::json!({ "title": "Budget Review", "date": "2026-06-24T10:00:00Z", "category": "Finance" });
        let c = "# Ignored Heading\n\nbody";

        assert_eq!(
            MlService::suggest_rename("n.md", c, Some(&fm), &[], "kebab-case").as_deref(),
            Some("budget-review.md")
        );
        assert_eq!(
            MlService::suggest_rename("n.md", c, Some(&fm), &[], "title-case").as_deref(),
            Some("Budget Review.md")
        );
        assert_eq!(
            MlService::suggest_rename("n.md", c, Some(&fm), &[], "date-prefixed").as_deref(),
            Some("2026-06-24-budget-review.md")
        );
        assert_eq!(
            MlService::suggest_rename("n.md", c, Some(&fm), &[], "category-slug").as_deref(),
            Some("finance-budget-review.md")
        );
    }

    #[test]
    fn suggest_rename_noop_when_already_canonical() {
        let content = "# My Note\n\nbody";
        // Current stem already equals the kebab slug -> no suggestion.
        assert!(
            MlService::suggest_rename("dir/my-note.md", content, None, &[], "kebab-case").is_none()
        );
    }

    #[test]
    fn suggest_rename_falls_back_to_keyphrases_when_no_title() {
        let content = "no heading here, just prose about revenue forecasting models";
        let keyphrases = MlService::extract_keyphrases(content, 5);
        let proposed =
            MlService::suggest_rename("inbox/note.md", content, None, &keyphrases, "kebab-case");
        assert!(proposed.is_some());
        assert!(proposed.unwrap().ends_with(".md"));
    }

    #[test]
    fn analyze_skips_issue_refs_and_dedupes_tags() {
        let content = "Fixes #123 and relates to #bug. Also #bug again and #bug/regression.";
        let analysis = MlService::analyze("n.md", content, None, "classical");

        assert!(!analysis.inline_tags.iter().any(|t| t == "123"));
        // #bug appears twice but is deduped.
        assert_eq!(analysis.inline_tags.iter().filter(|t| *t == "bug").count(), 1);
        assert!(analysis.inline_tags.contains(&"bug/regression".to_string()));
    }
}
