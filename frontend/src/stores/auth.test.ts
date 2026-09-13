import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';

vi.mock('@/api/client', () => {
    // Declared inside the factory because vi.mock is hoisted above module-level
    // declarations; a top-level class reference here would ReferenceError.
    class ApiError extends Error {
        constructor(public status: number, message: string, public body?: unknown) {
            super(message);
            this.name = 'ApiError';
        }
    }
    return {
        ApiError,
        isSessionInvalid: (err: unknown) => err instanceof ApiError && err.status === 401,
        apiLogin: vi.fn(),
        apiRefreshToken: vi.fn(),
        apiLogout: vi.fn(),
        apiMe: vi.fn(),
        apiChangePassword: vi.fn(),
        apiVerifyTotpLogin: vi.fn(),
        apiOidcCallback: vi.fn(),
        apiGetHealth: vi.fn(),
    };
});

vi.mock('@/utils/tauri', () => ({
    isTauri: vi.fn(() => false),
    authTokenGet: vi.fn(async () => null),
    authTokenSet: vi.fn(async () => undefined),
    authTokenClear: vi.fn(async () => undefined),
}));

import {
    ApiError,
    apiLogin,
    apiLogout,
    apiMe,
    apiRefreshToken,
    apiChangePassword,
    apiVerifyTotpLogin,
    apiGetHealth,
} from '@/api/client';
import { isTauri, authTokenGet, authTokenSet, authTokenClear } from '@/utils/tauri';
import { useAuthStore } from './auth';

const mockProfile = {
    id: 'u1',
    username: 'alice',
    is_admin: false,
    must_change_password: false,
    groups: [],
    auth_method: 'password',
};

