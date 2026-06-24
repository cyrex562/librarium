// Types that mirror the Rust backend models exactly.
// Keep these in sync with src/models/mod.rs

export interface Vault {
    id: string;
    name: string;
    path: string;
    path_exists: boolean;
    created_at: string;
    updated_at: string;
    /** Always present; currently always "markdown". Reserved for future MDX support. */
    document_format: string;
}

export interface CreateVaultRequest {
    name: string;
    path?: string;
}

export interface FileNode {
    name: string;
    path: string;
    is_directory: boolean;
    children?: FileNode[];
    size?: number;
    modified?: string;
}

export interface FileContent {
    path: string;
    content: string;
    modified: string;
    frontmatter?: Record<string, unknown>;
}

export interface UpdateFileRequest {
    content: string;
    last_modified?: string;
    frontmatter?: Record<string, unknown>;
}

export interface CreateFileRequest {
    path: string;
    content?: string;
}

export interface SearchMatch {
    line_number: number;
    line_text: string;
    match_start: number;
    match_end: number;
}

export interface SearchResult {
    path: string;
    title: string;
    matches: SearchMatch[];
    score: number;
    entity_type?: string;
    labels: string[];
}

export interface PagedSearchResult {
    results: SearchResult[];
    total_count: number;
    page: number;
    page_size: number;
}

export type FileChangeType =
    | 'created'
    | 'modified'
    | 'deleted'
    | { renamed: { from: string; to: string } };

export interface FileChangeEvent {
    vault_id: string;
    path: string;
    event_type: FileChangeType;
    timestamp: string;
}

/** Editor modes persisted to the backend (must match the Rust EditorMode enum). */
export type PersistedEditorMode = 'raw' | 'side_by_side' | 'formatted_raw' | 'fully_rendered';
/** UI-level editor mode — 'structural' is client-only and never stored in UserPreferences. */
export type EditorMode = PersistedEditorMode | 'structural';

export interface UserPreferences {
    theme: string;
    editor_mode: PersistedEditorMode;
    font_size: number;
    window_layout?: string;
    icon_map?: Record<string, string>;
}

// ── Entity / Plugin schema types ──────────────────────────────────────────────

export type FieldType =
    | 'string'
    | 'text'
    | 'number'
    | 'date'
    | 'boolean'
    | 'enum'
    | 'entity_ref'
    | 'list';

export interface FieldSchema {
    key: string;
    label: string;
    field_type: FieldType;
    required: boolean;
    item_type?: FieldType;
    values: string[];
    default?: unknown;
    target_label?: string;
    relation?: string;
    description?: string;
}

export interface EntityTypeSchema {
    id: string;
    plugin_id: string;
    name: string;
    icon?: string;
    color?: string;
    template?: string;
    labels: string[];
    fields: FieldSchema[];
    display_field?: string;
    show_on_create?: string[];
}

export interface RelationTypeSchema {
    id: string;
    plugin_id: string;
    name: string;
    label: string;
    from_label?: string;
    to_label?: string;
    directed: boolean;
    inverse_label?: string;
    color?: string;
    metadata_fields: FieldSchema[];
}

export interface Entity {
    id: string;
    vault_id: string;
    path: string;
    entity_type?: string;
    plugin_id?: string;
    labels: string[];
    fields: Record<string, unknown>;
    modified_at: string;
    indexed_at: string;
}

export interface EntityRelation {
    id: string;
    source_entity_id: string;
    target_entity_id: string;
    target_path: string;
    relation_type?: string;
    label: string;
    directed: boolean;
    metadata: Record<string, unknown>;
    plugin_id?: string;
    is_inverse: boolean;
}

export interface GraphNode {
    id: string;
    path: string;
    entity_type?: string;
    labels: string[];
    title: string;
    color?: string;
    icon?: string;
}

export interface GraphEdge {
    id: string;
    source: string;
    target: string;
    label: string;
    relation_type?: string;
    color?: string;
    is_inverse: boolean;
}

export interface GraphData {
    nodes: GraphNode[];
    edges: GraphEdge[];
}

// Upload session types
export interface CreateUploadSessionRequest {
    filename: string;
    path: string;
    total_size?: number;
}

export interface UploadSessionResponse {
    session_id: string;
    uploaded_bytes: number;
    total_size?: number;
}

export interface ImportCandidate {
    file: File;
    relativePath: string;
}

