<template>
  <v-container class="fill-height d-flex align-center justify-center">
    <v-card min-width="360" max-width="420">
      <v-card-title class="text-center pa-6">
        <v-icon icon="mdi-notebook-outline" size="40" color="primary" />
        <div class="mt-2 text-h6">Codex</div>
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

      <v-card-actions class="px-4 pb-4">
        <v-btn block color="primary" :loading="loading" data-testid="login-submit-btn" @click="login">{{ authStore.pendingTotp ? 'Verify Code' : 'Sign In' }}</v-btn>
      </v-card-actions>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { useAuthStore } from '@/stores/auth';

const router = useRouter();
const authStore = useAuthStore();

const username = ref('');
const password = ref('');
const verificationCode = ref('');
const loading = ref(false);
const error = ref('');

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
