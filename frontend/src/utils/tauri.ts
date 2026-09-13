/**
 * Tauri capability wrapper.
 *
 * All Tauri-specific API calls are centralised here so they can be:
 *  - Guarded behind an `isTauri()` check (no-ops in the browser)
 *  - Mocked in Vitest via `vi.mock('@/utils/tauri')`
 *
 * NEVER import from `@tauri-apps/*` directly in Vue components or stores.
 * Import from this module instead.
 */

import type {
  Bookmark,
  Favorite,
  FileContent,
  FileNode,
  PagedSearchResult,
  TagEntry,
  UserPreferences,
  Vault,
} from '@/api/types';

// ── Sync API types ──────────────────────────────────────────────────────────
export interface SyncRemoteDto {
  id: string;
  base_url: string;
  enabled: boolean;
}

export type SyncVaultState = 'offline' | 'connecting' | 'syncing' | 'catching_up' | 'live';

export interface SyncVaultStatus {
  remote_id: string;
  local_vault_id: string;
  state: SyncVaultState;
  last_synced_seq: number;
  pending_outbox: number;
  last_error: string | null;
  /** Files processed so far in the current reconcile pass. */
  synced: number;
  /** Total files to process in the current reconcile pass (0 when idle). */
  total: number;
}

/**
 * The paired remote's connection info, as surfaced by `librarium-mobile`'s
 * opinionated single-remote `pairing_*` commands (#54). The API key itself
 * never appears here, only whether one is currently stored.
 */
export interface PairingInfo {
  base_url: string;
  key_present: boolean;
}

/**
 * Returns `true` when the app is running inside a Tauri WebView.
 *
 * In a normal browser the `__TAURI_INTERNALS__` object injected by Tauri's
 * IPC bridge is absent, so this reliably distinguishes the two environments.
 */
export const isTauri = (): boolean =>
  typeof window !== 'undefined' &&
  ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);

/**
 * Extract a human-readable message from a failed `invoke()` call.
 *
 * Tauri rejects with whatever the Rust command's `Err` value serializes to.
 * Every command in this codebase returns `Result<T, String>`, so that
 * rejection is a plain string — which has no `.message` property. Callers
 * that wrote `e?.message ?? fallback` therefore always got `fallback`,
 * silently discarding the real error text (e.g. "connection refused",
 * "invalid API key") in favor of a generic message. Use this instead.
 */
export function tauriErrorMessage(e: unknown, fallback: string): string {
  if (typeof e === 'string' && e) return e;
  if (e instanceof Error && e.message) return e.message;
  return fallback;
}

/**
 * Open a native directory picker dialog.
 *
 * Returns the selected directory path, or `null` when:
 *  - The user cancels the dialog
 *  - The app is running in a browser (not Tauri)
 */
export const openDirectoryDialog = async (): Promise<string | null> => {
  if (!isTauri()) return null;
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const result = await open({ directory: true, multiple: false });
    return typeof result === 'string' ? result : null;
  } catch {
    return null;
  }
};

/**
 * Open a native file open dialog filtered by the given extensions.
 *
 * Returns the selected file path, or `null` when cancelled / browser context.
 *
 * @param extensions - Array of extensions without leading dot, e.g. `['md', 'txt']`
 */
export const openFileDialog = async (extensions?: string[]): Promise<string | null> => {
  if (!isTauri()) return null;
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const filters = extensions?.length
      ? [{ name: 'Files', extensions }]
      : undefined;
    const result = await open({ directory: false, multiple: false, filters });
    return typeof result === 'string' ? result : null;
  } catch {
    return null;
  }
};

/**
 * Open a native save dialog.
 *
 * Returns the chosen save path or `null` when cancelled / browser context.
 */
export const saveFileDialog = async (
  defaultName?: string,
  extensions?: string[],
): Promise<string | null> => {
  if (!isTauri()) return null;
  try {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const filters = extensions?.length
      ? [{ name: 'Files', extensions }]
      : undefined;
    const result = await save({ defaultPath: defaultName, filters });
    return typeof result === 'string' ? result : null;
  } catch {
    return null;
  }
};

/**
 * Open a URL in the OS default handler (browser for http/https, mail client
 * for mailto, phone dialer for tel).
 *
 * Desktop: routes through the `open_external_url` Tauri command so the
 * WebView never navigates out of the app shell. Non-allowed schemes are
 * rejected on the Rust side.
 *
 * Browser: falls back to `window.open` with `noopener,noreferrer` so the
 * opener/referrer don't leak to the target.
 *
 * Returns `true` when the open call was dispatched, `false` on any error
 * (unsupported scheme, popup blocker, IPC failure).
 */
export const openExternalUrl = async (url: string): Promise<boolean> => {
  if (isTauri()) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('open_external_url', { url });
      return true;
    } catch {
      return false;
    }
  }
  try {
    const w = window.open(url, '_blank', 'noopener,noreferrer');
    return !!w;
  } catch {
    return false;
  }
};

