//! Typst → Markdown conversion (#141), over the `typst-syntax` tree.
//!
//! The inverse of `typst_convert`. It's lossy by nature: Typst is also a
//! programming language, and most of that has no Markdown form. The rules:
//! - Markup converts: headings, strong/emphasis, lists (bulleted, numbered,
//!   terms), raw code, links, images, block quotes, footnotes, tables
//!   (merged cells become `^^` / `<<`), rules, line breaks.
//! - Librarium's conventions map back: `#link("librarium://note/X")[..]`
//!   becomes `[[X]]` (or `[[X|label]]`), and the
//!   `#metadata(..) <frontmatter>` block becomes YAML frontmatter.
//! - Math stays in Typst syntax between `$` signs (warned): there is no
//!   Typst → LaTeX translation.
//! - Code with no Markdown form (`#set`, `#show`, `#let`, `#import`,
//!   unknown functions) is kept as an HTML comment, `<!-- typst: … -->`, so
//!   it's invisible but not lost (warned).
//!
//! Warnings carry the line in the Typst source.

use serde_json::Value;
use typst_syntax::ast::{self, Arg, Expr};
use typst_syntax::{LinkedNode, SyntaxKind};

use super::typst_convert::{ConversionWarning, TypstConversion};

const NOTE_LINK_PREFIX: &str = "librarium://note/";

/// Convert a Typst note (metadata block included) to Markdown.
pub fn typst_to_markdown(source: &str) -> TypstConversion {
    let (frontmatter, body) = librarium_core::typst_frontmatter::parse(source);
    // The block is normally at the top; shift warning lines past it.
    let line_offset = if source.ends_with(&body) {
        source[..source.len() - body.len()].matches('\n').count()
    } else {
        0
    };
    let root = typst_syntax::parse(&body);
    let mut conv = Converter {
        source: &body,
        line_offset,
        warnings: Vec::new(),
        footnotes: Vec::new(),
    };
    let linked = LinkedNode::new(&root);
    let mut markdown = conv.markup(&linked);

    if !conv.footnotes.is_empty() {
        markdown = markdown.trim_end().to_string();
        markdown.push_str("\n\n");
        for (i, def) in conv.footnotes.iter().enumerate() {
            markdown.push_str(&format!(
                "[^{}]: {}\n",
                i + 1,
                def.trim().replace('\n', " ")
            ));
        }
    }
    let mut markdown = collapse_blank_lines(markdown.trim()).to_string() + "\n";
    if let Some(Value::Object(map)) = &frontmatter {
        if !map.is_empty() {
            let yaml = serde_yaml::to_string(frontmatter.as_ref().unwrap()).unwrap_or_default();
            markdown = format!("---\n{yaml}---\n\n{markdown}");
        }
    }
    TypstConversion {
        typst: markdown,
        warnings: conv.warnings,
    }
}

struct Cell {
    text: String,
    row_span: usize,
    col_span: usize,
}

struct Converter<'a> {
    source: &'a str,
    line_offset: usize,
    warnings: Vec<ConversionWarning>,
    footnotes: Vec<String>,
}

