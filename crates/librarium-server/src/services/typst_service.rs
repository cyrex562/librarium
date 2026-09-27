//! Compiles Typst notes (`.typ`) to HTML for Preview (#139).
//!
//! In-process via the `typst` crate (feature `typst`, on by default). The
//! note's text comes from the request, so Preview shows the unsaved buffer;
//! anything it pulls in (`image("fig.png")`, `#include`, `#import`,
//! `read(..)`) is read from the vault through `FileService::resolve_path`,
//! so the usual path-safety rules apply and nothing outside the vault is
//! reachable. Packages (`@preview/..`) are refused: the server may be
//! offline, and downloading code on a render request is not something a
//! note should be able to trigger.
//!
//! Fonts (Typst's bundled set) and the standard library are loaded once per
//! process. HTML export is still experimental upstream; page-layout features
//! with no HTML equivalent produce warnings, not errors.

use std::path::PathBuf;
use std::sync::LazyLock;

use serde::Serialize;
use typst::diag::{FileError, FileResult, Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Features, Library, LibraryExt, World, WorldExt};

use crate::services::FileService;

struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let fonts: Vec<Font> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    Fonts {
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
    }
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| {
    let features: Features = [Feature::Html].into_iter().collect();
    LazyHash::new(Library::builder().with_features(features).build())
});

/// One problem reported by the compiler. `line`/`column` are 1-based and
/// point into the note itself; they're absent when the problem is in an
/// included file or has no location.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TypstDiagnostic {
    pub severity: &'static str,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub hints: Vec<String>,
}

/// Result of rendering a note. `html` is `None` when compilation failed, in
/// which case `diagnostics` holds at least one error.
#[derive(Debug, Clone, Serialize)]
pub struct TypstRender {
    /// Body markup of the rendered document (no `<html>`/`<head>`).
    pub html: Option<String>,
    /// Stylesheet Typst emits alongside the body (MathML layout rules).
    pub css: String,
    pub diagnostics: Vec<TypstDiagnostic>,
}

struct VaultWorld {
    vault_path: String,
    main: Source,
    today: Option<Datetime>,
}

impl VaultWorld {
    fn new(vault_path: &str, note_path: &str, content: String) -> Result<Self, String> {
        let vpath = VirtualPath::new(format!("/{}", note_path.trim_start_matches('/')))
            .map_err(|e| format!("invalid note path {note_path:?}: {e}"))?;
        let id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
        let now = chrono::Local::now();
        use chrono::Datelike;
        Ok(Self {
            vault_path: vault_path.to_string(),
            main: Source::new(id, content),
            today: Datetime::from_ymd(now.year(), now.month() as u8, now.day() as u8),
        })
    }

    fn resolve(&self, id: FileId) -> FileResult<PathBuf> {
        if matches!(id.root(), VirtualRoot::Package(_)) {
            return Err(FileError::Other(Some(
                "packages are not available in Librarium; copy the package's files into the vault instead".into(),
            )));
        }
        let rel = id.vpath().get_without_slash();
        FileService::resolve_path(&self.vault_path, rel).map_err(|_| FileError::AccessDenied)
    }

    fn read(&self, id: FileId) -> FileResult<Vec<u8>> {
        let path = self.resolve(id)?;
        if path.is_dir() {
            return Err(FileError::IsDirectory);
        }
        std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))
    }
}

impl World for VaultWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        let bytes = self.read(id)?;
        let text = String::from_utf8(bytes).map_err(|_| FileError::InvalidUtf8)?;
        Ok(Source::new(id, text))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.read(id).map(Bytes::new)
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        self.today
    }
}

/// Render `content` (the text of the note at `note_path` in the vault at
/// `vault_path`) to HTML. CPU-bound; call it off the async runtime.
pub fn render_html(vault_path: &str, note_path: &str, content: String) -> TypstRender {
    let world = match VaultWorld::new(vault_path, note_path, content) {
        Ok(world) => world,
        Err(message) => {
            return TypstRender {
                html: None,
                css: String::new(),
                diagnostics: vec![TypstDiagnostic {
                    severity: "error",
                    message,
                    line: None,
                    column: None,
                    hints: vec![],
                }],
            }
        }
    };

    let compiled = typst::compile::<typst_html::HtmlDocument>(&world);
    let mut diagnostics: Vec<TypstDiagnostic> = compiled
        .warnings
        .iter()
        // Typst warns on every HTML compile that the exporter is
        // experimental; that's known and not the note's fault.
        .filter(|d| {
            !d.message
                .starts_with("html export is under active development")
        })
        .map(|d| to_diagnostic(&world, d))
        .collect();

    let result = compiled
        .output
        .and_then(|doc| typst_html::html(&doc, &Default::default()));
    // Memoized compilation results otherwise accumulate for the life of
    // the process; keep only what recent renders touched (as the typst CLI's
    // watch mode does).
    comemo::evict(10);

    match result {
        Ok(page) => {
            let (css, html) = split_document(&page);
            TypstRender {
                html: Some(html),
                css,
                diagnostics,
            }
        }
        Err(errors) => {
            diagnostics.splice(0..0, errors.iter().map(|d| to_diagnostic(&world, d)));
            TypstRender {
                html: None,
                css: String::new(),
                diagnostics,
            }
        }
    }
}

