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
    use std::sync::LazyLock;
    use tokio::sync::Semaphore;

    // Compiles are CPU-bound and can't be cancelled once started (a note
    // with an unbounded loop runs until it finishes), so cap how many run at
    // once rather than letting one user's typing occupy every blocking thread.
    static COMPILES: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));
    let _permit = COMPILES
        .acquire()
        .await
        .map_err(|e| crate::error::AppError::InternalError(e.to_string()))?;

    let rendered = web::block(move || {
        crate::services::typst_service::render_html(&vault_path, &req.path, req.content)
    })
    .await
    .map_err(|e| crate::error::AppError::InternalError(e.to_string()))?;
    Ok(HttpResponse::Ok().json(rendered))
}

#[cfg(not(feature = "typst"))]
async fn render_typst_impl(
    _vault_path: String,
    _req: RenderTypstRequest,
) -> AppResult<HttpResponse> {
    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "error": "This server was built without Typst support (cargo feature \"typst\")."
    })))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(render_markdown)
        .service(render_markdown_with_resolution)
        .service(render_typst);
}
