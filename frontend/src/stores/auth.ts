import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { apiLogin, apiRefreshToken, apiLogout, apiMe, apiChangePassword, apiVerifyTotpLogin, apiOidcCallback, apiGetHealth, isSessionInvalid } from '@/api/client';
import type { LoginResponse, AuthenticatedUserProfile } from '@/api/types';
import { isTauri, authTokenGet, authTokenSet, authTokenClear } from '@/utils/tauri';
import { getLogger } from '@/utils/logger';

const log = getLogger('auth');

const ACCESS_TOKEN_KEY = 'obsidian_access_token';
const EXPIRES_AT_KEY = 'obsidian_token_expires_at';
const PENDING_TOTP_KEY = 'obsidian_pending_totp';
// Refresh-token persistence. In a normal browser the refresh token lives only
// in an HttpOnly cookie so page JS (and any XSS) cannot read it. The Tauri
// desktop WebView, however, does not reliably persist that HttpOnly cookie
// across app restarts, so on desktop we also persist the refresh token in
// localStorage and send it in the /api/auth/refresh body — which the server
// already accepts as a fallback. Loopback-only origin + no third-party content
// makes localStorage acceptable for this deployment.
const REFRESH_TOKEN_KEY = 'obsidian_refresh_token';

export const useAuthStore = defineStore('auth', () => {
    const persistRefreshTokenLocally = isTauri();
    if (!persistRefreshTokenLocally) {
        // Purge any refresh token left over from an older desktop-mode session
        // so the browser build never ships one to disk.
        try { localStorage.removeItem(REFRESH_TOKEN_KEY); } catch { /* no-op */ }
    }

    const accessToken = ref<string | null>(localStorage.getItem(ACCESS_TOKEN_KEY));
    // On desktop (Tauri) the refresh token is durably stored in localStorage
    // and sent in the request body — the HttpOnly cookie is best-effort.
    // In the browser build it stays memory-only; the HttpOnly cookie is durable.
    const refreshToken = ref<string | null>(
        persistRefreshTokenLocally ? localStorage.getItem(REFRESH_TOKEN_KEY) : null,
    );
    const expiresAt = ref<number>(parseInt(localStorage.getItem(EXPIRES_AT_KEY) ?? '0', 10));
    const pendingTotp = ref(localStorage.getItem(PENDING_TOTP_KEY) === 'true');
    const profile = ref<AuthenticatedUserProfile | null>(null);
    const loadingProfile = ref(false);
    let refreshPromise: Promise<void> | null = null;

    // Capture what the store finds in localStorage at boot. This is the very
    // first thing to check after an unexpected drop to /login: was the token
    // wiped BEFORE the store initialised (persistence failure), or AFTER
    // (a downstream logout() call — see the `logout` log entry for its
    // caller stack).
    log.info('store init', {
        isTauri: persistRefreshTokenLocally,
        haveAccessToken: !!accessToken.value,
        haveRefreshToken: !!refreshToken.value,
        expiresAt: expiresAt.value,
        pendingTotp: pendingTotp.value,
        expiresInMs: expiresAt.value - Date.now(),
    });

    const isAuthenticated = computed(() => !!accessToken.value && !pendingTotp.value);
    // Deliberately NOT a Vue computed. `Date.now()` is not a reactive source,
    // so a `computed` here would cache its first result after each `expiresAt`
    // change and keep returning it — even hours later, after the token had
    // in fact expired. That silently disabled `ensureFresh` for the rest of
    // the session (see the diagnostic log walkthrough in PR #29). A plain
    // function re-reads the wall clock on every call, which is what callers
    // like `ensureFresh` actually need. 60-second margin so a refresh fires
    // before we hand a nearly-expired token to a downstream request.
    function isExpired(): boolean {
        return Date.now() > expiresAt.value - 60_000;
    }
    const isAdmin = computed(() => !!profile.value?.is_admin);
    const mustChangePassword = computed(() => !!profile.value?.must_change_password);

    function _applyTokens(resp: LoginResponse) {
        accessToken.value = resp.access_token;
        refreshToken.value = resp.refresh_token;
        expiresAt.value = Date.now() + resp.expires_in * 1000;
        pendingTotp.value = !!resp.totp_required;
        localStorage.setItem(ACCESS_TOKEN_KEY, resp.access_token);
        if (persistRefreshTokenLocally) {
            localStorage.setItem(REFRESH_TOKEN_KEY, resp.refresh_token);
            // Fire-and-forget: mirror to the disk store so a WebView UserData
            // wipe (uninstall/reinstall, cache clear) can still be recovered
            // from the portable data_dir on the next boot.
            void authTokenSet(resp.refresh_token);
        }
        localStorage.setItem(EXPIRES_AT_KEY, String(expiresAt.value));
        localStorage.setItem(PENDING_TOTP_KEY, String(pendingTotp.value));
    }

    async function login(username: string, password: string) {
        log.info('login attempt', { username });
        try {
            const resp = await apiLogin(username, password);
            _applyTokens(resp);
            log.info('login success', {
                totpRequired: !!resp.totp_required,
                expiresInSec: resp.expires_in,
            });
            if (resp.totp_required) {
                profile.value = null;
                return;
            }
            await loadProfile(true);
        } catch (err) {
            log.warn('login failed', {
                status: (err as { status?: number })?.status,
                message: (err as Error)?.message ?? String(err),
            });
            throw err;
        }
    }

    async function loginWithOidc(code: string, state: string) {
        const resp = await apiOidcCallback(code, state);
        _applyTokens(resp);
        if (resp.totp_required) {
            profile.value = null;
            return;
        }
        await loadProfile(true);
    }

    async function completeTotpLogin(code: string) {
        const resp = await apiVerifyTotpLogin(code);
        _applyTokens({
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expires_in: resp.expires_in,
            totp_required: false,
        });
        await loadProfile(true);
    }

    async function refresh() {
        // Browser build: the refresh token rides in the HttpOnly cookie, so we
        // send an empty body and let the server read the cookie.
        // Desktop build: the WebView cookie doesn't survive restarts reliably,
        // so we send the persisted token in the body (server accepts either).
        log.info('refresh attempt', {
            haveBodyToken: persistRefreshTokenLocally && !!refreshToken.value,
            isTauri: persistRefreshTokenLocally,
        });
        // Retry on transient failures. A 401 means the token is truly invalid
        // (rotated, revoked, wrong secret) — bail immediately so the caller
        // can log out. Any other throw is treated as transient (loopback
        // hiccup on wake-from-sleep, brief WebSocket race, GC pause) and
        // retried with a short backoff; the vast majority of these clear in
        // well under a second, so a handful of retries is enough.
        const attempts = 3;
        let lastErr: unknown = null;
        for (let i = 0; i < attempts; i++) {
            try {
                const resp = persistRefreshTokenLocally && refreshToken.value
                    ? await apiRefreshToken(refreshToken.value)
                    : await apiRefreshToken();
                _applyTokens(resp);
                log.info('refresh success', { expiresInSec: resp.expires_in, attempt: i + 1 });
                return;
            } catch (err) {
                const status = (err as { status?: number })?.status;
                log.warn('refresh attempt failed', {
                    status,
                    attempt: i + 1,
                    sessionInvalid: isSessionInvalid(err),
                    message: (err as Error)?.message ?? String(err),
                });
                if (isSessionInvalid(err)) throw err;
                lastErr = err;
                if (i < attempts - 1) {
                    await new Promise((r) => setTimeout(r, 250 * (i + 1)));
                }
            }
        }
        throw lastErr;
    }

    /**
     * Clear local session state WITHOUT revoking the server-side session or
     * deleting the durable on-disk refresh token.
     *
     * This is the *involuntary* path — a 401 we could not recover from, or a
     * WebSocket auth failure. Those are frequently transient (wake-from-sleep,
     * a loopback hiccup, a server restart), and on desktop the refresh token is
     * a 10-year credential (LIB-080) backed by a disk copy. Destroying it on a
     * transient failure is what caused desktop sessions to die after days or
     * weeks of use: one spurious 401 was enough.
     *
     * Worst case here is one extra login. Worst case for `logout()` is a
     * permanently destroyed credential — so involuntary callers must use this.
     */
    function clearLocalSession() {
        accessToken.value = null;
        refreshToken.value = null;
        expiresAt.value = 0;
        pendingTotp.value = false;
        profile.value = null;
        localStorage.removeItem(ACCESS_TOKEN_KEY);
        localStorage.removeItem(REFRESH_TOKEN_KEY);
        localStorage.removeItem(EXPIRES_AT_KEY);
        localStorage.removeItem(PENDING_TOTP_KEY);
    }

    async function logout() {
        // Pass the persisted token so the server revokes THIS session only.
        // Omitting it would trigger the "logout everywhere" contract.
        log.warn('logout called', {
            haveAccessToken: !!accessToken.value,
            haveRefreshToken: !!refreshToken.value,
            // Client-side stack helps identify which caller triggered the wipe
            // (router guard, useWebSocket, App.vue mount, etc.).
            stack: new Error().stack?.split('\n').slice(1, 5).join(' | '),
        });
        try {
            if (persistRefreshTokenLocally && refreshToken.value) {
                await apiLogout(refreshToken.value);
            } else {
                await apiLogout();
            }
        } catch (err) {
            log.warn('apiLogout server call failed (localStorage still wiped)', {
                message: (err as Error)?.message ?? String(err),
            });
        }
        clearLocalSession();
        // Wipe the disk-backed copy in the same call so a subsequent boot
        // can't restore the just-revoked token from the durable fallback.
        void authTokenClear();
    }

    async function loadProfile(force = false) {
        if (!accessToken.value) {
            profile.value = null;
            return null;
        }
        if (!force && profile.value) return profile.value;

        loadingProfile.value = true;
        try {
            profile.value = await apiMe();
            return profile.value;
        } finally {
            loadingProfile.value = false;
        }
    }

    // Call before any authenticated request to ensure the token is still valid.
    async function ensureFresh() {
        if (accessToken.value && isExpired()) {
            refreshPromise ??= refresh().finally(() => {
                refreshPromise = null;
            });
            await refreshPromise;
        }
    }

    // Cached across calls in this tab — `/api/health` is cheap but there is no
    // reason to hit it on every navigation once we know the answer.  `null`
    // means "not yet checked"; the router guard awaits this before its first
    // decision.
    const serverAuthEnabled = ref<boolean | null>(null);
    let serverAuthEnabledPromise: Promise<boolean> | null = null;

    // Whether the server this desktop/browser build talks to enforces auth at
    // all. A server started with `auth.enabled = false` never bootstraps an
    // admin account (see `librarium-server`'s `lib.rs::run`), so a login
    // screen gated only on "is there a valid token" would deadlock — no token
    // can ever be obtained. The router guard uses this to bypass login
    // entirely in that case, mirroring how the local (mobile) transport
    // bypasses it for its own, unrelated reason.
    async function checkServerAuthEnabled(): Promise<boolean> {
        if (serverAuthEnabled.value !== null) return serverAuthEnabled.value;
        serverAuthEnabledPromise ??= apiGetHealth()
            .then((health) => {
                serverAuthEnabled.value = health.auth_enabled;
                return health.auth_enabled;
            })
            .catch((err) => {
                log.warn('checkServerAuthEnabled: /api/health failed — assuming auth is enabled', {
                    message: (err as Error)?.message ?? String(err),
                });
                // Fail closed: if we can't tell, keep requiring login rather
                // than accidentally granting unauthenticated access.
                serverAuthEnabled.value = true;
                return true;
            })
            .finally(() => {
                serverAuthEnabledPromise = null;
            });
        return serverAuthEnabledPromise;
    }

    async function changePassword(currentPassword: string, newPassword: string) {
        await apiChangePassword({
            current_password: currentPassword,
            new_password: newPassword,
        });
        await loadProfile(true);
    }

    // Restore the refresh token from the disk-backed store when localStorage
    // has none. Runs at app boot (before the router evaluates its first
    // guard) so a wipe of the WebView UserData folder does not force a
    // re-login when the portable/installed data_dir is still intact. No-op
    // on browser deployments and when localStorage already holds a token.
    async function bootstrapPersistence() {
        if (!persistRefreshTokenLocally) return;
        if (refreshToken.value) return;
        const fromDisk = await authTokenGet();
        if (!fromDisk) return;
        refreshToken.value = fromDisk;
        localStorage.setItem(REFRESH_TOKEN_KEY, fromDisk);
        // Force the next authenticated request to refresh — the localStorage
        // access token (if any) predates this restore and is almost
        // certainly stale. Clearing expiresAt makes isExpired() true
        // unconditionally.
        expiresAt.value = 0;
        localStorage.setItem(EXPIRES_AT_KEY, '0');
        log.info('bootstrapPersistence restored refresh token from disk');
    }

    // Called by the API client when the server returns 403 TOTP_VERIFICATION_REQUIRED
    // mid-session (e.g. the access token was issued before TOTP verification and the
    // in-memory pendingTotp flag was lost across a page reload).
    function flagPendingTotp() {
        pendingTotp.value = true;
        localStorage.setItem(PENDING_TOTP_KEY, 'true');
    }

    return {
        accessToken,
        refreshToken,
        expiresAt,
        pendingTotp,
        profile,
        loadingProfile,
        isAuthenticated,
        isExpired,
        isAdmin,
        mustChangePassword,
        login,
        loginWithOidc,
        completeTotpLogin,
        refresh,
        logout,
        clearLocalSession,
        ensureFresh,
        loadProfile,
        changePassword,
        flagPendingTotp,
        serverAuthEnabled,
        checkServerAuthEnabled,
        bootstrapPersistence,
    };
});
