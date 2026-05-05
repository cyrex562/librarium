use crate::error::AppResult;
use crate::routes::vaults::AppState;
use actix_web::{delete, get, post, web, HttpResponse};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::models::bookmarks::Bookmark;

#[derive(Debug, Deserialize)]
pub struct CreateBookmarkRequest {
    pub path: String,
    pub title: String,
}

#[get("/api/vaults/{vault_id}/bookmarks")]
async fn list_bookmarks(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    state.db.get_vault(&vault_id).await?;
    let bookmarks = state.db.list_bookmarks(&vault_id).await?;
    Ok(HttpResponse::Ok().json(bookmarks))
}

#[post("/api/vaults/{vault_id}/bookmarks")]
async fn create_bookmark(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    body: web::Json<CreateBookmarkRequest>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    state.db.get_vault(&vault_id).await?;
    let bookmark = Bookmark {
        id: Uuid::new_v4().to_string(),
        vault_id: vault_id.clone(),
        path: body.path.clone(),
        title: body.title.clone(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.create_bookmark(&bookmark).await?;
    Ok(HttpResponse::Created().json(bookmark))
}

#[delete("/api/vaults/{vault_id}/bookmarks/{bookmark_id}")]
async fn delete_bookmark(
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> AppResult<HttpResponse> {
    let (vault_id, bookmark_id) = path.into_inner();
    state.db.get_vault(&vault_id).await?;
    state.db.delete_bookmark(&vault_id, &bookmark_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_bookmarks)
        .service(create_bookmark)
        .service(delete_bookmark);
}
