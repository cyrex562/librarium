//! Plain text of a Typst note, for full-text search (#143).
//!
//! Keeps what a reader sees as words: prose, headings, list items, raw
//! (code) text, math, and the markup inside content blocks (`#strong[..]`,
//! `#link(..)[..]`). Drops markup characters, comments, labels and code (
//! function calls, `#let`, `#set`, string arguments).
//!
//! **Every newline in the source is kept**, so the output has exactly as
//! many lines as the note and a search hit's line number points at the same
//! line in the editor (search results carry line numbers from the indexed
//! body).

use typst_syntax::{SyntaxKind, SyntaxNode};

/// Searchable plain text of Typst source; same line count as `source`.
pub fn typst_plain_text(source: &str) -> String {
    let root = typst_syntax::parse(source);
    let mut out = String::with_capacity(source.len());
    walk(&root, &mut out);
    out
}

fn newlines_of(node: &SyntaxNode, out: &mut String) {
    for _ in 0..node.full_text().matches('\n').count() {
        out.push('\n');
    }
}

fn walk(node: &SyntaxNode, out: &mut String) {
    match node.kind() {
        // Words.
        SyntaxKind::Text | SyntaxKind::MathText | SyntaxKind::MathIdent | SyntaxKind::Link => {
            out.push_str(node.leaf_text())
        }
        SyntaxKind::Space | SyntaxKind::Parbreak | SyntaxKind::Linebreak => {
            let text = node.full_text();
            if text.contains('\n') {
                newlines_of(node, out);
            } else {
                out.push(' ');
            }
        }
        // `\*` → `*`, `\u{1F600}` → the character.
        SyntaxKind::Escape => {
            let text = node.leaf_text();
            let escaped = text.trim_start_matches('\\');
            match escaped.strip_prefix("u{").and_then(|h| h.strip_suffix('}')) {
                Some(hex) => {
                    if let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                None => out.push_str(escaped),
            }
        }
        SyntaxKind::Shorthand => out.push(match node.leaf_text().as_str() {
            "--" => '–',
            "---" => '—',
            "..." => '…',
            _ => ' ',
        }),
        SyntaxKind::SmartQuote => out.push_str(node.leaf_text()),
        // Code blocks keep their text; the fences and language tag don't.
        SyntaxKind::Raw => {
            for child in node.children() {
                match child.kind() {
                    SyntaxKind::RawDelim | SyntaxKind::RawLang => newlines_of(child, out),
                    SyntaxKind::RawTrimmed => newlines_of(child, out),
                    _ => out.push_str(&child.full_text()),
                }
            }
        }
        // Markup, and the markup inside `[..]` content blocks, recurse.
        SyntaxKind::Markup
        | SyntaxKind::Strong
        | SyntaxKind::Emph
        | SyntaxKind::Heading
        | SyntaxKind::ListItem
        | SyntaxKind::EnumItem
        | SyntaxKind::TermItem
        | SyntaxKind::Equation
        | SyntaxKind::Math
        | SyntaxKind::MathAttach
        | SyntaxKind::MathFrac
        | SyntaxKind::MathRoot
        | SyntaxKind::MathDelimited
        | SyntaxKind::MathCall
        | SyntaxKind::MathArgs
        | SyntaxKind::ContentBlock => {
            for child in node.children() {
                walk(child, out);
            }
        }
        // Code: skip it, but look inside for content blocks (the `[text]` of
        // `#link("..")[text]`, `#emph[..]`, `#quote[..]`).
        SyntaxKind::FuncCall | SyntaxKind::Args | SyntaxKind::CodeBlock | SyntaxKind::Code => {
            // Broken code (a note mid-edit: `#call(` never closed) can swallow
            // the rest of the file; index its raw text rather than lose it.
            if node.diagnosis().errors {
                out.push_str(&node.full_text());
                return;
            }
            for child in node.children() {
                match child.kind() {
                    SyntaxKind::ContentBlock | SyntaxKind::FuncCall | SyntaxKind::Args => {
                        walk(child, out)
                    }
                    _ => newlines_of(child, out),
                }
            }
        }
        // Everything else (markers, delimiters, comments, labels, refs,
        // other code): nothing to read, but keep the line structure.
        _ => {
            if node.diagnosis().errors {
                out.push_str(&node.full_text());
                return;
            }
            if node.children().len() > 0 {
                let has_content = node
                    .children()
                    .any(|c| matches!(c.kind(), SyntaxKind::ContentBlock | SyntaxKind::FuncCall));
                if has_content {
                    for child in node.children() {
                        walk(child, out);
                    }
                    return;
                }
            }
            newlines_of(node, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn keeps_prose_and_drops_markup() {
        let src = "= Results\n\nThe *main* finding, in _short_: it works.\n\n- one\n+ two\n/ Term: definition\n";
        let text = typst_plain_text(src);
        assert_eq!(
            words(&text),
            "Results The main finding, in short: it works. one two Term definition"
        );
    }

    #[test]
    fn keeps_every_line() {
        let src = "#set page(margin: 2cm)\n\n= A\n\n/* block\ncomment */\ntext\n// line comment\n#let x = (\n  a: 1,\n)\n```rust\nfn main() {}\n```\nend";
        let text = typst_plain_text(src);
        assert_eq!(text.lines().count(), src.lines().count(), "{text:?}");
        let end_line = text.lines().position(|l| l.contains("end")).unwrap();
        assert_eq!(end_line, src.lines().position(|l| l == "end").unwrap());
        let code_line = text.lines().position(|l| l.contains("fn main")).unwrap();
        assert_eq!(
            code_line,
            src.lines().position(|l| l.contains("fn main")).unwrap()
        );
    }

    #[test]
    fn content_blocks_inside_code_are_text() {
        let text = typst_plain_text(
            "See #link(\"https://x.y\")[the site] and #strong[this], #emph[that].",
        );
        assert_eq!(words(&text), "See the site and this, that.");
    }

    #[test]
    fn code_comments_and_labels_are_not_text() {
        let text = typst_plain_text(
            "#metadata((title: \"Secret title\")) <frontmatter>\n#let hidden = \"nope\"\n// a comment\nVisible @ref.",
        );
        assert!(!text.contains("Secret"), "{text:?}");
        assert!(!text.contains("nope"));
        assert!(!text.contains("comment"));
        assert!(!text.contains("frontmatter"));
        assert!(text.contains("Visible"));
    }

    #[test]
    fn escapes_shorthands_math_and_raw() {
        let text = typst_plain_text("Costs \\$5 -- or \\#tag... Math $x + y$ and `inline code`.");
        assert!(text.contains("$5"), "{text:?}");
        assert!(text.contains("#tag"));
        assert!(text.contains('–'));
        assert!(text.contains('…'));
        assert!(text.contains("x") && text.contains("y"));
        assert!(text.contains("inline code"));
        assert!(!text.contains('`'));
    }

    #[test]
    fn invalid_source_still_yields_text() {
        let text = typst_plain_text("Unclosed *strong and #broken( call\nnext line words");
        assert!(text.contains("next line words"), "{text:?}");
    }
}