export interface ImportProgress {
    totalFiles: number;
    completedFiles: number;
    totalBytes: number;
    uploadedBytes: number;
    currentFile?: string;
}

export interface ImportResultItem {
    path: string;
    filename: string;
    size?: number;
    skipped?: boolean;
}

export interface ImportResult {
    uploaded: ImportResultItem[];
    skipped: ImportResultItem[];
    directoryCount: number;
    totalBytes: number;
}

export interface Bookmark {
    id: string;
    vault_id: string;
    path: string;
    title: string;
    created_at: string;
}

export interface TagEntry {
    tag: string;
    count: number;
    files: string[];
}

export interface BacklinkEntry {
    path: string;
    title: string;
}

export interface GenerateOutlineRequest {
    file_path: string;
    content?: string;
    max_sections?: number;
}

export interface OutlineSection {
    level: number;
    title: string;
    line_number: number;
}

export interface NoteOutlineResponse {
    file_path: string;
    summary: string;
    sections: OutlineSection[];
    generated_at: string;
}

export interface AnalyzeNoteRequest {
    file_path: string;
    content?: string;
}

export interface NoteTask {
    text: string;
    done: boolean;
    line_number: number;
}

export interface Keyphrase {
    phrase: string;
    score: number;
}

export interface NoteAnalysis {
    file_path: string;
    title?: string;
    summary: string;
    sections: OutlineSection[];
    word_count: number;
    inline_tags: string[];
    frontmatter_tags: string[];
    wiki_links: string[];
    tasks: NoteTask[];
    keyphrases: Keyphrase[];
    tier: string;
    generated_at: string;
}

export interface GenerateOrganizationSuggestionsRequest {
    file_path: string;
    content?: string;
    max_suggestions?: number;
}

export type OrganizationSuggestionKind = 'tag' | 'category' | 'move_to_folder';

export interface OrganizationSuggestion {
    id: string;
    kind: OrganizationSuggestionKind;
    confidence: number;
    rationale: string;
    tag?: string;
    category?: string;
    target_folder?: string;
}

export interface OrganizationSuggestionsResponse {
    file_path: string;
    suggestions: OrganizationSuggestion[];
    existing_tags: string[];
    generated_at: string;
}

export interface ApplyOrganizationSuggestionRequest {
    file_path: string;
    suggestion: OrganizationSuggestion;
    dry_run?: boolean;
}

export interface ApplyChange {
    kind: string;
    description: string;
}

export interface ApplyOrganizationSuggestionResponse {
    file_path: string;
    applied: boolean;
    dry_run: boolean;
    updated_file_path?: string;
    changes: ApplyChange[];
    applied_at: string;
    receipt_id?: string;
}

export interface UndoMlActionResponse {
    receipt_id: string;
    undone: boolean;
    description: string;
    file_path: string;
}

export interface SessionInfo {
    token_id: string;
    created_at: string;
    expires_at: string;
}

export interface TotpEnrollResponse {
    /** otpauth:// URI for QR-code generation. */
    otpauth_url: string;
    /** Raw base32-encoded secret for manual entry. */
    secret: string;
    backup_codes: string[];
}

export interface AuditLogEntry {
    id: number;
    timestamp: string;
    user_id: string | null;
    username: string | null;
    event_type: string;
    detail: string | null;
    ip_address: string | null;
    success: boolean;
}

export interface ApiKeyInfo {
    id: string;
    name: string;
    prefix: string;
    user_id: string;
    created_at: string;
    expires_at: string | null;
    revoked: boolean;
}

export interface CreateApiKeyRequest {
    name: string;
    /** Expiration in days from now. Omit for a non-expiring key. */
    expires_in_days?: number;
}

export interface CreateApiKeyResponse {
    id: string;
    name: string;
    /** Full API key — shown only once at creation time. */
    api_key: string;
    prefix: string;
    expires_at: string | null;
}

export interface CreateInviteRequest {
    role: string;
    vault_id?: string;
    expires_in_hours?: number;
}

export interface InviteInfo {
    id: string;
    token: string;
    role: string;
    vault_id: string | null;
    created_by: string;
    created_at: string;
    expires_at: string;
    accepted: boolean;
    accepted_by: string | null;
}

export interface AcceptInviteRequest {
    token: string;
    username: string;
    password: string;
}

