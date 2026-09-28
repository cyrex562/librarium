//! Links between notes, Markdown and Typst alike (#144).
//!
//! Typst has no wiki-link syntax, so Librarium's convention is a plain
//! `#link(..)` whose target names a note:
//! - `#link("librarium://note/Target")[label]` resolves like `[[Target]]`
//!   (what "Convert to Typst" writes for wiki links), and
//! - `#link("notes/other.typ")` / `#link("other.md")`: a vault-relative or
//!   note-relative path to a note file.
//!
//! Markdown notes link to Typst notes with ordinary wiki links:
//! `[[paper.typ]]`, or `[[paper]]` (resolved by stem).
//!
//! [`BacklinkTarget`] is the backlink test shared by the server's
//! `/backlinks` route and the mobile client, so the two can't drift.

use typst_syntax::ast::{self, Arg, Expr};
use typst_syntax::{SyntaxKind, SyntaxNode};

/// Whether a file is a note that can hold links (Markdown or Typst).
pub fn is_note_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("typ"))
}

/// Scheme of note links in Typst notes.
pub const NOTE_LINK_PREFIX: &str = "librarium://note/";

/// The note links in Typst source, in order: `librarium://note/` targets
/// (prefix stripped) and relative paths to `.md`/`.typ` files. External
/// URLs, `mailto:` and in-document labels are not note links.
pub fn typst_note_links(source: &str) -> Vec<String> {
    let root = typst_syntax::parse(source);
    let mut links = Vec::new();
    collect(&root, &mut links);
    links
}

fn collect(node: &SyntaxNode, links: &mut Vec<String>) {
    if node.kind() == SyntaxKind::FuncCall {
        if let Some(call) = node.cast::<ast::FuncCall>() {
            if let Expr::Ident(ident) = call.callee() {
                if ident.get() == "link" {
                    let first = call.args().items().find_map(|arg| match arg {
                        Arg::Pos(Expr::Str(s)) => Some(s.get().to_string()),
                        _ => None,
                    });
                    if let Some(target) = first.and_then(|t| note_target(&t)) {
                        links.push(target);
                    }
                }
            }
        }
    }
    for child in node.children() {
        collect(child, links);
    }
}

/// `librarium://note/X` → `X`; a relative note path → itself; else `None`.
fn note_target(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix(NOTE_LINK_PREFIX) {
        return (!rest.is_empty()).then(|| rest.to_string());
    }
    if url.contains("://") || url.starts_with("mailto:") || url.starts_with('#') {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    let path = lower.split('#').next().unwrap_or("");
    (path.ends_with(".md") || path.ends_with(".typ")).then(|| url.to_string())
}

fn strip_note_ext(s: &str) -> &str {
    let lower = s.to_ascii_lowercase();
    if lower.ends_with(".md") {
        &s[..s.len() - 3]
    } else if lower.ends_with(".typ") {
        &s[..s.len() - 4]
    } else {
        s
    }
}

/// Collapse `a/./b/../c` and leading `/`; `..` above the root is dropped.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// "Does this note link to `target`?", for backlinks.
pub struct BacklinkTarget {
    path_lower: String,
    path_no_ext: String,
    stem_lower: String,
    wiki_stem: String,
}

impl BacklinkTarget {
    pub fn new(target_path: &str) -> Self {
        let target_path = target_path.trim().trim_start_matches('/');
        let stem = std::path::Path::new(target_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(target_path);
        Self {
            path_lower: target_path.to_lowercase(),
            path_no_ext: strip_note_ext(target_path).to_lowercase(),
            stem_lower: stem.to_lowercase(),
            wiki_stem: format!("[[{}]]", stem.to_lowercase()),
        }
    }

    /// Whether the note at `source_path` (vault-relative) with `content`
    /// links here. The target itself never counts.
    pub fn is_linked_from(&self, source_path: &str, content: &str) -> bool {
        if source_path.to_lowercase() == self.path_lower {
            return false;
        }
        if source_path.to_ascii_lowercase().ends_with(".typ") {
            let dir = source_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            return typst_note_links(content)
                .iter()
                .any(|link| self.matches_link(link, dir));
        }
        // Markdown: `[[stem]]` or a `(path)` link, as the backlinks route
        // has always matched.
        let lower = content.to_lowercase();
        lower.contains(&self.wiki_stem)
            || lower.contains(&format!("({})", self.path_lower))
            || lower.contains(&format!("({})", self.path_no_ext))
    }

    fn matches_link(&self, link: &str, source_dir: &str) -> bool {
        let link = link.split(['#', '|']).next().unwrap_or("").trim();
        let lower = link.to_lowercase();
        let no_ext = strip_note_ext(&lower);
        // By name, like a wiki link: `Target` or `Target.typ`.
        if !no_ext.contains('/') && no_ext == self.stem_lower {
            return true;
        }
        // By path: vault-relative, or relative to the linking note.
        let candidates = [
            normalize(no_ext),
            normalize(&format!("{source_dir}/{no_ext}").to_lowercase()),
        ];
        candidates.contains(&self.path_no_ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_note_links_from_typst() {
        let src = r#"
See #link("librarium://note/Roadmap")[the roadmap], #link("notes/other.typ")[other],
#link("../up.md"), #link("https://example.com")[web], #link("mailto:a@b.c"),
#link(<label>)[internal], and #link("image.png").
#let x = link("librarium://note/InCode")
"#;
        assert_eq!(
            typst_note_links(src),
            vec!["Roadmap", "notes/other.typ", "../up.md", "InCode"]
        );
    }

    #[test]
    fn markdown_backlinks_keep_their_old_rules_and_find_typst_targets() {
        let target = BacklinkTarget::new("papers/paper.typ");
        assert!(target.is_linked_from("a.md", "see [[paper]] here"));
        assert!(target.is_linked_from("a.md", "see [x](papers/paper.typ)"));
        assert!(target.is_linked_from("a.md", "see [x](papers/paper)"));
        assert!(!target.is_linked_from("a.md", "see [[other]]"));
        // A note doesn't backlink itself.
        assert!(!target.is_linked_from("papers/paper.typ", "#link(\"librarium://note/paper\")"));
    }

    #[test]
    fn typst_backlinks_by_name_and_by_path() {
        let md_target = BacklinkTarget::new("notes/Roadmap.md");
        assert!(md_target.is_linked_from("x.typ", "#link(\"librarium://note/Roadmap\")[r]"));
        assert!(md_target.is_linked_from("x.typ", "#link(\"librarium://note/roadmap#Goals\")[r]"));
        assert!(md_target.is_linked_from("x.typ", "#link(\"notes/Roadmap.md\")[r]"));
        assert!(md_target.is_linked_from("notes/sub/x.typ", "#link(\"../Roadmap.md\")[r]"));
        assert!(!md_target.is_linked_from("x.typ", "Roadmap mentioned, not linked"));
        assert!(!md_target.is_linked_from("x.typ", "#link(\"https://x.y/Roadmap.md\")"));

        let typ_target = BacklinkTarget::new("papers/draft.typ");
        assert!(typ_target.is_linked_from("papers/main.typ", "#link(\"draft.typ\")[d]"));
        assert!(!typ_target.is_linked_from("elsewhere/main.typ", "#link(\"other/draft.typ\")[d]"));
    }
}
