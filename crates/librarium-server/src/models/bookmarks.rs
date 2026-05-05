use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Bookmark {
    pub id: String,
    pub vault_id: String,
    pub path: String,
    pub title: String,
    pub created_at: String,
}
