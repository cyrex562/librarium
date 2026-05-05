use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vault {
    pub id: String,
    pub name: String,
    pub path: String,
    pub path_exists: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Document format used by this vault. Currently always `"markdown"`;
    /// reserved for a future `"mdx"` upgrade.
    #[serde(default = "default_document_format")]
    pub document_format: String,
}

fn default_document_format() -> String {
    "markdown".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSummary {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVaultRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VaultRole {
    Owner,
    Editor,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub user_id: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedUserProfile {
    pub id: String,
    pub username: String,
    pub is_admin: bool,
    pub must_change_password: bool,
    pub groups: Vec<GroupInfo>,
    pub auth_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: String,
    pub username: String,
    pub is_admin: bool,
    pub must_change_password: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub token_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

// ── TOTP 2FA types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpEnrollResponse {
    /// The otpauth:// URI for QR code generation.
    pub otpauth_url: String,
    /// The raw base32-encoded secret (for manual entry).
    pub secret: String,
    /// One-time backup codes.
    pub backup_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpVerifyRequest {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpLoginVerifyResponse {
    pub success: bool,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

// ── Invitation types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInviteRequest {
    /// Role to grant when invite is accepted (editor, viewer).
    pub role: String,
    /// Vault to grant access to (optional — if omitted, server-level invite).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<String>,
    /// Expiration in hours from now. Default 72.
    #[serde(default = "default_invite_hours")]
    pub expires_in_hours: u64,
}

fn default_invite_hours() -> u64 {
    72
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteInfo {
    pub id: String,
    pub token: String,
    pub role: String,
    pub vault_id: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub accepted: bool,
    pub accepted_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptInviteRequest {
    pub token: String,
    pub username: String,
    pub password: String,
}

// ── Bulk user import types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkUserEntry {
    pub username: String,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkImportResult {
    pub created: Vec<String>,
    pub failed: Vec<BulkImportError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkImportError {
    pub username: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// Optional expiration in days from now. None = never expires.
    pub expires_in_days: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: String,
    pub name: String,
    /// The full API key — only shown once at creation time.
    pub api_key: String,
    pub prefix: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub event_type: String,
    pub detail: Option<String>,
    pub ip_address: Option<String>,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserResponse {
    pub id: String,
    pub username: String,
    pub temporary_password: String,
    pub is_admin: bool,
    pub must_change_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddGroupMemberRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareVaultWithUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub role: VaultRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareVaultWithGroupRequest {
    pub group_id: String,
    pub role: VaultRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultShareEntry {
    pub principal_type: String,
    pub principal_id: String,
    pub principal_name: String,
    pub role: VaultRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultShareList {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    pub user_shares: Vec<VaultShareEntry>,
    pub group_shares: Vec<VaultShareEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub children: Option<Vec<FileNode>>,
    pub size: Option<u64>,
    pub modified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub modified: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFileRequest {
    pub content: String,
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFileRequest {
    pub path: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub title: String,
    pub matches: Vec<SearchMatch>,
    pub score: f32,
    /// Set when the file is a typed entity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// Labels from `librarium_labels` frontmatter when present
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line_number: usize,
    pub line_text: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedSearchResult {
    pub results: Vec<SearchResult>,
    pub total_count: usize,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOutlineRequest {
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_sections: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineSection {
    pub level: u8,
    pub title: String,
    pub line_number: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteOutlineResponse {
    pub file_path: String,
    pub summary: String,
    pub sections: Vec<OutlineSection>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOrganizationSuggestionsRequest {
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_suggestions: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationSuggestionKind {
    Tag,
    Category,
    MoveToFolder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationSuggestion {
    pub id: String,
    pub kind: OrganizationSuggestionKind,
    pub confidence: f32,
    pub rationale: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationSuggestionsResponse {
    pub file_path: String,
    pub suggestions: Vec<OrganizationSuggestion>,
    pub existing_tags: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyOrganizationSuggestionRequest {
    pub file_path: String,
    pub suggestion: OrganizationSuggestion,
    #[serde(default = "default_true")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyChange {
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyOrganizationSuggestionResponse {
    pub file_path: String,
    pub applied: bool,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_file_path: Option<String>,
    pub changes: Vec<ApplyChange>,
    pub applied_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReverseAction {
    RemoveTag { tag: String },
    RestoreCategory { previous_value: Option<String> },
    MoveBack { from_path: String, to_path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlUndoReceipt {
    pub receipt_id: String,
    pub vault_id: String,
    pub file_path: String,
    pub description: String,
    pub reverse_action: ReverseAction,
    pub applied_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoMlActionResponse {
    pub receipt_id: String,
    pub undone: bool,
    pub description: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    pub vault_id: String,
    pub path: String,
    pub event_type: FileChangeType,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed { from: String, to: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub theme: String,
    pub editor_mode: EditorMode,
    pub font_size: u16,
    pub window_layout: Option<String>,
    pub icon_map: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EditorMode {
    Raw,
    SideBySide,
    FormattedRaw,
    FullyRendered,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            editor_mode: EditorMode::SideBySide,
            font_size: 14,
            window_layout: None,
            icon_map: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUploadSessionRequest {
    pub filename: String,
    pub path: String,
    pub total_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSessionResponse {
    pub session_id: String,
    pub uploaded_bytes: u64,
    pub total_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    FileChanged {
        vault_id: String,
        path: String,
        event_type: FileChangeType,
        etag: Option<String>,
        timestamp: i64,
    },
    /// Broadcast after a full vault reindex completes.
    ReindexComplete {
        vault_id: String,
        file_count: i64,
        duration_ms: i64,
    },
    SyncPing,
    SyncPong {
        server_time: i64,
    },
    Error {
        message: String,
    },
}

// ── Document format abstraction ──────────────────────────────────────────────

/// The output of rendering a document source into HTML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedDocument {
    /// Rendered HTML body (does not include `<html>`/`<body>` wrapper tags).
    pub html: String,
}

/// Parsed document frontmatter as a JSON value.
///
/// Typically a `Value::Object` mapping key names to values.  `Value::Null`
/// is returned when the source document contains no frontmatter block.
pub type Frontmatter = Value;

/// Abstraction over document formats (Markdown, MDX, …).
///
/// All document rendering and metadata extraction goes through this trait so
/// that a future `MdxParser` can be swapped in without touching call sites.
///
/// Implementations must be cheaply cloneable behind an `Arc`; they should
/// hold no mutable per-request state.
pub trait DocumentParser: Send + Sync {
    /// Render `source` to HTML.
    fn render(&self, source: &str) -> RenderedDocument;

    /// Render `source` to HTML with optional vault-context link resolution.
    ///
    /// `vault_path` is the absolute path to the vault root (used to resolve
    /// wiki links).  `current_file` is the path of the currently open file
    /// relative to `vault_path`.  Implementations may ignore these parameters;
    /// the default delegates to [`Self::render`].
    fn render_with_context(
        &self,
        source: &str,
        vault_path: Option<&str>,
        current_file: Option<&str>,
    ) -> RenderedDocument {
        let _ = (vault_path, current_file);
        self.render(source)
    }

    /// Extract the frontmatter block from `source`.
    ///
    /// Returns `Value::Null` when no frontmatter is present.
    fn extract_frontmatter(&self, source: &str) -> Frontmatter;

    /// Return the prose body of `source` with frontmatter and any structural
    /// sentinels (e.g. `<!-- librarium:prose:begin -->`) stripped.
    fn extract_prose(&self, source: &str) -> String;
}
