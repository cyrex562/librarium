use crate::error::AppResult;
use crate::routes::vaults::AppState;
use actix_web::{get, web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_page")]
    page: usize,
    #[serde(default = "default_limit")]
    page_size: usize,
}

fn default_limit() -> usize {
    50
}

fn default_page() -> usize {
    1
}

#[get("/api/vaults/{vault_id}/search")]
async fn search(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    query: web::Query<SearchQuery>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();

    // Verify vault exists
    state.db.get_vault(&vault_id).await?;

    let results = state
        .search_index
        .search(&vault_id, &query.q, query.page, query.page_size)?;

    Ok(HttpResponse::Ok().json(results))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(search);
}
