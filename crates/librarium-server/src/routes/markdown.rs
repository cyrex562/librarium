use crate::error::AppResult;
use crate::routes::vaults::AppState;
use actix_web::{post, web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct RenderRequest {
    content: String,
}

#[derive(Deserialize)]
pub struct RenderWithResolutionRequest {
    content: String,
    /// Current file path for relative link resolution
    current_file: Option<String>,
}

/// Render markdown to HTML (no vault context — uses the default parser).
#[post("/api/render")]
pub async fn render_markdown(
    state: web::Data<AppState>,
    req: web::Json<RenderRequest>,
) -> AppResult<HttpResponse> {
    let doc = state.document_parser.render(&req.content);
    Ok(HttpResponse::Ok().content_type("text/html").body(doc.html))
}

/// Render markdown with wiki link resolution for a specific vault.
#[post("/api/vaults/{vault_id}/render")]
pub async fn render_markdown_with_resolution(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    req: web::Json<RenderWithResolutionRequest>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let vault = state.db.get_vault(&vault_id).await?;

    let doc = state.document_parser.render_with_context(
        &req.content,
        Some(&vault.path),
        req.current_file.as_deref(),
    );
    Ok(HttpResponse::Ok().content_type("text/html").body(doc.html))
}

#[derive(Deserialize)]
#[cfg_attr(not(feature = "typst"), allow(dead_code))]
pub struct RenderTypstRequest {
    /// Vault-relative path of the note; `#include`/`image()` paths resolve
    /// relative to it.
    path: String,
    /// The note's current text (possibly unsaved).
    content: String,
}

/// Compile a Typst note to HTML for Preview. Returns
/// `{html, css, diagnostics}`; compile errors are a 200 with `html: null`,
/// since a note mid-edit is routinely invalid.
#[post("/api/vaults/{vault_id}/render-typst")]
pub async fn render_typst(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    req: web::Json<RenderTypstRequest>,
) -> AppResult<HttpResponse> {
    let vault = state.db.get_vault(&vault_id.into_inner()).await?;
    render_typst_impl(vault.path, req.into_inner()).await
}

#[cfg(feature = "typst")]
async fn render_typst_impl(vault_path: String, req: RenderTypstRequest) -> AppResult<HttpResponse> {
    let rendered = run_typst(move || {
        crate::services::typst_service::render_html(&vault_path, &req.path, req.content)
    })
    .await?;
    Ok(HttpResponse::Ok().json(rendered))
}

/// Run a Typst compile on the blocking pool. Compiles are CPU-bound and
/// can't be cancelled once started (a note with an unbounded loop runs until
/// it finishes), so at most two run at once, shared by Preview and PDF
/// export, rather than letting one user's typing occupy every blocking thread.
#[cfg(feature = "typst")]
async fn run_typst<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> AppResult<T> {
    use std::sync::LazyLock;
    use tokio::sync::Semaphore;

    static COMPILES: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));
    let _permit = COMPILES
        .acquire()
        .await
        .map_err(|e| crate::error::AppError::InternalError(e.to_string()))?;
    web::block(f)
        .await
        .map_err(|e| crate::error::AppError::InternalError(e.to_string()))
}

#[cfg(not(feature = "typst"))]
async fn render_typst_impl(
    _vault_path: String,
    _req: RenderTypstRequest,
) -> AppResult<HttpResponse> {
    Ok(typst_unavailable())
}

#[cfg(not(feature = "typst"))]
fn typst_unavailable() -> HttpResponse {
    HttpResponse::NotImplemented().json(serde_json::json!({
        "error": "This server was built without Typst support (cargo feature \"typst\")."
    }))
}

#[derive(Deserialize)]
#[cfg_attr(not(feature = "typst"), allow(dead_code))]
pub struct ExportPdfRequest {
    /// Vault-relative path of the note.
    path: String,
    /// The note's current text; when absent the saved file is exported.
    content: Option<String>,
}

/// Export a Typst or Markdown note as a PDF download (#140). Markdown is
/// converted to Typst first (#141); the conversion's warnings aren't
/// reported here (Convert to Typst shows them). Compile errors are a 422
/// with `{error, diagnostics}`.
#[post("/api/vaults/{vault_id}/export-pdf")]
pub async fn export_pdf(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    req: web::Json<ExportPdfRequest>,
) -> AppResult<HttpResponse> {
    let vault = state.db.get_vault(&vault_id.into_inner()).await?;
    let req = req.into_inner();
    let lower = req.path.to_ascii_lowercase();
    if !lower.ends_with(".typ") && !lower.ends_with(".md") {
        return Err(crate::error::AppError::InvalidInput(
            "Only Typst (.typ) and Markdown (.md) notes can be exported to PDF".to_string(),
        ));
    }
    export_pdf_impl(vault.path, req).await
}

#[cfg(feature = "typst")]
async fn export_pdf_impl(vault_path: String, req: ExportPdfRequest) -> AppResult<HttpResponse> {
    use crate::services::FileService;

    let content = match req.content {
        Some(content) => content,
        None => {
            let full = FileService::resolve_path(&vault_path, &req.path)?;
            if !full.is_file() {
                return Err(crate::error::AppError::NotFound(format!(
                    "File not found: {}",
                    req.path
                )));
            }
            std::fs::read_to_string(&full)?
        }
    };
    let content = if req.path.to_ascii_lowercase().ends_with(".md") {
        crate::services::typst_convert::markdown_to_typst(&content).typst
    } else {
        content
    };
    let file_name = std::path::Path::new(&req.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("note")
        .to_string();
    let path = req.path;
    let result =
        run_typst(move || crate::services::typst_service::render_pdf(&vault_path, &path, content))
            .await?;
    match result {
        Ok(pdf) => Ok(HttpResponse::Ok()
            .content_type("application/pdf")
            .insert_header((
                "Content-Disposition",
                format!(
                    "attachment; filename=\"{}.pdf\"; filename*=UTF-8''{}.pdf",
                    file_name.replace(['"', '\\'], "_"),
                    urlencoding::encode(&file_name)
                ),
            ))
            .body(pdf)),
        Err(diagnostics) => Ok(HttpResponse::UnprocessableEntity().json(serde_json::json!({
            "error": "The note has errors, so it couldn't be exported",
            "diagnostics": diagnostics,
        }))),
    }
}

#[cfg(not(feature = "typst"))]
async fn export_pdf_impl(_vault_path: String, _req: ExportPdfRequest) -> AppResult<HttpResponse> {
    Ok(typst_unavailable())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(render_markdown)
        .service(render_markdown_with_resolution)
        .service(render_typst)
        .service(export_pdf);
}
