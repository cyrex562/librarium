<template>
  <v-container class="fill-height d-flex align-center justify-center">
    <!-- width 100% + max-width (not min-width) so the card shrinks on phones -->
    <v-card width="100%" max-width="420">
      <v-card-title class="text-center pa-6">
        <v-icon icon="mdi-notebook-outline" size="40" color="primary" />
        <div class="mt-2 text-h6">Librarium</div>
      </v-card-title>

      <v-card-text>
        <v-alert v-if="error" type="error" class="mb-4" closable data-testid="login-error-alert" @click:close="error = ''">{{ error }}</v-alert>

        <v-text-field
          v-if="!authStore.pendingTotp"
          v-model="username"
          label="Username"
          prepend-inner-icon="mdi-account-outline"
          autofocus
          data-testid="login-username-input"
          @keyup.enter="login"
        />
        <v-text-field
          v-if="!authStore.pendingTotp"
          v-model="password"
          label="Password"
          type="password"
          prepend-inner-icon="mdi-lock-outline"
          data-testid="login-password-input"
          @keyup.enter="login"
        />
        <v-text-field
          v-if="authStore.pendingTotp"
          v-model="verificationCode"
          label="Verification Code"
          prepend-inner-icon="mdi-shield-key-outline"
          data-testid="login-totp-input"
          @keyup.enter="login"
        />
      </v-card-text>

      <v-card-actions class="px-4 pb-4 d-flex flex-column ga-2">
        <v-btn block color="primary" :loading="loading" data-testid="login-submit-btn" @click="login">{{ authStore.pendingTotp ? 'Verify Code' : 'Sign In' }}</v-btn>
        <!-- Desktop only: the reset command exists solely in the Tauri shell. -->
        <v-btn
          v-if="canResetLocally && !authStore.pendingTotp"
          variant="text"
          size="small"
          data-testid="login-forgot-btn"
          @click="openReset"
        >
          Forgot password?
        </v-btn>
      </v-card-actions>
    </v-card>

    <v-dialog v-model="resetOpen" max-width="440" persistent>
      <v-card>
        <v-card-title class="text-h6 pa-4">Reset your password</v-card-title>
        <v-card-text>
          <p class="text-body-2 text-medium-emphasis mb-4">
            This resets the password for this copy of Librarium on this
            computer. Anyone who can use this machine can already open your
            vault files directly, so no old password is needed.
          </p>
          <v-alert v-if="resetError" type="error" class="mb-4" density="compact">{{ resetError }}</v-alert>
          <v-text-field v-model="resetUsername" label="Username" density="compact" data-testid="reset-username-input" />
          <v-text-field
            v-model="resetPassword"
            label="New password"
            type="password"
            density="compact"
            data-testid="reset-password-input"
          />
          <v-text-field
            v-model="resetConfirm"
            label="Confirm new password"
            type="password"
            density="compact"
            data-testid="reset-confirm-input"
            @keyup.enter="submitReset"
          />
        </v-card-text>
        <v-card-actions class="px-4 pb-4">
          <v-spacer />
          <v-btn variant="text" :disabled="resetLoading" @click="resetOpen = false">Cancel</v-btn>
          <v-btn color="primary" :loading="resetLoading" data-testid="reset-submit-btn" @click="submitReset">
            Reset and sign in
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-container>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { useAuthStore } from '@/stores/auth';
import { isTauri, resetLocalPassword } from '@/utils/tauri';

const router = useRouter();
const authStore = useAuthStore();

const username = ref('');
const password = ref('');
const verificationCode = ref('');
const loading = ref(false);
const error = ref('');

// Local password reset — desktop shell only. The browser build never renders
// the affordance, and the underlying Tauri command does not exist there.
const canResetLocally = isTauri();
const resetOpen = ref(false);
const resetUsername = ref('admin');
const resetPassword = ref('');
const resetConfirm = ref('');
const resetLoading = ref(false);
const resetError = ref('');

function openReset() {
  resetError.value = '';
  resetPassword.value = '';
  resetConfirm.value = '';
  resetUsername.value = username.value || 'admin';
  resetOpen.value = true;
}

async function submitReset() {
  if (!resetUsername.value || !resetPassword.value) {
    resetError.value = 'Enter a username and a new password.';
    return;
  }
  if (resetPassword.value !== resetConfirm.value) {
    resetError.value = 'Passwords do not match.';
    return;
  }
  resetLoading.value = true;
  resetError.value = '';
  try {
    await resetLocalPassword(resetUsername.value, resetPassword.value);
    resetOpen.value = false;
    // Sign straight in so the user lands where they were headed.
    username.value = resetUsername.value;
    password.value = resetPassword.value;
    await login();
  } catch (e: any) {
    // Show the server's message verbatim — the password-policy text from
    // validate_password_policy is already user-facing.
    resetError.value = e?.message ?? 'Could not reset the password.';
  } finally {
    resetLoading.value = false;
  }
}

async function login() {
  if (!authStore.pendingTotp && (!username.value || !password.value)) return;
  if (authStore.pendingTotp && !verificationCode.value) return;
  loading.value = true;
  error.value = '';
  try {
    if (authStore.pendingTotp) {
      await authStore.completeTotpLogin(verificationCode.value);
    } else {
      await authStore.login(username.value, password.value);
    }
    if (authStore.pendingTotp) return;
    const redirect = typeof router.currentRoute.value.query.redirect === 'string'
      ? router.currentRoute.value.query.redirect
      : '/';
    router.push(redirect);
  } catch (e: any) {
    error.value = e?.message ?? 'Login failed.';
  } finally {
    loading.value = false;
  }
}
</script>
