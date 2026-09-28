//! Platform-independent core: error type, path-safe file operations, and
//! frontmatter parsing. No actix, sqlx, or tokio in the default feature set —
//! this crate is shared between `librarium-server` (which enables the `actix`
//! and `sqlx` features below) and, eventually, a thin mobile client that
//! embeds neither.

pub mod error;
pub mod file_service;
pub mod frontmatter_service;
pub mod markdown_service;
pub mod models;
pub mod note_links;
#[cfg(feature = "search")]
pub mod search_service;
pub mod typst_frontmatter;
pub mod typst_text;
pub mod wiki_link_service;
