//! Markdown → Typst conversion (#141), native over `pulldown-cmark`.
//!
//! Used by "Convert to Typst" and by PDF export of Markdown notes (#140).
//! Produces Typst that compiles on its own (no packages, no prelude), plus a
//! list of warnings for anything that couldn't be carried over faithfully,
//! so the user sees what was lost instead of it vanishing silently.
//!
//! Librarium/Obsidian-specific mappings:
//! - YAML frontmatter → `#metadata((..)) <frontmatter>`, and a `title` also
//!   sets `#set document(title: ..)` (the PDF's title).
//! - `[[Note]]` / `[[Note|Label]]` → `#link("librarium://note/Note")[Label]`.
//! - `![[image.png]]` → `#image("image.png")`; embedding another note has no
//!   Typst equivalent and becomes a link (warned).
//! - `#tag` stays as visible text (escaped: `#` starts code in Typst).
//! - `==highlight==` → `#highlight[..]`.
//! - `^^` / `<<` merged table cells → `table.cell(rowspan:/colspan:)`.
//! - LaTeX math → Typst math via `mitex`, accepted only when the result uses
//!   plain Typst math names; otherwise kept as LaTeX source code (warned).
//! - Raw HTML → kept as literal code (warned).

use std::collections::HashMap;
use std::sync::LazyLock;

use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd,
};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::services::frontmatter_service;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConversionWarning {
    /// 1-based line in the Markdown source, when known.
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypstConversion {
    pub typst: String,
    pub warnings: Vec<ConversionWarning>,
}

/// Convert a Markdown note (frontmatter included) to Typst.
pub fn markdown_to_typst(markdown: &str) -> TypstConversion {
    Converter::new(markdown).run()
}

// ── Obsidian syntax inside text ─────────────────────────────────────────────

static WIKI_OR_HIGHLIGHT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(!?)\[\[([^\[\]|]+)(?:\|([^\[\]]*))?\]\]|==([^=\n]+)==").unwrap()
});

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg", "webp"];

// ── Converter ───────────────────────────────────────────────────────────────

struct ListState {
    /// `Some(n)`: ordered, next item number; `None`: bulleted.
    next: Option<u64>,
}

struct TableState {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<String>>,
    header_rows: usize,
    in_head: bool,
}

struct Converter<'a> {
    source: &'a str,
    body: String,
    /// Line number (0-based) where `body` starts in `source`.
    body_line_offset: usize,
    /// Stack of output buffers; nested constructs capture into their own.
    out: Vec<String>,
    warnings: Vec<ConversionWarning>,
    lists: Vec<ListState>,
    tables: Vec<TableState>,
    /// Pending plain text (pulldown splits text around `[`, `]`, …; wiki
    /// links must be matched on the joined text).
    text: String,
    text_line: Option<usize>,
    in_code_block: bool,
    code_fence_lang: String,
    code_buf: String,
    link_dest: Vec<(String, LinkType)>,
    image_dest: Vec<String>,
    footnotes: HashMap<String, String>,
    footnote_refs: Vec<(String, Option<usize>)>,
    current_footnote: Option<String>,
    current_line: Option<usize>,
}

