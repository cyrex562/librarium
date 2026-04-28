import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { apiLogin, apiRefreshToken, apiLogout, apiMe, apiChangePassword, apiVerifyTotpLogin } from '@/api/client';
import type { LoginResponse, AuthenticatedUserProfile } from '@/api/types';

const ACCESS_TOKEN_KEY = 'obsidian_access_token';
const REFRESH_TOKEN_KEY = 'obsidian_refresh_token';
const EXPIRES_AT_KEY = 'obsidian_token_expires_at';
const PENDING_TOTP_KEY = 'obsidian_pending_totp';

export const useAuthStore = defineStore('auth', () => {
    const accessToken = ref<string | null>(localStorage.getItem(ACCESS_TOKEN_KEY));
    const refreshToken = ref<string | null>(localStorage.getItem(REFRESH_TOKEN_KEY));
    const expiresAt = ref<number>(parseInt(localStorage.getItem(EXPIRES_AT_KEY) ?? '0', 10));
    const pendingTotp = ref(localStorage.getItem(PENDING_TOTP_KEY) === 'true');
    const profile = ref<AuthenticatedUserProfile | null>(null);
    const loadingProfile = ref(false);

    const isAuthenticated = computed(() => !!accessToken.value && !pendingTotp.value);
    const isExpired = computed(() => Date.now() > expiresAt.value - 60_000); // 60s margin
    const isAdmin = computed(() => !!profile.value?.is_admin);
    const mustChangePassword = computed(() => !!profile.value?.must_change_password);

    function _applyTokens(resp: LoginResponse) {
        accessToken.value = resp.access_token;
        refreshToken.value = resp.refresh_token;
        expiresAt.value = Date.now() + resp.expires_in * 1000;
        pendingTotp.value = !!resp.totp_required;
        localStorage.setItem(ACCESS_TOKEN_KEY, resp.access_token);
        localStorage.setItem(REFRESH_TOKEN_KEY, resp.refresh_token);
        localStorage.setItem(EXPIRES_AT_KEY, String(expiresAt.value));
        localStorage.setItem(PENDING_TOTP_KEY, String(pendingTotp.value));
    }

    async function login(username: string, password: string) {
        const resp = await apiLogin(username, password);
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
        if (!refreshToken.value) throw new Error('No refresh token');
        const resp = await apiRefreshToken(refreshToken.value);
        _applyTokens(resp);
    }

    async function logout() {
        try { await apiLogout(refreshToken.value); } catch { /* ignore server errors on logout */ }
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
        if (accessToken.value && isExpired.value) {
            await refresh();
        }
    }

    async function changePassword(currentPassword: string, newPassword: string) {
        await apiChangePassword({
            current_password: currentPassword,
            new_password: newPassword,
        });
        await loadProfile(true);
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
        completeTotpLogin,
        refresh,
        logout,
        ensureFresh,
        loadProfile,
        changePassword,
    };
});
