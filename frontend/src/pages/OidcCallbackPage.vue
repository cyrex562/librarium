<template>
  <v-container class="fill-height d-flex flex-column align-center justify-center">
    <v-progress-circular indeterminate color="primary" size="64" class="mb-4" />
    <div class="text-h6">Completing sign in...</div>
    <v-alert v-if="error" type="error" variant="tonal" class="mt-4" style="max-width: 400px;">
      {{ error }}
      <template #append>
        <v-btn variant="text" size="small" @click="goLogin">Back to login</v-btn>
      </template>
    </v-alert>
  </v-container>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useAuthStore } from '@/stores/auth';

const route = useRoute();
const router = useRouter();
const authStore = useAuthStore();
const error = ref('');

onMounted(async () => {
  const code = route.query.code as string;
  const state = route.query.state as string;
  const oidcError = route.query.error as string;

  if (oidcError) {
    error.value = `Provider error: ${oidcError}`;
    return;
  }

  if (!code || !state) {
    error.value = 'Missing authorization code or state from provider.';
    return;
  }

  try {
    await authStore.loginWithOidc(code, state);
    void router.push('/');
  } catch (e: any) {
    error.value = e?.message ?? 'Failed to complete OIDC login.';
  }
});

function goLogin() {
  void router.push('/login');
}
</script>