impl<'a> Converter<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            body: String::new(),
            body_line_offset: 0,
            out: vec![String::new()],
            warnings: Vec::new(),
            lists: Vec::new(),
            tables: Vec::new(),
            text: String::new(),
            text_line: None,
            in_code_block: false,
            code_fence_lang: String::new(),
            code_buf: String::new(),
            link_dest: Vec::new(),
            image_dest: Vec::new(),
            footnotes: HashMap::new(),
            footnote_refs: Vec::new(),
            current_footnote: None,
            current_line: None,
        }
    }

    fn warn(&mut self, line: Option<usize>, message: impl Into<String>) {
        self.warnings.push(ConversionWarning {
            line,
            message: message.into(),
        });
    }

    fn push(&mut self, s: &str) {
        self.out.last_mut().unwrap().push_str(s);
    }

    fn begin_capture(&mut self) {
        self.flush_text();
        self.out.push(String::new());
    }

    fn end_capture(&mut self) -> String {
        self.flush_text();
        self.out.pop().unwrap_or_default()
    }

    /// Ensure the current buffer ends at the start of a line (block
    /// boundary), without piling up blank lines.
    fn block_break(&mut self) {
        self.flush_text();
        let buf = self.out.last_mut().unwrap();
        let trimmed = buf.trim_end_matches([' ', '\t']).len();
        buf.truncate(trimmed);
        if buf.is_empty() {
            return;
        }
        while !buf.ends_with("\n\n") {
            buf.push('\n');
        }
    }

    fn run(mut self) -> TypstConversion {
        let mut prelude = String::new();
        match frontmatter_service::parse_frontmatter(self.source) {
            Ok((Some(front), body)) => {
                self.body_line_offset = self.source.lines().count() - body.lines().count();
                self.body = body;
                if let Some(title) = front.get("title").and_then(Value::as_str) {
                    prelude.push_str(&format!("#set document(title: {})\n", typst_string(title)));
                }
                prelude.push_str(&format!(
                    "#metadata({}) <frontmatter>\n\n",
                    typst_value(&front)
                ));
            }
            Ok((None, body)) => self.body = body,
            Err(_) => {
                self.warn(
                    Some(1),
                    "The frontmatter isn't valid YAML, so it was kept as text",
                );
                self.body = self.source.to_string();
            }
        }

        let body = std::mem::take(&mut self.body);
        let opts = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_MATH;
        let events: Vec<(Event, std::ops::Range<usize>)> =
            Parser::new_ext(&body, opts).into_offset_iter().collect();
        for (event, range) in events {
            self.current_line =
                Some(self.body_line_offset + body[..range.start].matches('\n').count() + 1);
            self.event(event);
        }
        self.flush_text();

        let mut typst = self.out.pop().unwrap_or_default();
        // Footnotes: definitions can come after their references.
        for (label, line) in std::mem::take(&mut self.footnote_refs) {
            let placeholder = footnote_placeholder(&label);
            match self.footnotes.get(&label) {
                Some(def) => {
                    typst = typst.replacen(&placeholder, &format!("#footnote[{}]", def.trim()), 1)
                }
                None => {
                    self.warn(
                        line,
                        format!("Footnote [^{label}] has no definition; kept as text"),
                    );
                    typst = typst.replacen(
                        &placeholder,
                        &escape_text(&format!("[^{label}]"), false),
                        1,
                    );
                }
            }
        }
        let typst = format!("{prelude}{}\n", typst.trim_end());
        TypstConversion {
            typst,
            warnings: self.warnings,
        }
    }

    fn event(&mut self, event: Event) {
        if self.in_code_block {
            match event {
                Event::Text(t) => self.code_buf.push_str(&t),
                Event::End(TagEnd::CodeBlock) => self.end_code_block(),
                _ => {}
            }
            return;
        }
        match event {
            Event::Text(t) => {
                if self.text.is_empty() {
                    self.text_line = self.current_line;
                }
                self.text.push_str(&t);
            }
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Code(code) => {
                self.flush_text();
                self.push(&inline_raw(&code));
            }
            Event::InlineMath(tex) => {
                self.flush_text();
                let math = self.math(&tex, false);
                self.push(&math);
            }
            Event::DisplayMath(tex) => {
                self.flush_text();
                let math = self.math(&tex, true);
                self.push(&math);
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                self.flush_text();
                let trimmed = html.trim();
                if trimmed.starts_with("<!--") {
                    // Comments: nothing visible to lose.
                    return;
                }
                if trimmed == "<br>" || trimmed == "<br/>" || trimmed == "<br />" {
                    self.push(" \\\n");
                    return;
                }
                self.warn(
                    self.current_line,
                    format!(
                        "HTML has no Typst equivalent; kept as literal text: {}",
                        truncate(trimmed, 60)
                    ),
                );
                self.push(&format!("#raw({})", typst_string(&html)));
            }
            Event::FootnoteReference(label) => {
                self.flush_text();
                self.footnote_refs
                    .push((label.to_string(), self.current_line));
                self.push(&footnote_placeholder(&label));
            }
            Event::SoftBreak => {
                self.flush_text();
                self.push("\n");
            }
            Event::HardBreak => {
                self.flush_text();
                self.push(" \\\n");
            }
            Event::Rule => {
                self.block_break();
                self.push("#line(length: 100%)\n\n");
            }
            Event::TaskListMarker(done) => {
                self.flush_text();
                self.push(if done { "☑ " } else { "☐ " });
            }
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                if self.lists.is_empty() && self.tables.is_empty() {
                    self.block_break();
                }
            }
            Tag::Heading { .. }
            | Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::TableCell => self.begin_capture(),
            Tag::BlockQuote(_) => {
                self.block_break();
                self.begin_capture();
            }
            Tag::CodeBlock(kind) => {
                self.block_break();
                self.in_code_block = true;
                self.code_buf.clear();
                self.code_fence_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        lang.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Tag::List(start) => {
                if self.lists.is_empty() {
                    self.block_break();
                } else {
                    self.flush_text();
                    let buf = self.out.last_mut().unwrap();
                    if !buf.is_empty() && !buf.ends_with('\n') {
                        buf.push('\n');
                    }
                }
                self.lists.push(ListState { next: start });
            }
            Tag::Item => self.begin_capture(),
            Tag::Table(alignments) => {
                self.block_break();
                self.tables.push(TableState {
                    alignments,
                    rows: Vec::new(),
                    header_rows: 0,
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(t) = self.tables.last_mut() {
                    t.in_head = true;
                    t.rows.push(Vec::new());
                }
            }
            Tag::TableRow => {
                if let Some(t) = self.tables.last_mut() {
                    t.rows.push(Vec::new());
                }
            }
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => {
                self.begin_capture();
                self.link_dest.push((dest_url.to_string(), link_type));
            }
            Tag::Image { dest_url, .. } => {
                self.begin_capture();
                self.image_dest.push(dest_url.to_string());
            }
            Tag::FootnoteDefinition(label) => {
                self.block_break();
                self.current_footnote = Some(label.to_string());
                self.begin_capture();
            }
            Tag::HtmlBlock => self.block_break(),
            Tag::MetadataBlock(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if self.lists.is_empty() && self.tables.is_empty() {
                    self.block_break();
                } else {
                    self.flush_text();
                    self.push("\n");
                }
            }
            TagEnd::Heading(level) => {
                let text = self.end_capture();
                self.block_break();
                let depth = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.push(&format!(
                    "{} {}\n\n",
                    "=".repeat(depth),
                    text.trim().replace('\n', " ")
                ));
            }
            TagEnd::Emphasis => {
                let inner = self.end_capture();
                self.push(&format!("#emph[{inner}]"));
            }
            TagEnd::Strong => {
                let inner = self.end_capture();
                self.push(&format!("#strong[{inner}]"));
            }
            TagEnd::Strikethrough => {
                let inner = self.end_capture();
                self.push(&format!("#strike[{inner}]"));
            }
            TagEnd::BlockQuote(_) => {
                let inner = self.end_capture();
                self.push(&format!("#quote(block: true)[\n{}\n]\n\n", inner.trim()));
            }
            TagEnd::CodeBlock => self.end_code_block(),
            TagEnd::List(_) => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.block_break();
                }
            }
            TagEnd::Item => {
                let content = self.end_capture();
                // No per-depth indent here: a nested list is captured inside
                // its parent item, whose continuation indent already shifts it.
                let marker = match self.lists.last_mut().and_then(|l| l.next.as_mut()) {
                    Some(n) => {
                        let m = format!("{n}.");
                        *n += 1;
                        m
                    }
                    None => "-".to_string(),
                };
                let continuation = format!("\n{}", " ".repeat(marker.len() + 1));
                let body = content.trim_end().replace('\n', &continuation);
                // Blank continuation lines shouldn't carry trailing spaces.
                let body = body
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join("\n");
                self.push(&format!("{marker} {}\n", body.trim_start()));
            }
            TagEnd::Table => self.end_table(),
            TagEnd::TableHead => {
                if let Some(t) = self.tables.last_mut() {
                    t.in_head = false;
                    t.header_rows = t.rows.len();
                }
            }
            TagEnd::TableRow => {}
            TagEnd::TableCell => {
                let cell = self.end_capture();
                if let Some(t) = self.tables.last_mut() {
                    if let Some(row) = t.rows.last_mut() {
                        row.push(cell.trim().to_string());
                    }
                }
            }
            TagEnd::Link => {
                let text = self.end_capture();
                let (dest, link_type) = self
                    .link_dest
                    .pop()
                    .unwrap_or((String::new(), LinkType::Inline));
                let text = text.trim();
                if text.is_empty() || matches!(link_type, LinkType::Autolink | LinkType::Email) {
                    let dest = if link_type == LinkType::Email {
                        format!("mailto:{dest}")
                    } else {
                        dest
                    };
                    self.push(&format!("#link({})", typst_string(&dest)));
                } else {
                    self.push(&format!("#link({})[{text}]", typst_string(&dest)));
                }
            }
            TagEnd::Image => {
                let alt = self.end_capture();
                let dest = self.image_dest.pop().unwrap_or_default();
                let image = self.image(&dest, &alt, self.current_line);
                self.push(&image);
            }
            TagEnd::FootnoteDefinition => {
                let def = self.end_capture();
                if let Some(label) = self.current_footnote.take() {
                    self.footnotes.insert(label, def.trim().to_string());
                }
            }
            TagEnd::HtmlBlock => self.block_break(),
            TagEnd::MetadataBlock(_)
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition => {}
        }
    }

    fn end_code_block(&mut self) {
        self.in_code_block = false;
        let code = std::mem::take(&mut self.code_buf);
        // A fence longer than any backtick run inside the code.
        let longest = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
        let fence = "`".repeat(longest.max(2) + 1);
        let lang = std::mem::take(&mut self.code_fence_lang);
        let lang = if lang
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+')
        {
            lang
        } else {
            String::new()
        };
        self.push(&format!(
            "{fence}{lang}\n{}{}{fence}\n\n",
            code,
            if code.ends_with('\n') { "" } else { "\n" }
        ));
    }

    fn end_table(&mut self) {
        let Some(table) = self.tables.pop() else {
            return;
        };
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let rows: Vec<Vec<String>> = table
            .rows
            .into_iter()
            .map(|mut r| {
                r.resize(columns, String::new());
                r
            })
            .collect();
        let layout = merge_layout(&rows);

        let align: Vec<&str> = (0..columns)
            .map(|i| match table.alignments.get(i) {
                Some(Alignment::Center) => "center",
                Some(Alignment::Right) => "right",
                _ => "left",
            })
            .collect();

        let render_row = |r: usize| -> String {
            rows[r]
                .iter()
                .enumerate()
                .filter(|(c, _)| !layout[r][*c].merged)
                .map(|(c, text)| {
                    let l = &layout[r][c];
                    let mut args = Vec::new();
                    if l.row_span > 1 {
                        args.push(format!("rowspan: {}", l.row_span));
                    }
                    if l.col_span > 1 {
                        args.push(format!("colspan: {}", l.col_span));
                    }
                    if args.is_empty() {
                        format!("[{text}]")
                    } else {
                        format!("table.cell({})[{text}]", args.join(", "))
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut out = format!(
            "#table(\n  columns: {columns},\n  align: ({}),\n",
            align.join(", ")
        );
        if table.header_rows > 0 {
            let header: Vec<String> = (0..table.header_rows).map(render_row).collect();
            out.push_str(&format!("  table.header({}),\n", header.join(", ")));
        }
        for r in table.header_rows..rows.len() {
            let row = render_row(r);
            if !row.is_empty() {
                out.push_str(&format!("  {row},\n"));
            }
        }
        out.push_str(")\n\n");
        self.push(&out);
    }

    fn image(&mut self, dest: &str, alt: &str, line: Option<usize>) -> String {
        if dest.contains("://") {
            self.warn(
                line,
                format!("Remote image {} can't be embedded (Typst only reads files in the vault); converted to a link", truncate(dest, 60)),
            );
            let label = if alt.trim().is_empty() {
                dest.to_string()
            } else {
                alt.trim().to_string()
            };
            return format!(
                "#link({})[{}]",
                typst_string(dest),
                escape_text(&label, false)
            );
        }
        let dest = percent_decode(dest);
        if alt.trim().is_empty() {
            format!("#image({})", typst_string(&dest))
        } else {
            format!(
                "#image({}, alt: {})",
                typst_string(&dest),
                typst_string(&unescape_typst(alt.trim()))
            )
        }
    }

    fn math(&mut self, tex: &str, display: bool) -> String {
        match latex_to_typst_math(tex) {
            Some(math) if display => format!("$ {math} $"),
            Some(math) => format!("${math}$"),
            None => {
                self.warn(
                    self.current_line,
                    format!(
                        "Math couldn't be converted to Typst; kept as LaTeX code: {}",
                        truncate(tex.trim(), 60)
                    ),
                );
                if display {
                    format!("```latex\n{}\n```", tex.trim())
                } else {
                    inline_raw(tex)
                }
            }
        }
    }

    /// Emit the pending text, converting Obsidian syntax and escaping the rest.
    fn flush_text(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text);
        let line = self.text_line.take();
        let mut out = String::new();
        let mut last = 0;
        for caps in WIKI_OR_HIGHLIGHT.captures_iter(&text) {
            let m = caps.get(0).unwrap();
            out.push_str(&self.escape_here(&text[last..m.start()], &out));
            if let Some(hl) = caps.get(4) {
                out.push_str(&format!("#highlight[{}]", escape_text(hl.as_str(), false)));
            } else {
                let embed = !caps[1].is_empty();
                let target = caps[2].trim();
                let label = caps
                    .get(3)
                    .map(|l| l.as_str().trim())
                    .filter(|l| !l.is_empty());
                out.push_str(&self.wiki(target, label, embed, line));
            }
            last = m.end();
        }
        out.push_str(&self.escape_here(&text[last..], &out));
        self.push(&out);
    }

    /// Escape `text` knowing what precedes it (for line-start rules).
    fn escape_here(&self, text: &str, pending: &str) -> String {
        let before = if pending.is_empty() {
            self.out.last().map(String::as_str).unwrap_or("")
        } else {
            pending
        };
        let at_line_start = before.is_empty() || before.ends_with('\n');
        escape_text(text, at_line_start)
    }

    fn wiki(
        &mut self,
        target: &str,
        label: Option<&str>,
        embed: bool,
        line: Option<usize>,
    ) -> String {
        let (path, _heading) = target.split_once('#').unwrap_or((target, ""));
        if embed {
            let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                return format!("#image({})", typst_string(path));
            }
            self.warn(
                line,
                format!("Embedding ![[{target}]] has no Typst equivalent; converted to a link"),
            );
        }
        let text = label.unwrap_or(target);
        format!(
            "#link({})[{}]",
            typst_string(&format!("librarium://note/{target}")),
            escape_text(text, false)
        )
    }
}

// ── Merged cells (mirrors frontend/src/editor/table-merge-layout.ts) ────────

#[derive(Clone)]
struct CellLayout {
    merged: bool,
    col_span: usize,
    row_span: usize,
}

/// `^^` merges into the cell above (body rows only), `<<` into the cell to
/// the left; a marker with nothing to merge into stays literal text. Cells
/// arrive already escaped for Typst (`<<` as `\<\<`), hence the unescape.
// The row-span pass walks column by column across rows, so it indexes.
#[allow(clippy::needless_range_loop)]
fn merge_layout(rows: &[Vec<String>]) -> Vec<Vec<CellLayout>> {
    let mut layout: Vec<Vec<CellLayout>> = rows
        .iter()
        .map(|r| {
            vec![
                CellLayout {
                    merged: false,
                    col_span: 1,
                    row_span: 1
                };
                r.len()
            ]
        })
        .collect();
    for (r, cells) in rows.iter().enumerate() {
        let mut anchor: Option<usize> = None;
        for (c, text) in cells.iter().enumerate() {
            match anchor {
                Some(a) if unescape_typst(text).trim() == "<<" => {
                    layout[r][c].merged = true;
                    layout[r][a].col_span += 1;
                }
                _ => anchor = Some(c),
            }
        }
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    for c in 0..cols {
        let mut anchor: Option<usize> = None;
        for (r, row) in rows.iter().enumerate().skip(1) {
            let Some(text) = row.get(c) else {
                anchor = None;
                continue;
            };
            match anchor {
                Some(a) if unescape_typst(text).trim() == "^^" && !layout[r][c].merged => {
                    layout[r][c].merged = true;
                    layout[a][c].row_span += 1;
                }
                _ => anchor = Some(r),
            }
        }
    }
    layout
}

// ── Escaping and literals ───────────────────────────────────────────────────

/// Escape Markdown text so Typst shows it literally.
fn escape_text(text: &str, mut at_line_start: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if at_line_start {
            if c == ' ' || c == '\t' {
                out.push(c);
                i += 1;
                continue;
            }
            // Markup that only means something at the start of a line.
            if matches!(c, '=' | '-' | '+' | '/') {
                out.push('\\');
            } else if c.is_ascii_digit() {
                let mut j = i;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                if chars.get(j) == Some(&'.') {
                    out.extend(&chars[i..j]);
                    out.push_str("\\.");
                    i = j + 1;
                    at_line_start = false;
                    continue;
                }
            }
            at_line_start = false;
        }
        match c {
            '\\' | '*' | '_' | '`' | '$' | '#' | '@' | '<' | '>' | '[' | ']' | '~' => {
                out.push('\\');
                out.push(c);
            }
            // `//` and `/*` start comments.
            '/' if matches!(next, Some('/') | Some('*')) => out.push_str("\\/"),
            // `--`/`---` become dashes and `-?` a soft hyphen in Typst.
            '-' if matches!(next, Some('-') | Some('?')) => out.push_str("\\-"),
            '\n' => {
                out.push('\n');
                at_line_start = true;
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// Undo `escape_text` for strings that go into a Typst string literal.
fn unescape_typst(escaped: &str) -> String {
    let mut out = String::new();
    let mut chars = escaped.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

use librarium_core::typst_frontmatter::{typst_string, typst_value};

fn inline_raw(code: &str) -> String {
    if code.contains('`') || code.contains('\n') {
        format!("#raw({})", typst_string(code))
    } else {
        format!("`{code}`")
    }
}

fn footnote_placeholder(label: &str) -> String {
    format!("\u{E010}{label}\u{E011}")
}

fn truncate(s: &str, max: usize) -> String {
    let flat = s.replace('\n', " ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

fn percent_decode(s: &str) -> String {
    urlencoding::decode(s)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

// ── LaTeX math ──────────────────────────────────────────────────────────────

/// Typst math names mitex output may use that resolve without its package.
static TYPST_MATH_NAMES: LazyLock<std::collections::HashSet<&'static str>> = LazyLock::new(|| {
    [
        // Greek.
        "alpha",
        "beta",
        "gamma",
        "delta",
        "epsilon",
        "zeta",
        "eta",
        "theta",
        "iota",
        "kappa",
        "lambda",
        "mu",
        "nu",
        "xi",
        "omicron",
        "pi",
        "rho",
        "sigma",
        "tau",
        "upsilon",
        "phi",
        "chi",
        "psi",
        "omega",
        "Alpha",
        "Beta",
        "Gamma",
        "Delta",
        "Epsilon",
        "Zeta",
        "Eta",
        "Theta",
        "Iota",
        "Kappa",
        "Lambda",
        "Mu",
        "Nu",
        "Xi",
        "Omicron",
        "Pi",
        "Rho",
        "Sigma",
        "Tau",
        "Upsilon",
        "Phi",
        "Chi",
        "Psi",
        "Omega",
        "alt",
        "epsilon.alt",
        "theta.alt",
        "phi.alt",
        "pi.alt",
        "rho.alt",
        "sigma.alt",
        "kappa.alt",
        // Functions and big operators.
        "frac",
        "sqrt",
        "root",
        "lr",
        "abs",
        "norm",
        "floor",
        "ceil",
        "binom",
        "mat",
        "vec",
        "cases",
        "op",
        "bold",
        "italic",
        "upright",
        "cal",
        "frak",
        "bb",
        "sans",
        "mono",
        "arrow",
        "hat",
        "tilde",
        "macron",
        "overline",
        "underline",
        "overbrace",
        "underbrace",
        "dot",
        "dot.double",
        "diaer",
        "acute",
        "grave",
        "breve",
        "caron",
        "circle",
        "sum",
        "product",
        "integral",
        "integral.double",
        "integral.triple",
        "integral.cont",
        "union",
        "inter",
        "union.big",
        "inter.big",
        "lim",
        "limsup",
        "liminf",
        "sup",
        "inf",
        "max",
        "min",
        "sin",
        "cos",
        "tan",
        "cot",
        "sec",
        "csc",
        "arcsin",
        "arccos",
        "arctan",
        "sinh",
        "cosh",
        "tanh",
        "coth",
        "log",
        "ln",
        "exp",
        "det",
        "dim",
        "ker",
        "deg",
        "gcd",
        "lcm",
        "arg",
        "mod",
        "Pr",
        "tr",
        "text",
        "display",
        "inline",
        "limits",
        "scripts",
        "attach",
        "stretch",
        "cancel",
        "delim",
        // Symbols.
        "oo",
        "dot.c",
        "times",
        "div",
        "plus.minus",
        "minus.plus",
        "approx",
        "equiv",
        "prop",
        "in",
        "in.not",
        "subset",
        "subset.eq",
        "supset",
        "supset.eq",
        "emptyset",
        "nabla",
        "partial",
        "forall",
        "exists",
        "exists.not",
        "not",
        "and",
        "or",
        "angle",
        "perp",
        "parallel",
        "ast",
        "star",
        "compose",
        "tilde.op",
        "dots",
        "dots.h",
        "dots.v",
        "dots.c",
        "dots.down",
        "thin",
        "med",
        "thick",
        "quad",
        "wide",
        "zws",
        "space",
        "sim",
        "tilde.eq",
        "gt",
        "lt",
        "ge",
        "le",
        "gt.eq",
        "lt.eq",
        "gt.double",
        "lt.double",
        "eq",
        "eq.not",
        "arrow.r",
        "arrow.l",
        "arrow.l.r",
        "arrow.r.double",
        "arrow.l.double",
        "arrow.l.r.double",
        "arrow.t",
        "arrow.b",
        "arrow.r.long",
        "arrow.l.long",
        "arrow.r.bar",
        "arrow.r.hook",
        "harpoon",
        "top",
        "bot",
        "aleph",
        "planck",
        "planck.reduce",
        "ell",
        "Re",
        "Im",
        "prime",
        "degree",
        "infinity",
        "diff",
        "without",
        "backslash",
        "bar",
        "bar.v",
        "bar.v.double",
        "angle.l",
        "angle.r",
        "brace.l",
        "brace.r",
        "paren.l",
        "paren.r",
        "bracket.l",
        "bracket.r",
        "none",
        "dif",
        "Delta.alt",
        "colon",
        "semi",
        "comma",
        "excl",
        "quest",
        "hash",
        "at",
        "percent",
        "amp",
        "lozenge",
        "square",
        "triangle",
        "diamond",
        "suit",
        "checkmark",
    ]
    .into_iter()
    .collect()
});

static MITEX_TEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#textmath\[([^\]]*)\];?").unwrap());
static MITEX_OPNAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"operatorname\(([^()]*)\)").unwrap());
static MATH_IDENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z][A-Za-z]*(?:\.[A-Za-z]+)*").unwrap());
static MATH_STRING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""(?:[^"\\]|\\.)*""#).unwrap());

/// LaTeX math → Typst math, or `None` if the result would need the mitex
/// Typst package (which Librarium doesn't load).
fn latex_to_typst_math(tex: &str) -> Option<String> {
    let converted = mitex::convert_math(tex.trim(), None).ok()?;
    // Rewrite the few mitex helpers that have plain-Typst equivalents.
    let converted = MITEX_TEXT.replace_all(&converted, |c: &regex::Captures| typst_string(&c[1]));
    let converted = MITEX_OPNAME.replace_all(&converted, |c: &regex::Captures| {
        format!(
            "op({})",
            typst_string(&c[1].split_whitespace().collect::<String>())
        )
    });
    let converted = converted
        .replace("mitexsqrt(", "sqrt(")
        .replace("mitexmathbf(", "bold(")
        .replace("mitexmathit(", "italic(")
        .replace("mitexmathrm(", "upright(")
        .replace("pmatrix(", "mat(delim: \"(\", ")
        .replace("bmatrix(", "mat(delim: \"[\", ")
        .replace("vmatrix(", "mat(delim: \"|\", ")
        .replace("Bmatrix(", "mat(delim: \"{\", ")
        .replace("matrix(", "mat(delim: #none, ");
    if converted.contains('#') && !converted.contains("#none") {
        return None;
    }
    // Every multi-letter name must be a Typst math name; single letters are
    // variables. String literals ("if") are text.
    let without_strings = MATH_STRING.replace_all(&converted, "");
    for ident in MATH_IDENT.find_iter(&without_strings) {
        let name = ident.as_str();
        if name.len() > 1 && !TYPST_MATH_NAMES.contains(name) && name != "delim" && name != "none" {
            return None;
        }
    }
    let collapsed = converted.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typst(md: &str) -> String {
        markdown_to_typst(md).typst
    }

    #[test]
    fn headings_emphasis_and_paragraphs() {
        let out =
            typst("# Title\n\nSome *emph*, **strong**, ~~gone~~ and `code`.\n\nSecond paragraph.");
        assert!(out.contains("= Title\n"), "{out}");
        assert!(out.contains("#emph[emph]"));
        assert!(out.contains("#strong[strong]"));
        assert!(out.contains("#strike[gone]"));
        assert!(out.contains("`code`"));
        assert!(out.contains("\n\nSecond paragraph."));
    }

    #[test]
    fn escapes_typst_syntax_in_prose() {
        let out = typst("Price: $5 * 3 = 15, a_b, #tag, me@example.com, a < b > c, // not a comment, a -- b\n\n= not a heading\n\n1. real list\n\n2\\. not a list");
        for needle in [
            "\\$5",
            "\\*",
            "a\\_b",
            "\\#tag",
            "me\\@example.com",
            "a \\< b \\> c",
            "\\//",
            "\\--",
        ] {
            assert!(out.contains(needle), "missing {needle} in:\n{out}");
        }
        assert!(out.contains("\\= not a heading"), "{out}");
        assert!(out.contains("2\\. not a list"), "{out}");
    }

    #[test]
    fn lists_nested_ordered_and_tasks() {
        let out = typst("- a\n  - nested\n- b\n\n3. three\n4. four\n\n- [ ] todo\n- [x] done");
        assert!(out.contains("- a\n  - nested\n- b\n"), "{out}");
        assert!(out.contains("3. three\n4. four"), "{out}");
        assert!(out.contains("- ☐ todo\n- ☑ done"), "{out}");
    }

    #[test]
    fn links_images_and_wiki_links() {
        let out = typst("[site](https://example.com) <https://auto.example> ![alt](img/a.png) [[Some Note]] [[Other|label]] ![[pic.png]]");
        assert!(
            out.contains("#link(\"https://example.com\")[site]"),
            "{out}"
        );
        assert!(out.contains("#link(\"https://auto.example\")"), "{out}");
        assert!(out.contains("#image(\"img/a.png\", alt: \"alt\")"), "{out}");
        assert!(
            out.contains("#link(\"librarium://note/Some Note\")[Some Note]"),
            "{out}"
        );
        assert!(
            out.contains("#link(\"librarium://note/Other\")[label]"),
            "{out}"
        );
        assert!(out.contains("#image(\"pic.png\")"), "{out}");
    }

    #[test]
    fn frontmatter_becomes_metadata_and_title() {
        let out =
            typst("---\ntitle: My \"Note\"\ntags: [a, b]\ncount: 3\ndraft: true\n---\n# Body");
        assert!(
            out.starts_with("#set document(title: \"My \\\"Note\\\"\")\n"),
            "{out}"
        );
        assert!(out.contains("#metadata((\"title\": \"My \\\"Note\\\"\", \"tags\": (\"a\", \"b\"), \"count\": 3, \"draft\": true)) <frontmatter>"), "{out}");
        assert!(out.contains("= Body"));
    }

    #[test]
    fn tables_with_alignment_and_merged_cells() {
        let out = typst("| Region | Q1 | Q2 |\n| :-- | :-: | --: |\n| North | 10 | 12 |\n| ^^ | 11 | 13 |\n| Total | 46 | << |");
        assert!(out.contains("columns: 3"), "{out}");
        assert!(out.contains("align: (left, center, right)"), "{out}");
        assert!(out.contains("table.header([Region], [Q1], [Q2])"), "{out}");
        assert!(
            out.contains("table.cell(rowspan: 2)[North], [10], [12]"),
            "{out}"
        );
        assert!(out.contains("  [11], [13],"), "{out}");
        assert!(out.contains("[Total], table.cell(colspan: 2)[46]"), "{out}");
    }

    #[test]
    fn code_blocks_keep_content_and_language() {
        let out = typst("```rust\nfn main() { let s = \"*not strong*\"; }\n```\n\n````md\n```\ninner fence\n```\n````");
        assert!(
            out.contains("```rust\nfn main() { let s = \"*not strong*\"; }\n```"),
            "{out}"
        );
        assert!(out.contains("````md\n```\ninner fence\n```\n````"), "{out}");
    }

    #[test]
    fn footnotes_are_inlined() {
        let out = typst("Claim.[^1] Another.[^missing]\n\n[^1]: The source.");
        assert!(out.contains("Claim.#footnote[The source.]"), "{out}");
        // An undefined footnote isn't a footnote to Markdown either: literal text.
        assert!(out.contains("Another.\\[^missing\\]"), "{out}");
    }

    #[test]
    fn math_converts_or_falls_back_with_a_warning() {
        let conv = markdown_to_typst(
            "Inline $x^2 + \\alpha$ and\n\n$$\\frac{a}{b}$$\n\nand $\\weirdmacro{x}$",
        );
        assert!(
            conv.typst.contains("$x ^(2 ) + alpha$") || conv.typst.contains("$x ^(2) + alpha$"),
            "{}",
            conv.typst
        );
        assert!(
            conv.typst.contains("$ frac(a ,b ) $") || conv.typst.contains("$ frac(a, b) $"),
            "{}",
            conv.typst
        );
        assert!(conv.typst.contains("`\\weirdmacro{x}`"), "{}", conv.typst);
        assert_eq!(conv.warnings.len(), 1, "{:?}", conv.warnings);
        assert!(conv.warnings[0]
            .message
            .contains("Math couldn't be converted"));
    }

    #[test]
    fn warns_about_what_it_cannot_carry_over() {
        let conv = markdown_to_typst(
            "# T\n\n<div>html</div>\n\n![remote](https://x.y/a.png)\n\n![[Other note]]",
        );
        let messages: Vec<&str> = conv.warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("HTML")), "{messages:?}");
        assert!(
            messages.iter().any(|m| m.contains("Remote image")),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("![[Other note]]")),
            "{messages:?}"
        );
        // Line numbers point into the Markdown.
        let html = conv
            .warnings
            .iter()
            .find(|w| w.message.contains("HTML"))
            .unwrap();
        assert_eq!(html.line, Some(3));
    }

    #[test]
    fn warning_lines_account_for_frontmatter() {
        let conv = markdown_to_typst("---\ntitle: x\n---\n\n<b>bold</b>\n");
        assert_eq!(conv.warnings[0].line, Some(5), "{:?}", conv.warnings);
    }

    #[test]
    fn highlight_and_block_quotes() {
        let out = typst("A ==marked== word.\n\n> Quoted *text*\n> more");
        assert!(out.contains("#highlight[marked]"), "{out}");
        assert!(
            out.contains("#quote(block: true)[\nQuoted #emph[text]\nmore\n]"),
            "{out}"
        );
    }

    /// The real proof: converted Markdown must compile. Runs the converter
    /// over the repository's own docs and a note full of edge cases.
    #[cfg(feature = "typst")]
    #[test]
    fn converted_markdown_compiles() {
        use crate::services::typst_service::render_pdf;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut docs: Vec<(String, String)> = [
            "README.md",
            "CHANGELOG.md",
            "docs/DESIGN.md",
            "docs/USER_GUIDE.md",
            "docs/DEPLOYMENT.md",
            "docs/CONFIGURATION.md",
            "docs/RELEASE-PUNCH-LIST.md",
        ]
        .iter()
        .map(|p| {
            (
                p.to_string(),
                std::fs::read_to_string(root.join(p)).unwrap(),
            )
        })
        .collect();
        docs.push((
            "edge cases".into(),
            "---\ntitle: Edge\ntags: [x]\nnested: {a: 1, b: [true, null]}\n---\n\
             # Heading with `code` and [link](x.md)\n\n\
             Word*star*word, snake_case_name, 2 * 3, #hashtag, @user, 1/2, a//b, <b>, ~tilde~, $5, 100%\n\
             = equals at line start\n- dash at line start in a paragraph? no, that's a list\n\n\
             + plus\n\n10. ten\n11. eleven\n\n\
             > quote with list:\n> - one\n> - two\n\n\
             - item\n\n  continued paragraph in item\n\n  ```js\n  code();\n  ```\n\n\
             | a | b |\n|---|---|\n| `x|y` | [[Note]] |\n| **bold** | ^^ |\n\n\
             Text with footnote[^n] and $\\sqrt{x}$ and $$\\sum_{i=1}^n i$$\n\n[^n]: Footnote *def*.\n\n\
             ![[img.png]] ![a](missing.png)\n\n***\n\nTrailing \\\nhard break.\n"
                .into(),
        ));

        let vault = tempfile::TempDir::new().unwrap();
        // Missing images are the only acceptable compile failure here; strip
        // image calls so the check is about syntax.
        let image_call = Regex::new(r#"#image\("[^"]*"(, alt: "[^"]*")?\)"#).unwrap();
        for (name, md) in docs {
            let conv = markdown_to_typst(&md);
            let typst = image_call.replace_all(&conv.typst, "[img]");
            if let Err(errors) = render_pdf(
                vault.path().to_str().unwrap(),
                "note.typ",
                typst.to_string(),
            ) {
                let lines: Vec<&str> = typst.lines().collect();
                let detail: Vec<String> = errors
                    .iter()
                    .map(|e| {
                        let src = e.line.and_then(|l| lines.get(l - 1)).copied().unwrap_or("");
                        format!("line {:?}: {} | {}", e.line, e.message, src)
                    })
                    .collect();
                panic!("{name} didn't compile:\n{}", detail.join("\n"));
            }
        }
    }
}
