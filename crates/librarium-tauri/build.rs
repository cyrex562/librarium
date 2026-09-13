// Every `#[tauri::command]` invoked from the frontend via `invoke()` must be
// explicitly allow-listed in Tauri's ACL, or every call is silently denied —
// see `capabilities/default.json` and `capabilities/mobile.json`, which
// reference the `allow-<command>` permissions this list auto-generates.
// Includes both this crate's own commands (src/lib.rs's desktop
// `invoke_handler()`) and `librarium-mobile`'s command surface (its
// `commands.rs`), since this one build script covers both platform builds.
const COMMANDS: &[&str] = &[
    // Desktop-only
    "open_directory_dialog",
    "auth_token_get",
    "auth_token_set",
    "auth_token_clear",
    "auth_reset_local_password",
    "auth_set_local_enabled",
    // Shared generic commands (desktop + mobile)
    "notify",
    "open_external_url",
    "write_binary_file",
    "frontend_log",
    "frontend_log_path",
    // Sync bridge commands (desktop's sync_bridge.rs and librarium-mobile's
    // sync.rs each register their own handler under these same names)
    "sync_add_remote",
    "sync_map_vault",
    "sync_list_remotes",
    "sync_list_remote_vaults",
    "sync_create_remote_vault",
    "sync_remove_remote",
    "sync_unmap_vault",
    "sync_status",
    "sync_start",
    "sync_stop",
    // Mobile-only: local dispatcher command surface (librarium-mobile)
    "vault_list",
    "vault_get",
    "file_tree",
    "file_read",
    "file_write",
    "file_create",
    "file_delete",
    "file_rename",
    "directory_create",
    "render_markdown",
    "render_markdown_in_vault",
    "resolve_wiki_link",
    "backlinks",
    "outgoing_links",
    "tags_list",
    "tag_files",
    "frontmatter_read",
    "frontmatter_write",
    "search",
    "search_paged",
    "search_index_rebuild",
    "search_index_size",
    "preferences_get",
    "preferences_set",
    "preferences_reset",
    "recent_list",
    "recent_record",
    "favorites_list",
    "favorites_add",
    "favorites_remove",
    "bookmarks_list",
    "bookmarks_add",
    "bookmarks_remove",
    "pairing_set",
    "pairing_get",
    "pairing_clear",
    "sync_get_policy",
    "sync_set_policy",
    "sync_reconcile_once",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