// ── Durable refresh-token store (desktop only) ─────────────────────────────
//
// A disk-backed shadow of the refresh token that lives in the app's data_dir
// (`{data_dir}/session.json` — portable-mode aware). The WebView localStorage
// copy is still the fast path; this one exists so a wipe of the WebView
// UserData folder does not force a re-login when the data_dir is otherwise
// intact. Every wrapper here is best-effort: any thrown error is swallowed
// so a failing store never breaks the primary localStorage flow.

/**
 * Read the persisted refresh token from disk.
 *
 * Returns `null` in a browser context, when the file is absent, or on any
 * IPC/read error (the caller then falls through to the localStorage value).
 */
export const authTokenGet = async (): Promise<string | null> => {
  if (!isTauri()) return null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const token = await invoke<string | null>('auth_token_get');
    return token ?? null;
  } catch {
    return null;
  }
};

/**
 * Mirror the current refresh token to disk.
 *
 * Called after every successful login/refresh so the disk copy stays in
 * lockstep with WebView localStorage. Silent no-op in a browser context.
 */
/**
 * Reset the local desktop admin's password without knowing the old one.
 *
 * Desktop only — the underlying command exists only in the Tauri shell, and
 * deliberately is not an HTTP route: the embedded server is loopback, where any
 * local process (or the browser build) could reach an endpoint.
 *
 * Unlike the other wrappers here this one does NOT swallow errors: the caller
 * is a user-facing recovery form and must show what went wrong.
 */
export const resetLocalPassword = async (
  username: string,
  newPassword: string,
): Promise<void> => {
  if (!isTauri()) throw new Error('Password reset is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('auth_reset_local_password', { username, newPassword });
};

/**
 * Turn password protection on or off for this desktop instance.
 *
 * Takes effect on the next launch: the server reads `AppConfig` once at
 * startup. Desktop only; errors propagate so the caller can show them.
 */
export const setLocalAuthEnabled = async (
  enabled: boolean,
  username?: string,
  newPassword?: string,
): Promise<void> => {
  if (!isTauri()) throw new Error('Password protection is managed by your server administrator');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('auth_set_local_enabled', { enabled, username, newPassword });
};

export const authTokenSet = async (token: string): Promise<void> => {
  if (!isTauri()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('auth_token_set', { token });
  } catch {
    // best-effort — localStorage remains the source of truth for this run
  }
};

/**
 * Wipe the disk-backed refresh token on logout.
 *
 * Called from `authStore.logout()` alongside the localStorage clear so a
 * subsequent boot cannot restore a revoked token from the durable fallback.
 */
export const authTokenClear = async (): Promise<void> => {
  if (!isTauri()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('auth_token_clear');
  } catch {
    // best-effort
  }
};

// ── Desktop sync API wrappers ───────────────────────────────────────────────

/**
 * Add a new sync remote.
 *
 * @param baseUrl - The base URL of the sync server
 * @param apiKey - The API key for authentication
 * @returns The ID of the created remote
 * @throws Error in browser context
 */
export const syncAddRemote = async (
  baseUrl: string,
  apiKey: string,
): Promise<string> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('sync_add_remote', { baseUrl, apiKey });
};

/**
 * Map a remote vault to a local vault.
 *
 * @param remoteId - The ID of the remote
 * @param localVaultId - The ID of the local vault
 * @param remoteVaultId - The ID of the remote vault
 * @throws Error in browser context
 */
export const syncMapVault = async (
  remoteId: string,
  localVaultId: string,
  remoteVaultId: string,
): Promise<void> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('sync_map_vault', { remoteId, localVaultId, remoteVaultId });
};

/**
 * List all configured sync remotes.
 *
 * @returns Array of remote DTOs
 * @throws Error in browser context
 */
export const syncListRemotes = async (): Promise<SyncRemoteDto[]> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('sync_list_remotes');
};

/**
 * List vaults available on a remote.
 *
 * @param remoteId - The ID of the remote
 * @returns Array of vaults from the remote
 * @throws Error in browser context
 */
export const syncListRemoteVaults = async (
  remoteId: string,
): Promise<Vault[]> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('sync_list_remote_vaults', { remoteId });
};

/**
 * Create a new vault on a remote.
 *
 * @param remoteId - The ID of the remote
 * @param name - The name for the new vault
 * @returns The created vault
 * @throws Error in browser context
 */
export const syncCreateRemoteVault = async (
  remoteId: string,
  name: string,
): Promise<Vault> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('sync_create_remote_vault', { remoteId, name });
};

/**
 * Remove a sync remote.
 *
 * @param remoteId - The ID of the remote to remove
 * @throws Error in browser context
 */