describe('useAuthStore', () => {
    beforeEach(() => {
        localStorage.clear();
        setActivePinia(createPinia());
        vi.clearAllMocks();
    });

    it('stores pending TOTP state after password login without loading the profile', async () => {
        vi.mocked(apiLogin).mockResolvedValueOnce({
            access_token: 'pending-access',
            refresh_token: 'pending-refresh',
            expires_in: 3600,
            totp_required: true,
        });

        const store = useAuthStore();
        await store.login('alice', 'correct-horse-battery-staple');

        expect(store.pendingTotp).toBe(true);
        expect(store.isAuthenticated).toBe(false);
        expect(store.profile).toBeNull();
        expect(localStorage.getItem('obsidian_pending_totp')).toBe('true');
        expect(localStorage.getItem('obsidian_access_token')).toBe('pending-access');
        // LIB-089: refresh token is held in memory + the HttpOnly cookie, never
        // persisted to localStorage.
        expect(store.refreshToken).toBe('pending-refresh');
        expect(localStorage.getItem('obsidian_refresh_token')).toBeNull();
        expect(apiMe).not.toHaveBeenCalled();
    });

    it('completes TOTP login, clears pending state, and loads the authenticated profile', async () => {
        localStorage.setItem('obsidian_pending_totp', 'true');
        localStorage.setItem('obsidian_access_token', 'pending-access');
        localStorage.setItem('obsidian_token_expires_at', String(Date.now() + 60_000));

        vi.mocked(apiVerifyTotpLogin).mockResolvedValueOnce({
            success: true,
            access_token: 'verified-access',
            refresh_token: 'verified-refresh',
            expires_in: 3600,
        });
        vi.mocked(apiMe).mockResolvedValueOnce({ ...mockProfile });

        const store = useAuthStore();
        await store.completeTotpLogin('123456');

        expect(apiVerifyTotpLogin).toHaveBeenCalledWith('123456');
        expect(apiMe).toHaveBeenCalledTimes(1);
        expect(store.pendingTotp).toBe(false);
        expect(store.isAuthenticated).toBe(true);
        expect(store.profile).toEqual(mockProfile);
        expect(localStorage.getItem('obsidian_pending_totp')).toBe('false');
        expect(localStorage.getItem('obsidian_access_token')).toBe('verified-access');
        // LIB-089: refresh token in memory only, not localStorage.
        expect(store.refreshToken).toBe('verified-refresh');
        expect(localStorage.getItem('obsidian_refresh_token')).toBeNull();
    });

    it('passes the refresh token to logout and clears pending TOTP auth state', async () => {
        localStorage.setItem('obsidian_pending_totp', 'true');
        localStorage.setItem('obsidian_access_token', 'pending-access');
        localStorage.setItem('obsidian_token_expires_at', String(Date.now() + 60_000));

        const store = useAuthStore();
        await store.logout();

        // LIB-089: logout no longer passes a token — the server reads the
        // HttpOnly refresh cookie and clears it.
        expect(apiLogout).toHaveBeenCalledWith();
        expect(store.accessToken).toBeNull();
        expect(store.refreshToken).toBeNull();
        expect(store.pendingTotp).toBe(false);
        expect(store.profile).toBeNull();
        expect(store.isAuthenticated).toBe(false);
        expect(localStorage.getItem('obsidian_access_token')).toBeNull();
        expect(localStorage.getItem('obsidian_refresh_token')).toBeNull();
        expect(localStorage.getItem('obsidian_pending_totp')).toBeNull();
    });

    it('preserves pending TOTP state across refresh responses until verification completes', async () => {
        localStorage.setItem('obsidian_pending_totp', 'true');
        localStorage.setItem('obsidian_access_token', 'pending-access');
        localStorage.setItem('obsidian_refresh_token', 'pending-refresh');
        localStorage.setItem('obsidian_token_expires_at', '1');

        vi.mocked(apiRefreshToken).mockResolvedValueOnce({
            access_token: 'refreshed-access',
            refresh_token: 'refreshed-refresh',
            expires_in: 3600,
            totp_required: true,
        });

        const store = useAuthStore();
        await store.refresh();

        // LIB-089: refresh reads the HttpOnly cookie server-side; no token arg.
        expect(apiRefreshToken).toHaveBeenCalledWith();
        expect(store.pendingTotp).toBe(true);
        expect(store.isAuthenticated).toBe(false);
        expect(localStorage.getItem('obsidian_pending_totp')).toBe('true');
        expect(localStorage.getItem('obsidian_access_token')).toBe('refreshed-access');
    });

    it('coalesces concurrent stale-token refresh attempts', async () => {
        localStorage.setItem('obsidian_access_token', 'stale-access');
        localStorage.setItem('obsidian_refresh_token', 'refresh-token');
        localStorage.setItem('obsidian_token_expires_at', '1');

        vi.mocked(apiRefreshToken).mockResolvedValueOnce({
            access_token: 'fresh-access',
            refresh_token: 'fresh-refresh',
            expires_in: 3600,
            totp_required: false,
        });

        const store = useAuthStore();
        await Promise.all([store.ensureFresh(), store.ensureFresh(), store.ensureFresh()]);

        expect(apiRefreshToken).toHaveBeenCalledTimes(1);
        expect(store.accessToken).toBe('fresh-access');
    });

    // Regression: `isExpired` used to be a Vue `computed` reading a
    // non-reactive `Date.now()`. Vue would cache the first "not expired"
    // result after `expiresAt` was set and keep returning it for the rest
    // of the session — so `ensureFresh` never triggered a refresh, and
    // the token silently expired mid-session. The rewritten function must
    // re-read the wall clock on every call.
    it('ensureFresh triggers a refresh when time passes past expiresAt', async () => {
        const start = 1_000_000_000_000; // arbitrary fixed epoch ms
        vi.spyOn(Date, 'now').mockReturnValue(start);

        vi.mocked(apiLogin).mockResolvedValueOnce({
            access_token: 'a0',
            refresh_token: 'r0',
            expires_in: 3600, // 1h TTL — expires at start + 3_600_000
            totp_required: false,
        });
        vi.mocked(apiMe).mockResolvedValueOnce({ ...mockProfile });

        const store = useAuthStore();
        await store.login('alice', 'password');
        expect(apiRefreshToken).not.toHaveBeenCalled();

        // First call while the token is still comfortably fresh — no refresh.
        vi.spyOn(Date, 'now').mockReturnValue(start + 60_000);
        await store.ensureFresh();
        expect(apiRefreshToken).not.toHaveBeenCalled();

        // Advance the wall clock past `expiresAt - 60_000`. A Vue `computed`
        // would still be cached at "not expired" and NOT trigger refresh; a
        // plain function re-reads the clock every call and does. This is the
        // load-bearing assertion — it's the regression that caused sessions
        // to silently die between wake-from-sleep cycles.
        vi.spyOn(Date, 'now').mockReturnValue(start + 3_600_000);
        vi.mocked(apiRefreshToken).mockResolvedValueOnce({
            access_token: 'a1',
            refresh_token: 'r0',
            expires_in: 3600,
            totp_required: false,
        });
        await store.ensureFresh();

        expect(apiRefreshToken).toHaveBeenCalledTimes(1);
        expect(store.accessToken).toBe('a1');
    });

    describe('desktop (Tauri) refresh-token persistence', () => {
        beforeEach(() => {
            vi.mocked(isTauri).mockReturnValue(true);
        });
        afterEach(() => {
            vi.mocked(isTauri).mockReturnValue(false);
        });

        it('persists the refresh token to localStorage and sends it in the refresh body', async () => {
            vi.mocked(apiLogin).mockResolvedValueOnce({
                access_token: 'desktop-access',
                refresh_token: 'desktop-refresh',
                expires_in: 3600,
                totp_required: false,
            });
            vi.mocked(apiMe).mockResolvedValueOnce({ ...mockProfile });

            const store = useAuthStore();
            await store.login('alice', 'password');

            expect(localStorage.getItem('obsidian_refresh_token')).toBe('desktop-refresh');

            vi.mocked(apiRefreshToken).mockResolvedValueOnce({
                access_token: 'refreshed-access',
                refresh_token: 'desktop-refresh',
                expires_in: 3600,
                totp_required: false,
            });
            await store.refresh();

            // Desktop path passes the persisted token in the body so the request
            // doesn't depend on the (unreliable) WebView cookie.
            expect(apiRefreshToken).toHaveBeenCalledWith('desktop-refresh');
        });

        it('restores the refresh token from localStorage on store re-init (app restart)', async () => {
            localStorage.setItem('obsidian_access_token', 'stale-access');
            localStorage.setItem('obsidian_refresh_token', 'persisted-refresh');
            localStorage.setItem('obsidian_token_expires_at', '1');

            vi.mocked(apiRefreshToken).mockResolvedValueOnce({
                access_token: 'fresh-access',
                refresh_token: 'persisted-refresh',
                expires_in: 3600,
                totp_required: false,
            });

            const store = useAuthStore();
            expect(store.refreshToken).toBe('persisted-refresh');

            await store.ensureFresh();

            expect(apiRefreshToken).toHaveBeenCalledWith('persisted-refresh');
            expect(store.accessToken).toBe('fresh-access');
        });

        it('clears the persisted refresh token on logout and passes it to the server', async () => {
            localStorage.setItem('obsidian_access_token', 'a');
            localStorage.setItem('obsidian_refresh_token', 'to-revoke');

            const store = useAuthStore();
            await store.logout();

            expect(apiLogout).toHaveBeenCalledWith('to-revoke');
            expect(localStorage.getItem('obsidian_refresh_token')).toBeNull();
            expect(store.refreshToken).toBeNull();
        });

        // ── durable disk-backed fallback ─────────────────────────────────────
        //
        // Regression coverage for the "WebView UserData wipe" scenario: a full
        // reset of the WebView storage leaves localStorage empty, but a token
        // written to {data_dir}/session.json by a previous session must still
        // seed the store on the next boot so the user isn't kicked to /login.
        it('mirrors the refresh token to the disk store on login', async () => {
            vi.mocked(apiLogin).mockResolvedValueOnce({
                access_token: 'a',
                refresh_token: 'mirror-me',
                expires_in: 3600,
                totp_required: false,
            });
            vi.mocked(apiMe).mockResolvedValueOnce({ ...mockProfile });

            const store = useAuthStore();
            await store.login('alice', 'pw');
            // Give the fire-and-forget mirror a tick to reach the mock.
            await Promise.resolve();

            expect(authTokenSet).toHaveBeenCalledWith('mirror-me');
        });

        it('restores the refresh token from disk when localStorage is empty', async () => {
            vi.mocked(authTokenGet).mockResolvedValueOnce('from-disk');

            const store = useAuthStore();
            expect(store.refreshToken).toBeNull();

            await store.bootstrapPersistence();

            expect(authTokenGet).toHaveBeenCalledTimes(1);
            expect(store.refreshToken).toBe('from-disk');
            expect(localStorage.getItem('obsidian_refresh_token')).toBe('from-disk');
        });

        it('does not overwrite an existing localStorage refresh token from disk', async () => {
            localStorage.setItem('obsidian_refresh_token', 'from-local-storage');
            vi.mocked(authTokenGet).mockResolvedValueOnce('from-disk');

            const store = useAuthStore();
            await store.bootstrapPersistence();

            // localStorage was already the source of truth — disk is ignored.
            expect(authTokenGet).not.toHaveBeenCalled();
            expect(store.refreshToken).toBe('from-local-storage');
        });

        it('wipes the disk store on logout', async () => {
            localStorage.setItem('obsidian_access_token', 'a');
            localStorage.setItem('obsidian_refresh_token', 'x');

            const store = useAuthStore();
            await store.logout();
            await Promise.resolve();

            expect(authTokenClear).toHaveBeenCalledTimes(1);
        });
    });

    describe('bootstrapPersistence in browser mode', () => {
        it('is a no-op in a browser context (never calls the disk store)', async () => {
            const store = useAuthStore();
            await store.bootstrapPersistence();
            expect(authTokenGet).not.toHaveBeenCalled();
        });
    });

    // ── refresh() retry semantics ─────────────────────────────────────────────
    // Regression coverage for the overnight-logout bug: a transient loopback
    // failure during wake-from-sleep used to bubble out of refresh() and cause
    // callers (useWebSocket / App.vue mount / router guard) to invoke logout()
    // and wipe the session. refresh() now retries on non-401 errors and only
    // fails fast when the server actually said the session is invalid.
    describe('refresh() retry semantics', () => {
        beforeEach(() => {
            vi.useFakeTimers();
        });
        afterEach(() => {
            vi.useRealTimers();
        });

        it('retries transient (non-401) failures and eventually succeeds', async () => {
            localStorage.setItem('obsidian_access_token', 'stale');
            localStorage.setItem('obsidian_token_expires_at', '1');

            vi.mocked(apiRefreshToken)
                .mockRejectedValueOnce(new TypeError('Failed to fetch'))
                .mockRejectedValueOnce(new TypeError('Failed to fetch'))
                .mockResolvedValueOnce({
                    access_token: 'fresh-after-retry',
                    refresh_token: 'unused',
                    expires_in: 3600,
                    totp_required: false,
                });

            const store = useAuthStore();
            const refreshPromise = store.refresh();
            await vi.runAllTimersAsync();
            await refreshPromise;

            expect(apiRefreshToken).toHaveBeenCalledTimes(3);
            expect(store.accessToken).toBe('fresh-after-retry');
        });

        it('bails immediately on a 401 (real session-invalid) without retrying', async () => {
            localStorage.setItem('obsidian_access_token', 'stale');
            localStorage.setItem('obsidian_token_expires_at', '1');

            const err401 = new ApiError(401, 'Unauthorized');
            vi.mocked(apiRefreshToken).mockRejectedValueOnce(err401);

            const store = useAuthStore();
            await expect(store.refresh()).rejects.toBe(err401);
            expect(apiRefreshToken).toHaveBeenCalledTimes(1);
        });

        it('gives up after the retry budget and re-throws the transient error', async () => {
            localStorage.setItem('obsidian_access_token', 'stale');
            localStorage.setItem('obsidian_token_expires_at', '1');

            const netErr = new TypeError('Failed to fetch');
            vi.mocked(apiRefreshToken).mockRejectedValue(netErr);

            const store = useAuthStore();
            const refreshPromise = store.refresh();
            const assertion = expect(refreshPromise).rejects.toBe(netErr);
            await vi.runAllTimersAsync();
            await assertion;
            // 3 attempts total (initial + 2 retries).
            expect(apiRefreshToken).toHaveBeenCalledTimes(3);
        });
    });

    it('flagPendingTotp sets pendingTotp and persists to localStorage without clearing the token', () => {
        localStorage.setItem('obsidian_access_token', 'existing-token');
        localStorage.setItem('obsidian_pending_totp', 'false');

        const store = useAuthStore();
        store.flagPendingTotp();

        expect(store.pendingTotp).toBe(true);
        expect(store.isAuthenticated).toBe(false);
        // Access token is preserved so the TOTP verify call can still be made.
        expect(store.accessToken).toBe('existing-token');
        expect(localStorage.getItem('obsidian_pending_totp')).toBe('true');
    });

    it('loads the profile immediately for a non-TOTP login', async () => {
        vi.mocked(apiLogin).mockResolvedValueOnce({
            access_token: 'access-token',
            refresh_token: 'refresh-token',
            expires_in: 3600,
            totp_required: false,
        });
        vi.mocked(apiMe).mockResolvedValueOnce({ ...mockProfile });

        const store = useAuthStore();
        await store.login('alice', 'password');

        expect(store.pendingTotp).toBe(false);
        expect(store.isAuthenticated).toBe(true);
        expect(store.profile).toEqual(mockProfile);
        expect(apiMe).toHaveBeenCalledTimes(1);
    });

    describe('checkServerAuthEnabled', () => {
        it('returns true and caches it when the server reports auth enabled', async () => {
            vi.mocked(apiGetHealth).mockResolvedValueOnce({
                status: 'healthy',
                database: 'connected',
                auth_enabled: true,
            });

            const store = useAuthStore();
            expect(await store.checkServerAuthEnabled()).toBe(true);
            expect(await store.checkServerAuthEnabled()).toBe(true);
            // Cached after the first call — no second /api/health round trip.
            expect(apiGetHealth).toHaveBeenCalledTimes(1);
        });

        it('returns false when the server reports auth disabled', async () => {
            vi.mocked(apiGetHealth).mockResolvedValueOnce({
                status: 'healthy',
                database: 'connected',
                auth_enabled: false,
            });

            const store = useAuthStore();
            expect(await store.checkServerAuthEnabled()).toBe(false);
        });

        it('fails closed (assumes auth is enabled) when the health check errors', async () => {
            vi.mocked(apiGetHealth).mockRejectedValueOnce(new Error('network error'));

            const store = useAuthStore();
            expect(await store.checkServerAuthEnabled()).toBe(true);
        });

        it('de-duplicates concurrent calls into a single request', async () => {
            let resolveHealth!: (value: {
                status: string;
                database: string;
                auth_enabled: boolean;
            }) => void;
            vi.mocked(apiGetHealth).mockReturnValueOnce(
                new Promise((resolve) => {
                    resolveHealth = resolve;
                }),
            );

            const store = useAuthStore();
            const first = store.checkServerAuthEnabled();
            const second = store.checkServerAuthEnabled();
            resolveHealth({ status: 'healthy', database: 'connected', auth_enabled: false });

            expect(await first).toBe(false);
            expect(await second).toBe(false);
            expect(apiGetHealth).toHaveBeenCalledTimes(1);
        });
    });
    describe('clearLocalSession vs logout', () => {
        // A spurious 401 must not destroy the desktop's long-lived credential.
        // `logout` revokes the session server-side and wipes the durable
        // on-disk token; on desktop that token is a 10-year credential
        // (LIB-080), so an involuntary path must use `clearLocalSession`.

        async function signIn() {
            vi.mocked(apiLogin).mockResolvedValueOnce({
                access_token: 'access-1',
                refresh_token: 'refresh-1',
                expires_in: 3600,
                totp_required: false,
            });
            vi.mocked(apiMe).mockResolvedValueOnce(mockProfile);
            const store = useAuthStore();
            await store.login('alice', 'correct-horse-battery-staple');
            return store;
        }

        it('clearLocalSession wipes local state', async () => {
            const store = await signIn();
            expect(store.accessToken).toBe('access-1');

            store.clearLocalSession();

            expect(store.accessToken).toBeNull();
            expect(store.refreshToken).toBeNull();
            expect(store.expiresAt).toBe(0);
            expect(store.profile).toBeNull();
            expect(localStorage.getItem('obsidian_access_token')).toBeNull();
            expect(localStorage.getItem('obsidian_token_expires_at')).toBeNull();
        });

        it('clearLocalSession does NOT revoke the server session', async () => {
            const store = await signIn();

            store.clearLocalSession();

            expect(apiLogout).not.toHaveBeenCalled();
        });

        it('clearLocalSession does NOT clear the durable on-disk token', async () => {
            const store = await signIn();

            store.clearLocalSession();

            expect(authTokenClear).not.toHaveBeenCalled();
        });

        it('logout still revokes the server session and clears the durable token', async () => {
            const store = await signIn();

            await store.logout();

            expect(apiLogout).toHaveBeenCalled();
            expect(authTokenClear).toHaveBeenCalled();
            expect(store.accessToken).toBeNull();
            expect(store.profile).toBeNull();
        });
    });
});
