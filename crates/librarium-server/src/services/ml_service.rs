use crate::models::{
    Keyphrase, NoteAnalysis, NoteOutlineResponse, NoteTask, OrganizationSuggestion,
    OrganizationSuggestionKind, OrganizationSuggestionsResponse, OutlineSection,
};
use crate::services::frontmatter_service;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashSet;
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
            keyphrases: Vec::<Keyphrase>::new(),
            tier: tier.to_string(),
            generated_at: Utc::now(),
        }
    }

    pub fn suggest_organization(
        file_path: &str,
        content: &str,
        frontmatter: Option<&Value>,
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
    fn suggestions_include_tag_and_folder_move() {
        let content =
            "# Sprint Meeting\n\nAgenda:\n- Discuss project roadmap\n- Action items and todo list";
        let suggestions =
            MlService::suggest_organization("inbox/sprint-meeting.md", content, None, 10);

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
        assert!(analysis.keyphrases.is_empty()); // populated by Tier 1 (LIB-056)
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
    fn analyze_skips_issue_refs_and_dedupes_tags() {
        let content = "Fixes #123 and relates to #bug. Also #bug again and #bug/regression.";
        let analysis = MlService::analyze("n.md", content, None, "classical");

        assert!(!analysis.inline_tags.iter().any(|t| t == "123"));
        // #bug appears twice but is deduped.
        assert_eq!(analysis.inline_tags.iter().filter(|t| *t == "bug").count(), 1);
        assert!(analysis.inline_tags.contains(&"bug/regression".to_string()));
    }
}
