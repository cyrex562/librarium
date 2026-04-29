import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';

vi.mock('@/api/client', () => ({
    apiLogin: vi.fn(),
    apiRefreshToken: vi.fn(),
    apiLogout: vi.fn(),
    apiMe: vi.fn(),
    apiChangePassword: vi.fn(),
    apiVerifyTotpLogin: vi.fn(),
}));

import {
    apiLogin,
    apiLogout,
    apiMe,
    apiRefreshToken,
    apiChangePassword,
    apiVerifyTotpLogin,
} from '@/api/client';
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
        expect(localStorage.getItem('obsidian_refresh_token')).toBe('pending-refresh');
        expect(apiMe).not.toHaveBeenCalled();
    });

    it('completes TOTP login, clears pending state, and loads the authenticated profile', async () => {
        localStorage.setItem('obsidian_pending_totp', 'true');
        localStorage.setItem('obsidian_access_token', 'pending-access');
        localStorage.setItem('obsidian_refresh_token', 'pending-refresh');
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
        expect(localStorage.getItem('obsidian_refresh_token')).toBe('verified-refresh');
    });

    it('passes the refresh token to logout and clears pending TOTP auth state', async () => {
        localStorage.setItem('obsidian_pending_totp', 'true');
        localStorage.setItem('obsidian_access_token', 'pending-access');
        localStorage.setItem('obsidian_refresh_token', 'pending-refresh');
        localStorage.setItem('obsidian_token_expires_at', String(Date.now() + 60_000));

        const store = useAuthStore();
        await store.logout();

        expect(apiLogout).toHaveBeenCalledWith('pending-refresh');
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

        expect(apiRefreshToken).toHaveBeenCalledWith('pending-refresh');
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
});
