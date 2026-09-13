import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import {
    apiCreateUploadSession,
    apiListVaults,
    apiUploadChunk,
    ApiError,
    getTransport,
    httpTransport,
    localTransport,
    setTransport,
    type TransportResponseLike,
} from './client';

function jsonResponse(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
    });
}

// Never let one test's transport override leak into another's.
afterEach(() => {
    setTransport(httpTransport);
});

describe('api client auth refresh', () => {
    beforeEach(() => {
        localStorage.clear();
        setActivePinia(createPinia());
        vi.restoreAllMocks();
    });

    function seedExpiredTokens() {
        localStorage.setItem('obsidian_access_token', 'stale-access');
        localStorage.setItem('obsidian_refresh_token', 'refresh-token');
        localStorage.setItem('obsidian_token_expires_at', '1');
    }

    it('refreshes an expired token before creating an upload session', async () => {
        seedExpiredTokens();
        const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
            const url = String(input);
            if (url === '/api/auth/refresh') {
                return jsonResponse({
                    access_token: 'fresh-access',
                    refresh_token: 'fresh-refresh',
                    expires_in: 3600,
                });
            }
            if (url === '/api/vaults/v1/upload-sessions') {
                expect((init?.headers as Record<string, string>)?.Authorization).toBe('Bearer fresh-access');
                return jsonResponse({ session_id: 'session-1', uploaded_bytes: 0, total_size: 12 }, 201);
            }
            return jsonResponse({ error: 'unexpected request' }, 500);
        });
        vi.stubGlobal('fetch', fetchMock);

        await apiCreateUploadSession('v1', 'note.md', 12, 'Inbox');

        expect(fetchMock).toHaveBeenCalledTimes(2);
        expect(fetchMock.mock.calls[0][0]).toBe('/api/auth/refresh');
        expect(fetchMock.mock.calls[1][0]).toBe('/api/vaults/v1/upload-sessions');
    });

    it('refreshes an expired token before uploading a raw chunk', async () => {
        seedExpiredTokens();
        const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
            const url = String(input);
            if (url === '/api/auth/refresh') {
                return jsonResponse({
                    access_token: 'fresh-access',
                    refresh_token: 'fresh-refresh',
                    expires_in: 3600,
                });
            }
            if (url === '/api/vaults/v1/upload-sessions/session-1') {
                expect((init?.headers as Record<string, string>)?.Authorization).toBe('Bearer fresh-access');
                return jsonResponse({ uploaded_bytes: 4 });
            }
            return jsonResponse({ error: 'unexpected request' }, 500);
        });
        vi.stubGlobal('fetch', fetchMock);

        await apiUploadChunk('v1', 'session-1', new Blob(['test']));

        expect(fetchMock).toHaveBeenCalledTimes(2);
        expect(fetchMock.mock.calls[0][0]).toBe('/api/auth/refresh');
        expect(fetchMock.mock.calls[1][0]).toBe('/api/vaults/v1/upload-sessions/session-1');
    });
});

describe('pluggable transport (#55)', () => {
    beforeEach(() => {
        localStorage.clear();
        setActivePinia(createPinia());
        vi.restoreAllMocks();
    });

    function fakeTransportResponse(body: unknown, status = 200): TransportResponseLike {
        const text = JSON.stringify(body);
        return {
            ok: status >= 200 && status < 300,
            status,
            headers: {
                get: (name: string) =>
                    name.toLowerCase() === 'content-type' ? 'application/json' : null,
            },
            json: async () => JSON.parse(text),
            text: async () => text,
        };
    }

    it('produces an identical parsed result through httpTransport and a custom transport', async () => {
        const vaults = [{ id: 'v1', name: 'Vault' }];
        vi.stubGlobal('fetch', vi.fn(async () => jsonResponse(vaults)));
        const viaHttp = await apiListVaults();

        const customTransport = vi.fn(async () => fakeTransportResponse(vaults));
        setTransport(customTransport);
        const viaCustom = await apiListVaults();

        expect(viaCustom).toEqual(viaHttp);
        expect(customTransport).toHaveBeenCalledTimes(1);
    });

    it('produces an identical ApiError through httpTransport and a custom transport', async () => {
        vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ message: 'not found' }, 404)));
        const httpErr = (await apiListVaults().catch((e) => e)) as ApiError;
        expect(httpErr).toBeInstanceOf(ApiError);

        setTransport(vi.fn(async () => fakeTransportResponse({ message: 'not found' }, 404)));
        const customErr = (await apiListVaults().catch((e) => e)) as ApiError;

        expect(customErr).toBeInstanceOf(ApiError);
        expect(customErr.status).toBe(httpErr.status);
        expect(customErr.message).toBe(httpErr.message);
        expect(customErr.body).toEqual(httpErr.body);
    });

    it('getTransport reflects the active override', () => {
        const custom = async () => fakeTransportResponse({});
        setTransport(custom);
        expect(getTransport()).toBe(custom);
    });

    it('localTransport is selectable and dispatches instead of hitting fetch', async () => {
        // Route/response coverage for what localTransport actually dispatches
        // to lives in localDispatcher.test.ts (#56); this just confirms
        // setTransport(localTransport) really takes over from httpTransport.
        // Outside a Tauri context the underlying command wrapper rejects,
        // which is enough to prove fetch was never reached.
        const fetchMock = vi.fn();
        vi.stubGlobal('fetch', fetchMock);
        setTransport(localTransport);

        await expect(apiListVaults()).rejects.toThrow();
        expect(fetchMock).not.toHaveBeenCalled();
    });

    it('skips the token-refresh path entirely when a non-HTTP transport is active', async () => {
        localStorage.setItem('obsidian_access_token', 'stale-access');
        localStorage.setItem('obsidian_refresh_token', 'refresh-token');
        localStorage.setItem('obsidian_token_expires_at', '1'); // already expired
        const fetchMock = vi.fn(); // must never be reached
        vi.stubGlobal('fetch', fetchMock);
        setTransport(vi.fn(async () => fakeTransportResponse([])));

        await apiListVaults();

        expect(fetchMock).not.toHaveBeenCalled();
    });
});

