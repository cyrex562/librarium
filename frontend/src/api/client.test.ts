import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { apiCreateUploadSession, apiUploadChunk } from './client';

function jsonResponse(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
    });
}

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