fn to_diagnostic(world: &VaultWorld, diag: &SourceDiagnostic) -> TypstDiagnostic {
    let (line, column) = if diag.span.id() == Some(world.main.id()) {
        world
            .range(diag.span)
            .and_then(|range| world.main.lines().byte_to_line_column(range.start))
            .map(|(l, c)| (Some(l + 1), Some(c + 1)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };
    TypstDiagnostic {
        severity: match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        },
        message: diag.message.to_string(),
        line,
        column,
        hints: diag.hints.iter().map(|h| h.v.to_string()).collect(),
    }
}

/// Split Typst's standalone HTML page into (stylesheet, body markup).
fn split_document(page: &str) -> (String, String) {
    let css = match (page.find("<style>"), page.find("</style>")) {
        (Some(start), Some(end)) if end > start => page[start + "<style>".len()..end].to_string(),
        _ => String::new(),
    };
    let body = match page.find("<body>") {
        Some(start) => {
            let rest = &page[start + "<body>".len()..];
            rest[..rest.rfind("</body>").unwrap_or(rest.len())].to_string()
        }
        None => page.to_string(),
    };
    (css, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn render(dir: &TempDir, note: &str, content: &str) -> TypstRender {
        render_html(dir.path().to_str().unwrap(), note, content.to_string())
    }

    #[test]
    fn renders_markup_to_semantic_html() {
        let dir = TempDir::new().unwrap();
        let out = render(
            &dir,
            "note.typ",
            "= Title\n\nSome *bold* and _emph_.\n\n#table(columns: 2, table.header([A], [B]), table.cell(colspan: 2)[wide])\n\n$ x^2 $\n",
        );
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        let html = out.html.expect("html");
        assert!(html.contains("<h2>Title</h2>"), "{html}");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>emph</em>"));
        assert!(html.contains("colspan=\"2\""));
        assert!(html.contains("<math"));
        assert!(!html.contains("<body>") && !html.contains("<head>"));
    }

    #[test]
    fn reports_errors_with_line_and_column() {
        let dir = TempDir::new().unwrap();
        let out = render(&dir, "note.typ", "fine\n\n#nonexistent(1)\n");
        assert!(out.html.is_none());
        let err = out
            .diagnostics
            .iter()
            .find(|d| d.severity == "error")
            .expect("an error");
        assert!(err.message.contains("nonexistent"), "{err:?}");
        assert_eq!(err.line, Some(3));
        assert_eq!(err.column, Some(2));
    }

    #[test]
    fn reads_included_files_relative_to_the_note() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("papers")).unwrap();
        std::fs::write(dir.path().join("papers/intro.typ"), "Included *text*.").unwrap();
        let out = render(&dir, "papers/main.typ", "#include \"intro.typ\"\n");
        assert!(
            out.html.expect("html").contains("<strong>text</strong>"),
            "{:?}",
            out.diagnostics
        );
    }

    #[test]
    fn embeds_vault_images() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("img")).unwrap();
        std::fs::write(
            dir.path().join("img/dot.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#,
        )
        .unwrap();
        let out = render(&dir, "note.typ", "#image(\"img/dot.svg\")\n");
        let html = out.html.unwrap_or_else(|| panic!("{:?}", out.diagnostics));
        assert!(
            html.contains("<img") && html.contains("data:image/svg+xml"),
            "{html}"
        );
    }

    #[test]
    fn cannot_read_outside_the_vault() {
        let outer = TempDir::new().unwrap();
        std::fs::write(outer.path().join("secret.txt"), "top secret").unwrap();
        let vault = outer.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let out = render_html(
            vault.to_str().unwrap(),
            "note.typ",
            "#read(\"../secret.txt\")\n#read(\"/../secret.txt\")\n".to_string(),
        );
        assert!(
            out.html.is_none(),
            "read outside the vault succeeded: {:?}",
            out.html
        );
        assert!(out
            .diagnostics
            .iter()
            .all(|d| !d.message.contains("top secret")));
    }

    #[test]
    fn refuses_packages() {
        let dir = TempDir::new().unwrap();
        let out = render(
            &dir,
            "note.typ",
            "#import \"@preview/cetz:0.3.0\": canvas\n",
        );
        assert!(out.html.is_none());
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.message.contains("packages are not available")),
            "{:?}",
            out.diagnostics
        );
    }
}
