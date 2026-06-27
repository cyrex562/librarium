use chrono::{DateTime, Utc};
use sqlx::FromRow;

pub mod bookmarks;
pub mod graph;
pub mod plugin;
pub mod schema;

pub use schema::{
    EntityTypeSchema, FieldSchema, FieldType, PluginLabelDeclaration, RelationTypeSchema,
};

pub use librarium_types::{
    AcceptInviteRequest, AddGroupMemberRequest, AdminUser, AnalyzeNoteRequest, ApiKeyInfo,
    ApplyChange, ApplyOrganizationSuggestionRequest, ApplyOrganizationSuggestionResponse,
    ApplyPlanRequest, ApplyPlanResponse, ApplyPlanRow, ApplyPlanRowResult, AuditLogEntry,
    AuthenticatedUserProfile, BulkImportError, BulkImportResult, BulkUserEntry,
    ChangePasswordRequest, CreateApiKeyRequest, CreateApiKeyResponse, CreateFileRequest,
    CreateGroupRequest, CreateInviteRequest, CreateUploadSessionRequest, CreateUserRequest,
    CreateUserResponse, CreateVaultRequest, EditorMode, FileChangeEvent, FileChangeType,
    FileContent, FileNode, FolderCandidate, GenerateOrganizationSuggestionsRequest,
    GenerateOutlineRequest,
    GroupInfo, GroupMember, InviteInfo, Keyphrase, MlUndoReceipt, NoteAnalysis, NoteOutlineResponse,
    NoteTask, OrganizationPlan, OrganizationPlanRow, OrganizationSuggestion,
    OrganizationSuggestionKind, OrganizationSuggestionsResponse, OrganizeVaultRequest,
    OutlineSection, PagedSearchResult, RenameSuggestionRequest, RenameSuggestionResponse,
    ReverseAction, SearchMatch, SearchResult, SessionInfo, ShareVaultWithGroupRequest,
    ShareVaultWithUserRequest, TotpEnrollResponse, TotpLoginVerifyResponse, TotpVerifyRequest,
    UndoMlActionResponse, UpdateFileRequest, UploadSessionResponse, UserPreferences, Vault,
    VaultRole, VaultShareEntry, VaultShareList, WsMessage,
};

#[derive(Debug, Clone, FromRow)]
pub(crate) struct VaultRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
    #[sqlx(default)]
    pub document_format: Option<String>,
}

impl From<VaultRow> for Vault {
    fn from(row: VaultRow) -> Self {
        let path_exists = std::path::Path::new(&row.path).exists();
        Self {
            id: row.id,
            name: row.name,
            path: row.path,
            path_exists,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            document_format: row
                .document_format
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "markdown".to_string()),
        }
    }
}