export const syncRemoveRemote = async (remoteId: string): Promise<void> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('sync_remove_remote', { remoteId });
};

/**
 * Unmap a vault from a remote.
 *
 * @param remoteId - The ID of the remote
 * @param localVaultId - The ID of the local vault
 * @throws Error in browser context
 */
export const syncUnmapVault = async (
  remoteId: string,
  localVaultId: string,
): Promise<void> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('sync_unmap_vault', { remoteId, localVaultId });
};

/**
 * Get the current sync status of all vaults.
 *
 * @returns Array of vault sync statuses, or empty array in browser context
 */
export const syncStatus = async (): Promise<SyncVaultStatus[]> => {
  if (!isTauri()) return [];
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('sync_status');
};

/**
 * Start the sync service.
 *
 * @throws Error in browser context
 */
export const syncStart = async (): Promise<void> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('sync_start');
};

/**
 * Stop the sync service.
 *
 * No-op in browser context.
 */
export const syncStop = async (): Promise<void> => {
  if (!isTauri()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('sync_stop');
};

// ── Background sync policy + manual trigger (Route C, #64) ──────────────────
//
// The Android background reconcile service (`crates/librarium-tauri/src/
// background_sync.rs`) reads this policy on each wake before deciding
// whether to call `sync_reconcile_once`; these wrappers are also how the
// settings UI reads/writes it and offers a manual "sync now".

export interface SyncPolicy {
  wifi_only: boolean;
  battery_threshold: number;
}

/**
 * Get the background-sync policy (Wi-Fi-only, battery threshold).
 *
 * @returns The stored policy, or the default (`wifi_only: true`,
 *   `battery_threshold: 20`) in browser context.
 */
export const syncGetPolicy = async (): Promise<SyncPolicy> => {
  if (!isTauri()) return { wifi_only: true, battery_threshold: 20 };
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('sync_get_policy');
};

/**
 * Set the background-sync policy.
 *
 * No-op in browser context.
 */
export const syncSetPolicy = async (policy: SyncPolicy): Promise<void> => {
  if (!isTauri()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('sync_set_policy', { policy });
};

/**
 * Manual "sync now": one coarse reconcile pass for every mapped vault, with
 * no live socket kept open afterward — the same one-shot the background
 * service's periodic tick calls.
 *
 * @throws Error in browser context.
 */
export const syncReconcileOnce = async (): Promise<void> => {
  if (!isTauri()) throw new Error('sync is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('sync_reconcile_once');
};

/**
 * Start the Android background reconcile service (`tauri-plugin-background-
 * service`'s foreground service, wrapping `MobileSyncService`). Desktop/
 * browser never registers this plugin's commands at all (see librarium-
 * tauri's Cargo.toml — mobile-target-gated), so this must only ever be
 * called from the mobile shell's local-mode bootstrap, never unconditionally.
 *
 * No-op in browser context (mirrors `syncStop`'s soft-fail style, since this
 * is fire-and-forget bootstrap plumbing rather than a user action with an
 * error path worth surfacing).
 */
export const startBackgroundSyncService = async (): Promise<void> => {
  if (!isTauri()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('plugin:background-service|start', {
    serviceLabel: 'Librarium sync',
    foregroundServiceType: 'dataSync',
  });
};

// ── Mobile pairing API wrappers (Route C, #54/#60) ───────────────────────────
//
// The opinionated single-remote counterpart to the general `sync_*` remote
// API above: `pairing_set` validates the URL/key against the remote before
// persisting anything (the API key itself never round-trips back through
// `pairing_get` — only whether one is stored), then registers it under the
// fixed remote id `"primary"` (`crates/librarium-mobile/src/sync.rs`).

/**
 * Validate and store a remote's base URL + API key, registering it as the
 * paired remote.
 *
 * @throws Error if the URL/key can't be validated against the remote, or in
 *   browser context.
 */
export const pairingSet = async (baseUrl: string, apiKey: string): Promise<void> => {
  if (!isTauri()) throw new Error('pairing is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('pairing_set', { baseUrl, apiKey });
};

/**
 * Get the currently paired remote's info, or `null` if nothing is paired
 * yet (including in browser context, mirroring `syncStatus`'s soft-fail
 * style so callers don't need to special-case non-Tauri environments).
 */
export const pairingGet = async (): Promise<PairingInfo | null> => {
  if (!isTauri()) return null;
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('pairing_get');
};

/**
 * Stop syncing, remove the paired remote, and erase its stored key.
 *
 * No-op in browser context.
 */
export const pairingClear = async (): Promise<void> => {
  if (!isTauri()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('pairing_clear');
};

// ── Mobile vault/file/render API wrappers (Route C, issue #56) ──────────────
//
// Each mirrors one `librarium-mobile` `#[tauri::command]`, invoked directly
// rather than over HTTP. Used exclusively by `api/localDispatcher.ts` — see
// that module for the route table mapping REST-shaped calls to these.

export interface MobileRenameResult {
  from: string;
  to: string;
  new_path: string;
}

export interface MobileDirectoryCreateResult {
  path: string;
}

export interface MobileResolveWikiLinkResult {
  path: string;
  exists: boolean;
  alternatives: string[];
  ambiguous: boolean;
}

export const mobileVaultList = async (): Promise<Vault[]> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('vault_list');
};

export const mobileVaultGet = async (vaultId: string): Promise<Vault> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('vault_get', { vaultId });
};

export const mobileFileTree = async (vaultId: string): Promise<FileNode[]> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('file_tree', { vaultId });
};

export const mobileFileRead = async (vaultId: string, filePath: string): Promise<FileContent> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('file_read', { vaultId, filePath });
};

export const mobileFileWrite = async (
  vaultId: string,
  filePath: string,
  content: string,
  lastModified?: string,
  frontmatter?: Record<string, unknown>,
): Promise<FileContent> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('file_write', { vaultId, filePath, content, lastModified, frontmatter });
};

export const mobileFileCreate = async (
  vaultId: string,
  filePath: string,
  content?: string,
): Promise<FileContent> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('file_create', { vaultId, filePath, content });
};