impl Converter<'_> {
    fn line(&self, node: &LinkedNode) -> Option<usize> {
        Some(self.line_offset + self.source[..node.offset()].matches('\n').count() + 1)
    }

    fn warn(&mut self, node: &LinkedNode, message: impl Into<String>) {
        let line = self.line(node);
        self.warnings.push(ConversionWarning {
            line,
            message: message.into(),
        });
    }

    /// Convert a markup sequence (the children of a Markup node).
    fn markup(&mut self, node: &LinkedNode) -> String {
        let children: Vec<LinkedNode> = node.children().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < children.len() {
            let child = &children[i];
            if child.kind() == SyntaxKind::Hash {
                // `#` + an embedded expression.
                if let Some(expr) = children.get(i + 1) {
                    let at_line_start = out.is_empty() || out.ends_with('\n');
                    let converted = self.code(expr, at_line_start);
                    out.push_str(&converted);
                    i += 2;
                    continue;
                }
            }
            let at_line_start = out.is_empty() || out.ends_with('\n');
            let converted = self.node(child, at_line_start);
            out.push_str(&converted);
            i += 1;
        }
        out
    }

    fn node(&mut self, node: &LinkedNode, at_line_start: bool) -> String {
        let raw = node.get();
        match node.kind() {
            SyntaxKind::Text => escape_markdown(raw.leaf_text(), at_line_start),
            SyntaxKind::Space => {
                let n = raw.leaf_text().matches('\n').count();
                if n == 0 {
                    " ".into()
                } else {
                    "\n".repeat(n.min(2))
                }
            }
            SyntaxKind::Parbreak => "\n\n".into(),
            SyntaxKind::Linebreak => "\\\n".into(),
            SyntaxKind::Escape => {
                let c = raw.leaf_text().trim_start_matches('\\');
                let c = match c.strip_prefix("u{").and_then(|h| h.strip_suffix('}')) {
                    Some(hex) => u32::from_str_radix(hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .map(String::from)
                        .unwrap_or_default(),
                    None => c.to_string(),
                };
                escape_markdown(&c, at_line_start)
            }
            SyntaxKind::Shorthand => match raw.leaf_text().as_str() {
                "--" => "–".into(),
                "---" => "—".into(),
                "..." => "…".into(),
                "~" => "\u{a0}".into(),
                _ => String::new(),
            },
            SyntaxKind::SmartQuote => raw.leaf_text().to_string(),
            SyntaxKind::Strong => format!("**{}**", self.body_of(node).trim()),
            SyntaxKind::Emph => format!("*{}*", self.body_of(node).trim()),
            SyntaxKind::Raw => self.raw(node),
            SyntaxKind::Link => raw.leaf_text().to_string(),
            SyntaxKind::Heading => {
                let depth = raw
                    .cast::<ast::Heading>()
                    .map(|h| h.depth().get())
                    .unwrap_or(1);
                format!(
                    "{} {}\n",
                    "#".repeat(depth.min(6)),
                    self.body_of(node).trim().replace('\n', " ")
                )
            }
            SyntaxKind::ListItem => self.item(node, "-".into()),
            SyntaxKind::EnumItem => {
                let n = raw
                    .cast::<ast::EnumItem>()
                    .and_then(|e| e.number())
                    .unwrap_or(1);
                self.item(node, format!("{n}."))
            }
            SyntaxKind::TermItem => {
                let parts: Vec<LinkedNode> = node
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::Markup)
                    .collect();
                let term = parts.first().map(|t| self.markup(t)).unwrap_or_default();
                let desc = parts.get(1).map(|d| self.markup(d)).unwrap_or_default();
                format!("**{}**: {}", term.trim(), desc.trim())
            }
            SyntaxKind::Equation => {
                self.warn(node, "Math kept in Typst syntax (Markdown uses LaTeX math)");
                let text = raw.full_text();
                let block = raw
                    .cast::<ast::Equation>()
                    .map(|e| e.block())
                    .unwrap_or(false);
                let inner = text.trim_matches('$').trim();
                if block {
                    format!("$$\n{inner}\n$$")
                } else {
                    format!("${inner}$")
                }
            }
            SyntaxKind::Label => {
                self.warn(
                    node,
                    format!(
                        "Label {} has no Markdown equivalent; dropped",
                        raw.full_text()
                    ),
                );
                String::new()
            }
            SyntaxKind::Ref => {
                self.warn(
                    node,
                    format!(
                        "Reference {} has no Markdown equivalent; kept as text",
                        raw.full_text()
                    ),
                );
                raw.full_text().to_string()
            }
            SyntaxKind::LineComment => {
                format!(
                    "<!-- {} -->",
                    raw.leaf_text().trim_start_matches('/').trim()
                )
            }
            SyntaxKind::BlockComment => {
                let t = raw.leaf_text();
                format!(
                    "<!-- {} -->",
                    t.trim_start_matches("/*").trim_end_matches("*/").trim()
                )
            }
            SyntaxKind::Markup => self.markup(node),
            _ => {
                if raw.children().len() > 0 {
                    self.markup(node)
                } else {
                    escape_markdown(raw.leaf_text(), at_line_start)
                }
            }
        }
    }

    /// The converted body markup of a Strong/Emph/Heading/Item node.
    fn body_of(&mut self, node: &LinkedNode) -> String {
        match node.children().find(|c| c.kind() == SyntaxKind::Markup) {
            Some(markup) => self.markup(&markup),
            None => String::new(),
        }
    }

    fn item(&mut self, node: &LinkedNode, marker: String) -> String {
        let body = self.body_of(node);
        let indent = " ".repeat(marker.len() + 1);
        let body = body.trim_end().replace('\n', &format!("\n{indent}"));
        let body: Vec<&str> = body.lines().map(str::trim_end).collect();
        // No trailing newline: the whitespace after the item supplies it (a
        // second one would make the list loose).
        format!("{marker} {}", body.join("\n").trim_start())
    }

    fn raw(&mut self, node: &LinkedNode) -> String {
        let raw = node.get();
        let Some(r) = raw.cast::<ast::Raw>() else {
            return String::new();
        };
        let text: Vec<String> = r.lines().map(|l| l.get().to_string()).collect();
        let text = text.join("\n");
        let block = raw
            .children()
            .next()
            .map(|d| d.leaf_text().len() >= 3)
            .unwrap_or(false);
        let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
        if block {
            let fence = "`".repeat(longest.max(2) + 1);
            let lang = r.lang().map(|l| l.get().to_string()).unwrap_or_default();
            format!("{fence}{lang}\n{text}\n{fence}")
        } else {
            let fence = "`".repeat(longest + 1);
            let pad = if text.starts_with('`') || text.ends_with('`') {
                " "
            } else {
                ""
            };
            format!("{fence}{pad}{text}{pad}{fence}")
        }
    }

    /// An embedded code expression (after `#`).
    fn code(&mut self, node: &LinkedNode, at_line_start: bool) -> String {
        let raw = node.get();
        if let Some(call) = raw.cast::<ast::FuncCall>() {
            if let Some(converted) = self.call(node, call, at_line_start) {
                return converted;
            }
        }
        let source = raw.full_text();
        let what = match node.kind() {
            SyntaxKind::SetRule => "A `#set` rule",
            SyntaxKind::ShowRule => "A `#show` rule",
            SyntaxKind::LetBinding => "A `#let` definition",
            SyntaxKind::ModuleImport => "An `#import`",
            SyntaxKind::ModuleInclude => "An `#include` (the included file isn't inlined)",
            _ => "Typst code",
        };
        self.warn(
            node,
            format!(
                "{what} has no Markdown equivalent; kept as a comment: #{}",
                truncate(&source, 50)
            ),
        );
        format!("<!-- typst: #{} -->", source.replace("--", "- -"))
    }

    fn call(
        &mut self,
        node: &LinkedNode,
        call: ast::FuncCall,
        at_line_start: bool,
    ) -> Option<String> {
        let name = match call.callee() {
            Expr::Ident(i) => i.get().to_string(),
            Expr::FieldAccess(fa) => match fa.target() {
                Expr::Ident(t) => format!("{}.{}", t.get(), fa.field().get()),
                _ => return None,
            },
            _ => return None,
        };
        let args: Vec<Arg> = call.args().items().collect();
        let str_arg = |i: usize| -> Option<String> {
            args.iter()
                .filter_map(|a| match a {
                    Arg::Pos(e) => Some(e),
                    _ => None,
                })
                .nth(i)
                .and_then(|e| match e {
                    Expr::Str(s) => Some(s.get().to_string()),
                    _ => None,
                })
        };
        let named = |key: &str| -> Option<Expr> {
            args.iter().find_map(|a| match a {
                Arg::Named(n) if n.name().get() == key => Some(n.expr()),
                _ => None,
            })
        };
        let content = self.content_args(node);

        Some(match name.as_str() {
            "link" => {
                let url = str_arg(0)?;
                let label = content
                    .first()
                    .map(|c| c.trim().to_string())
                    .unwrap_or_default();
                match url.strip_prefix(NOTE_LINK_PREFIX) {
                    Some(target) if label.is_empty() || label == target => format!("[[{target}]]"),
                    Some(target) => format!("[[{target}|{label}]]"),
                    None if label.is_empty() => format!("<{url}>"),
                    None => format!("[{label}]({})", url.replace(' ', "%20")),
                }
            }
            "image" => {
                let path = str_arg(0)?;
                let alt = match named("alt") {
                    Some(Expr::Str(s)) => s.get().to_string(),
                    _ => String::new(),
                };
                format!("![{alt}]({})", path.replace(' ', "%20"))
            }
            "strong" => format!("**{}**", content.first()?.trim()),
            "emph" => format!("*{}*", content.first()?.trim()),
            "strike" => format!("~~{}~~", content.first()?.trim()),
            "highlight" => format!("=={}==", content.first()?.trim()),
            "underline" | "smallcaps" | "text" | "box" | "align" | "block" => {
                self.warn(
                    node,
                    format!("#{name} formatting has no Markdown equivalent; kept the text only"),
                );
                content.join(" ")
            }
            "quote" => {
                let body = content.first()?.trim().to_string();
                let block = matches!(named("block"), Some(Expr::Bool(b)) if b.get());
                if block {
                    let quoted: Vec<String> = body
                        .lines()
                        .map(|l| format!("> {l}").trim_end().to_string())
                        .collect();
                    format!("{}\n", quoted.join("\n"))
                } else {
                    format!("“{body}”")
                }
            }
            "footnote" => {
                self.footnotes.push(content.first()?.to_string());
                format!("[^{}]", self.footnotes.len())
            }
            "line" => "\n---\n".into(),
            "pagebreak" => "<!-- pagebreak -->".into(),
            "raw" => {
                let text = str_arg(0)?;
                let lang = match named("lang") {
                    Some(Expr::Str(s)) => s.get().to_string(),
                    _ => String::new(),
                };
                if matches!(named("block"), Some(Expr::Bool(b)) if b.get()) {
                    format!("```{lang}\n{text}\n```")
                } else {
                    format!("`{text}`")
                }
            }
            "table" => self.table(node, at_line_start)?,
            _ => return None,
        })
    }

    /// The converted `[..]` content arguments of a call, in order.
    fn content_args(&mut self, call: &LinkedNode) -> Vec<String> {
        let Some(args) = call.children().find(|c| c.kind() == SyntaxKind::Args) else {
            return vec![];
        };
        let blocks: Vec<LinkedNode> = args
            .children()
            .filter(|c| c.kind() == SyntaxKind::ContentBlock)
            .collect();
        blocks
            .iter()
            .map(
                |b| match b.children().find(|c| c.kind() == SyntaxKind::Markup) {
                    Some(m) => self.markup(&m),
                    None => String::new(),
                },
            )
            .collect()
    }

    fn table_cell(&mut self, n: &LinkedNode, e: Expr) -> Option<Cell> {
        match e {
            Expr::ContentBlock(_) => Some(Cell {
                text: self.cell_text(n),
                row_span: 1,
                col_span: 1,
            }),
            Expr::FuncCall(c) => {
                if !matches!(c.callee(), Expr::FieldAccess(fa) if fa.field().get() == "cell") {
                    return None;
                }
                let span_of = |key: &str| {
                    c.args().items().find_map(|a| match a {
                        Arg::Named(nm) if nm.name().get() == key => match nm.expr() {
                            Expr::Int(i) => Some(i.get().max(1) as usize),
                            _ => None,
                        },
                        _ => None,
                    })
                };
                Some(Cell {
                    text: self.content_args(n).join(" ").trim().replace('\n', " "),
                    row_span: span_of("rowspan").unwrap_or(1),
                    col_span: span_of("colspan").unwrap_or(1),
                })
            }
            _ => None,
        }
    }

    fn table(&mut self, node: &LinkedNode, at_line_start: bool) -> Option<String> {
        let mut columns: usize = 0;
        let mut aligns: Vec<&str> = Vec::new();
        let mut header: Vec<Cell> = Vec::new();
        let mut body: Vec<Cell> = Vec::new();

        let args_node = node.children().find(|c| c.kind() == SyntaxKind::Args)?;
        for arg_node in args_node.children() {
            if arg_node.kind() == SyntaxKind::Named {
                let n = arg_node.get().cast::<ast::Named>()?;
                match n.name().get().as_str() {
                    "columns" => {
                        columns = match n.expr() {
                            Expr::Int(i) => i.get().max(1) as usize,
                            Expr::Array(a) => a.items().count(),
                            _ => 0,
                        }
                    }
                    "align" => {
                        if let Expr::Array(a) = n.expr() {
                            aligns = a
                                .items()
                                .map(|it| match it {
                                    ast::ArrayItem::Pos(Expr::Ident(i)) => match i.get().as_str() {
                                        "center" => ":-:",
                                        "right" => "--:",
                                        _ => ":--",
                                    },
                                    _ => "---",
                                })
                                .collect();
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if arg_node.kind() == SyntaxKind::Spread {
                return None;
            }
            let Some(expr) = arg_node.get().cast::<Expr>() else {
                continue;
            };
            let is_header = matches!(expr, Expr::FuncCall(c)
                if matches!(c.callee(), Expr::FieldAccess(fa) if fa.field().get() == "header"));
            if is_header {
                let hargs = arg_node.children().find(|c| c.kind() == SyntaxKind::Args)?;
                for h in hargs.children() {
                    let Some(he) = h.get().cast::<Expr>() else {
                        continue;
                    };
                    if let Some(cell) = self.table_cell(&h, he) {
                        header.push(cell);
                    }
                }
            } else {
                match self.table_cell(&arg_node, expr) {
                    Some(cell) => body.push(cell),
                    None => self.warn(
                        &arg_node,
                        "A table cell that isn't plain content was left out",
                    ),
                }
            }
        }
        if columns == 0 {
            return None;
        }

        // Lay the cells into a grid, filling spans with merge markers.
        let place = |cells: &[Cell]| -> Vec<Vec<String>> {
            let mut grid: Vec<Vec<Option<String>>> = Vec::new();
            let (mut r, mut c) = (0usize, 0usize);
            for cell in cells {
                loop {
                    if grid.len() <= r {
                        grid.push(vec![None; columns]);
                    }
                    if c >= columns {
                        r += 1;
                        c = 0;
                        continue;
                    }
                    if grid[r][c].is_none() {
                        break;
                    }
                    c += 1;
                }
                let cs = cell.col_span.min(columns - c);
                for dr in 0..cell.row_span {
                    while grid.len() <= r + dr {
                        grid.push(vec![None; columns]);
                    }
                    for dc in 0..cs {
                        let marker = match (dr, dc) {
                            (0, 0) => cell.text.clone(),
                            (_, 0) => "^^".into(),
                            _ => "<<".into(),
                        };
                        grid[r + dr][c + dc] = Some(marker);
                    }
                }
                c += cs;
            }
            grid.into_iter()
                .map(|row| row.into_iter().map(Option::unwrap_or_default).collect())
                .collect()
        };
        let header_rows = place(&header);
        let body_rows = place(&body);
        let (head, rest): (Vec<String>, Vec<Vec<String>>) = match header_rows.into_iter().next() {
            Some(h) => (h, body_rows),
            None => (vec![String::new(); columns], body_rows),
        };
        let row = |cells: &[String]| {
            format!(
                "| {} |",
                cells
                    .iter()
                    .map(|c| c.replace('|', "\\|"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        };
        let divider: Vec<String> = (0..columns)
            .map(|i| aligns.get(i).copied().unwrap_or("---").to_string())
            .collect();
        let mut out = String::new();
        if !at_line_start {
            out.push_str("\n\n");
        }
        out.push_str(&row(&head));
        out.push('\n');
        out.push_str(&format!("| {} |\n", divider.join(" | ")));
        for r in rest {
            out.push_str(&row(&r));
            out.push('\n');
        }
        Some(out)
    }

    fn cell_text(&mut self, node: &LinkedNode) -> String {
        match node.children().find(|c| c.kind() == SyntaxKind::Markup) {
            Some(m) => self.markup(&m).trim().replace('\n', " "),
            None => String::new(),
        }
    }
}

/// Escape characters Markdown would read as syntax.
fn escape_markdown(text: &str, at_line_start: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.chars().enumerate() {
        match c {
            '*' | '_' | '`' | '[' | ']' | '<' | '>' | '\\' | '|' => {
                out.push('\\');
                out.push(c);
            }
            '#' | '-' | '+' if i == 0 && at_line_start => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines > 2 {
                continue;
            }
        } else if c != ' ' || newlines == 0 {
            newlines = 0;
        }
        out.push(c);
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    let flat = s.replace('\n', " ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(typst: &str) -> String {
        typst_to_markdown(typst).typst
    }

    #[test]
    fn markup_converts() {
        let out = md("= Title\n\nSome *strong* and _emph_ text with `code`.\n\n== Sub\n\n- one\n- two\n  - nested\n+ first\n+ second\n/ Term: its definition\n");
        assert!(out.contains("# Title\n"), "{out}");
        assert!(
            out.contains("**strong**") && out.contains("*emph*") && out.contains("`code`"),
            "{out}"
        );
        assert!(out.contains("## Sub\n"), "{out}");
        assert!(out.contains("- one\n- two\n  - nested\n"), "{out}");
        assert!(out.contains("1. first\n1. second\n"), "{out}");
        assert!(out.contains("**Term**: its definition"), "{out}");
    }

    #[test]
    fn links_images_and_librarium_conventions() {
        let out = md("See #link(\"https://example.com\")[the site], #link(\"librarium://note/Roadmap\")[Roadmap], #link(\"librarium://note/Plan\")[the plan], #link(\"https://bare.example\") and #image(\"img/a b.png\", alt: \"A pic\").");
        assert!(out.contains("[the site](https://example.com)"), "{out}");
        assert!(out.contains("[[Roadmap]]"), "{out}");
        assert!(out.contains("[[Plan|the plan]]"), "{out}");
        assert!(out.contains("<https://bare.example>"), "{out}");
        assert!(out.contains("![A pic](img/a%20b.png)"), "{out}");
    }

    #[test]
    fn metadata_becomes_yaml_frontmatter() {
        let out =
            md("#metadata((title: \"Paper\", tags: (\"a\", \"b\"))) <frontmatter>\n\n= Paper\n");
        assert!(
            out.starts_with("---\ntitle: Paper\ntags:\n- a\n- b\n---\n\n# Paper\n"),
            "{out}"
        );
    }

    #[test]
    fn code_blocks_quotes_footnotes_and_rules() {
        let out = md("```rust\nfn main() {}\n```\n\n#quote(block: true)[Quoted *text*]\n\nClaim.#footnote[Source here.]\n\n#line(length: 100%)\n\n#strike[old] and #highlight[marked]");
        assert!(out.contains("```rust\nfn main() {}\n```"), "{out}");
        assert!(out.contains("> Quoted **text**"), "{out}");
        assert!(
            out.contains("Claim.[^1]") && out.contains("[^1]: Source here."),
            "{out}"
        );
        assert!(out.contains("\n---\n"), "{out}");
        assert!(
            out.contains("~~old~~") && out.contains("==marked=="),
            "{out}"
        );
    }

    #[test]
    fn tables_with_merged_cells_and_alignment() {
        let out = md("#table(\n  columns: 3,\n  align: (left, center, right),\n  table.header([Region], [Q1], [Q2]),\n  table.cell(rowspan: 2)[North], [10], [12],\n  [11], [13],\n  [Total], table.cell(colspan: 2)[46],\n)\n");
        assert!(out.contains("| Region | Q1 | Q2 |\n| :-- | :-: | --: |\n| North | 10 | 12 |\n| ^^ | 11 | 13 |\n| Total | 46 | << |"), "{out}");
    }

    #[test]
    fn code_and_math_are_kept_with_warnings() {
        let conv = typst_to_markdown(
            "#set page(margin: 2cm)\n= T\n\nMath $x^2$ here.\n#let x = 1\n#unknownfn(3)\n",
        );
        assert!(
            conv.typst
                .contains("<!-- typst: #set page(margin: 2cm) -->"),
            "{}",
            conv.typst
        );
        assert!(conv.typst.contains("$x^2$"), "{}", conv.typst);
        assert!(
            conv.typst.contains("<!-- typst: #let x = 1 -->"),
            "{}",
            conv.typst
        );
        assert!(
            conv.typst.contains("<!-- typst: #unknownfn(3) -->"),
            "{}",
            conv.typst
        );
        let lines: Vec<(Option<usize>, &str)> = conv
            .warnings
            .iter()
            .map(|w| (w.line, w.message.as_str()))
            .collect();
        assert!(
            lines
                .iter()
                .any(|(l, m)| *l == Some(1) && m.contains("`#set` rule")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|(l, m)| *l == Some(4) && m.contains("Math")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|(l, m)| *l == Some(5) && m.contains("`#let`")),
            "{lines:?}"
        );
    }

    #[test]
    fn warning_lines_account_for_the_metadata_block() {
        let conv =
            typst_to_markdown("#metadata((title: \"x\")) <frontmatter>\n\nText\n#set text(red)\n");
        assert_eq!(conv.warnings[0].line, Some(4), "{:?}", conv.warnings);
    }

    #[test]
    fn escapes_markdown_syntax_in_text() {
        let out = md("Literal \\* star, a \\_b, pipe | and <angle>");
        assert!(
            out.contains("\\*") && out.contains("\\_") && out.contains("\\|"),
            "{out}"
        );
    }

    /// Markdown → Typst → Markdown keeps a document's structure: every
    /// heading, link and code block of the repo's own docs survives.
    #[test]
    fn round_trips_the_repo_docs_structurally() {
        use pulldown_cmark::{Event, Options, Parser, Tag};
        fn structure(md: &str) -> (Vec<String>, usize, usize) {
            let mut headings = Vec::new();
            let (mut links, mut code) = (0, 0);
            let mut in_heading = false;
            let mut text = String::new();
            for ev in Parser::new_ext(md, Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES) {
                match ev {
                    Event::Start(Tag::Heading { .. }) => {
                        in_heading = true;
                        text.clear();
                    }
                    Event::End(pulldown_cmark::TagEnd::Heading(_)) => {
                        in_heading = false;
                        headings.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
                    }
                    Event::Text(t) | Event::Code(t) if in_heading => text.push_str(&t),
                    Event::Start(Tag::Link { .. }) => links += 1,
                    Event::Start(Tag::CodeBlock(_)) => code += 1,
                    _ => {}
                }
            }
            (headings, links, code)
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for doc in ["README.md", "docs/USER_GUIDE.md", "docs/DEPLOYMENT.md"] {
            let original = std::fs::read_to_string(root.join(doc)).unwrap();
            let typst = super::super::typst_convert::markdown_to_typst(&original).typst;
            let back = typst_to_markdown(&typst).typst;
            let (h1, l1, c1) = structure(&original);
            let (h2, l2, c2) = structure(&back);
            assert_eq!(h1, h2, "{doc}: headings differ");
            assert_eq!(l1, l2, "{doc}: link count differs");
            assert_eq!(c1, c2, "{doc}: code block count differs");
        }
    }
}