export interface BulkUserEntry {
    username: string;
    is_admin?: boolean;
    temporary_password?: string;
}

export interface BulkImportError {
    username: string;
    error: string;
}

export interface BulkImportResult {
    created: string[];
    failed: BulkImportError[];
}

// Auth types
export interface LoginRequest {
    username: string;
    password: string;
}

export interface LoginResponse {
    access_token: string;
    refresh_token: string;
    expires_in: number; // seconds
    totp_required?: boolean;
}

export interface TotpLoginVerifyResponse {
    success: boolean;
    access_token: string;
    refresh_token: string;
    expires_in: number;
}

export type VaultRole = 'owner' | 'editor' | 'viewer';

export interface GroupInfo {
    id: string;
    name: string;
    created_at: string;
}

export interface GroupMember {
    user_id: string;
    username: string;
}

export interface AuthenticatedUserProfile {
    id: string;
    username: string;
    is_admin: boolean;
    must_change_password: boolean;
    groups: GroupInfo[];
    auth_method: string;
}

export interface AdminUser {
    id: string;
    username: string;
    is_admin: boolean;
    is_active: boolean;
    must_change_password: boolean;
    created_at: string;
}

export interface CreateUserRequest {
    username: string;
    temporary_password?: string;
    is_admin?: boolean;
}

export interface CreateUserResponse {
    id: string;
    username: string;
    temporary_password: string;
    is_admin: boolean;
    must_change_password: boolean;
}

export interface ChangePasswordRequest {
    current_password: string;
    new_password: string;
}

export interface CreateGroupRequest {
    name: string;
}

export interface AddGroupMemberRequest {
    user_id?: string;
    username?: string;
}

export interface ShareVaultWithUserRequest {
    user_id?: string;
    username?: string;
    role: VaultRole;
}

export interface ShareVaultWithGroupRequest {
    group_id: string;
    role: VaultRole;
}

export interface VaultShareEntry {
    principal_type: string;
    principal_id: string;
    principal_name: string;
    role: VaultRole;
}

export interface VaultShareList {
    owner_user_id?: string;
    user_shares: VaultShareEntry[];
    group_shares: VaultShareEntry[];
}

// WebSocket message envelope
export type WsMessage =
    | { type: 'FileChanged'; vault_id: string; path: string; event_type: FileChangeType; etag?: string; timestamp: number }
    | { type: 'ReindexComplete'; vault_id: string; file_count: number; duration_ms: number }
    | { type: 'SyncPing' }
    | { type: 'SyncPong'; server_time: number }
    | { type: 'Error'; message: string };

// ── Canvas types (Obsidian-compatible .canvas JSON format) ───────────────────

export type CanvasNodeSide = 'top' | 'right' | 'bottom' | 'left';

export interface CanvasNodeBase {
    id: string;
    x: number;
    y: number;
    width: number;
    height: number;
    color?: string;
}

export interface CanvasTextNode extends CanvasNodeBase {
    type: 'text';
    text: string;
}

export interface CanvasFileNode extends CanvasNodeBase {
    type: 'file';
    file: string;
    subpath?: string;
}

export interface CanvasLinkNode extends CanvasNodeBase {
    type: 'link';
    url: string;
}

export interface CanvasGroupNode extends CanvasNodeBase {
    type: 'group';
    label?: string;
    background?: string;
    backgroundStyle?: 'cover' | 'ratio' | 'repeat';
}

export type CanvasNode = CanvasTextNode | CanvasFileNode | CanvasLinkNode | CanvasGroupNode;

export interface CanvasEdge {
    id: string;
    fromNode: string;
    fromSide: CanvasNodeSide;
    toNode: string;
    toSide: CanvasNodeSide;
    color?: string;
    label?: string;
}

export interface CanvasData {
    nodes: CanvasNode[];
    edges: CanvasEdge[];
}

// UI-only tab type
export type FileType = 'markdown' | 'image' | 'pdf' | 'text' | 'audio' | 'video' | 'graph' | 'canvas' | 'other';

export interface Tab {
    id: string;
    filePath: string;
    fileName: string;
    content: string;
    modified: string;
    isDirty: boolean;
    paneId: string;
    fileType: FileType;
    frontmatter?: Record<string, unknown>;
}

export interface Pane {
    id: string;
    flex: number;
    activeTabId: string | null;
}