export const mobileFileDelete = async (vaultId: string, filePath: string): Promise<void> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('file_delete', { vaultId, filePath });
};

export const mobileFileRename = async (
  vaultId: string,
  from: string,
  to: string,
  strategy?: string,
): Promise<MobileRenameResult> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('file_rename', { vaultId, from, to, strategy });
};

export const mobileDirectoryCreate = async (
  vaultId: string,
  dirPath: string,
): Promise<MobileDirectoryCreateResult> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('directory_create', { vaultId, dirPath });
};

export const mobileRenderMarkdown = async (content: string): Promise<string> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('render_markdown', { content });
};

export const mobileRenderMarkdownInVault = async (
  vaultId: string,
  content: string,
  currentFile?: string,
): Promise<string> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('render_markdown_in_vault', { vaultId, content, currentFile });
};

export const mobileResolveWikiLink = async (
  vaultId: string,
  link: string,
  currentFile?: string,
): Promise<MobileResolveWikiLinkResult> => {
  if (!isTauri()) throw new Error('local vault access is only available in the mobile app');
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('resolve_wiki_link', { vaultId, link, currentFile });
};

// ── Mobile search/tags/backlinks/metadata API wrappers (issue #57) ──────────

const MOBILE_UNAVAILABLE = 'local vault access is only available in the mobile app';

export const mobileSearchPaged = async (
  vaultId: string,
  query: string,
  page: number,
  pageSize: number,
): Promise<PagedSearchResult> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('search_paged', { vaultId, query, page, pageSize });
};

export const mobileTagsList = async (vaultId: string): Promise<TagEntry[]> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('tags_list', { vaultId });
};

export const mobileTagFiles = async (vaultId: string, tag: string): Promise<string[]> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('tag_files', { vaultId, tag });
};

export interface MobileLinkedNote {
  path: string;
  title: string;
}

export const mobileBacklinks = async (vaultId: string, path: string): Promise<MobileLinkedNote[]> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('backlinks', { vaultId, path });
};

export const mobilePreferencesGet = async (): Promise<UserPreferences> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('preferences_get');
};

export const mobilePreferencesSet = async (preferences: UserPreferences): Promise<void> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('preferences_set', { preferences });
};

export const mobilePreferencesReset = async (): Promise<UserPreferences> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('preferences_reset');
};

export const mobileRecentList = async (vaultId: string): Promise<string[]> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('recent_list', { vaultId });
};

export const mobileRecentRecord = async (vaultId: string, path: string): Promise<void> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('recent_record', { vaultId, path });
};

export const mobileFavoritesList = async (vaultId: string): Promise<Favorite[]> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('favorites_list', { vaultId });
};

export const mobileFavoritesAdd = async (vaultId: string, path: string): Promise<Favorite> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('favorites_add', { vaultId, path });
};

export const mobileFavoritesRemove = async (vaultId: string, path: string): Promise<void> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('favorites_remove', { vaultId, path });
};

export const mobileBookmarksList = async (vaultId: string): Promise<Bookmark[]> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('bookmarks_list', { vaultId });
};

export const mobileBookmarksAdd = async (
  vaultId: string,
  path: string,
  title: string,
): Promise<Bookmark> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke('bookmarks_add', { vaultId, path, title });
};

export const mobileBookmarksRemove = async (vaultId: string, bookmarkId: string): Promise<void> => {
  if (!isTauri()) throw new Error(MOBILE_UNAVAILABLE);
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('bookmarks_remove', { vaultId, bookmarkId });
};
