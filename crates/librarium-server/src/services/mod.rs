pub mod auth_provider;
pub mod credentials;
pub mod embedding_service;
pub mod entity_service;
// `file_service`, `frontmatter_service`, `markdown_service`,
// `wiki_link_service`, and `search_service` moved to `librarium-core` (LIB
// Route-C-01 / -02 / -03): none of them need actix/sqlx, so they're shared
// with a future thin mobile client that has no server to call for rendering
// or search. Re-exported as modules here so every existing
// `crate::services::…` path keeps resolving.
pub use librarium_core::file_service;
pub use librarium_core::frontmatter_service;
pub use librarium_core::markdown_service;
pub use librarium_core::search_service;
pub use librarium_core::wiki_link_service;
pub mod image_service;
pub mod label_service;
pub mod ldap_provider;
pub mod local_lm_service;
pub mod ml_service;
pub mod oidc_provider;
pub mod organize_service;
pub mod plugin_api;
pub mod plugin_service;
pub mod reindex_service;
pub mod relation_service;
pub mod schema_service;
pub mod template_service;

pub use auth_provider::{
    authenticate_username_password, validate_password_policy, AuthProviderKind,
    AuthenticatedPrincipal,
};
pub use credentials::{hash_password, CredentialService};
pub use embedding_service::{embedder, Embedder};
pub use entity_service::{Entity, EntityService};
pub use file_service::{FileService, RenameStrategy};
pub use image_service::ImageService;
pub use label_service::{Label, LabelService};
pub use markdown_service::{MarkdownParser, MarkdownService, RenderOptions};
pub use ml_service::MlService;
pub use plugin_api::{Command, Event, EventBus, EventType, PluginApi, PluginStorage};
pub use plugin_service::{resolve_plugins_dir, PluginService};
pub use reindex_service::ReindexService;
pub use relation_service::{Relation, RelationService};
pub use schema_service::{EntityTypeRegistry, RelationTypeRegistry, SchemaService};
pub use search_service::SearchIndex;
pub use template_service::TemplateService;
pub use wiki_link_service::{rewrite_wiki_links, FileIndex, ResolvedLink, WikiLinkResolver};