describe('mobile transport detection (#63)', () => {
    // resolveDefaultTransport() runs once at module load, keyed off
    // VITE_LIBRARIUM_MOBILE (set by tauri.android.conf.json's
    // beforeBuildCommand for that build only — see that file and the
    // module doc comment). Re-import fresh per test so the module-load-time
    // check re-evaluates against the stubbed env var.
    afterEach(() => {
        vi.unstubAllEnvs();
        vi.resetModules();
    });

    it('defaults to the local transport when VITE_LIBRARIUM_MOBILE=true', async () => {
        vi.stubEnv('VITE_LIBRARIUM_MOBILE', 'true');
        vi.resetModules();

        const mod = await import('./client');

        expect(mod.isLocalTransportActive()).toBe(true);
    });

    it('defaults to the HTTP transport when VITE_LIBRARIUM_MOBILE is unset', async () => {
        vi.stubEnv('VITE_LIBRARIUM_MOBILE', undefined);
        vi.resetModules();

        const mod = await import('./client');

        expect(mod.isLocalTransportActive()).toBe(false);
    });
});

describe('401 handling: refresh and retry before giving up', () => {
    // A single 401 is NOT proof the session is over. `ensureFreshForRequest`
    // deliberately lets a request through when its refresh failed, so a
    // transient blip surfaces here as a 401 carrying a stale token. Treating
    // that as fatal — and calling the destructive logout — is what killed
    // long-lived desktop sessions after days or weeks of use.
    beforeEach(() => {
        localStorage.clear();
        setActivePinia(createPinia());
        vi.restoreAllMocks();
        localStorage.setItem('obsidian_access_token', 'stale-access');
        localStorage.setItem('obsidian_refresh_token', 'refresh-token');
        // Far future, so ensureFreshForRequest does not pre-emptively refresh
        // and the 401 path is what we actually exercise.
        localStorage.setItem('obsidian_token_expires_at', String(Date.now() + 3_600_000));
    });

    it('refreshes once, retries once, and does not clear the session on success', async () => {
        const { useAuthStore } = await import('@/stores/auth');
        const auth = useAuthStore();
        const refresh = vi.spyOn(auth, 'refresh').mockResolvedValue(undefined);
        const clearLocalSession = vi.spyOn(auth, 'clearLocalSession');
        const logout = vi.spyOn(auth, 'logout');

        const transport = vi
            .fn()
            .mockResolvedValueOnce(jsonResponse({ message: 'expired' }, 401))
            .mockResolvedValueOnce(jsonResponse([{ id: 'v1', name: 'vault' }]));
        setTransport(transport);

        const result = await apiListVaults();

        expect(transport).toHaveBeenCalledTimes(2);
        expect(refresh).toHaveBeenCalledTimes(1);
        expect(clearLocalSession).not.toHaveBeenCalled();
        expect(logout).not.toHaveBeenCalled();
        expect(result).toEqual([{ id: 'v1', name: 'vault' }]);
    });

    it('clears local session when the retry also 401s', async () => {
        const { useAuthStore } = await import('@/stores/auth');
        const auth = useAuthStore();
        vi.spyOn(auth, 'refresh').mockResolvedValue(undefined);
        const clearLocalSession = vi.spyOn(auth, 'clearLocalSession');
        const logout = vi.spyOn(auth, 'logout');

        setTransport(vi.fn(async () => jsonResponse({ message: 'expired' }, 401)));

        await expect(apiListVaults()).rejects.toBeInstanceOf(ApiError);

        expect(clearLocalSession).toHaveBeenCalled();
        // Never the destructive path: that would revoke the server session and
        // delete the durable 10-year refresh token.
        expect(logout).not.toHaveBeenCalled();
    });

    it('clears local session when the forced refresh itself fails', async () => {
        const { useAuthStore } = await import('@/stores/auth');
        const auth = useAuthStore();
        vi.spyOn(auth, 'refresh').mockRejectedValue(new Error('refresh failed'));
        const clearLocalSession = vi.spyOn(auth, 'clearLocalSession');
        const logout = vi.spyOn(auth, 'logout');

        setTransport(vi.fn(async () => jsonResponse({ message: 'expired' }, 401)));

        await expect(apiListVaults()).rejects.toBeInstanceOf(ApiError);

        expect(clearLocalSession).toHaveBeenCalled();
        expect(logout).not.toHaveBeenCalled();
    });
});
