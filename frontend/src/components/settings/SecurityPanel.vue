<template>
  <div>
    <!-- Server deployments manage accounts centrally; this panel only governs
         the single local user of a desktop instance. -->
    <v-alert v-if="!isDesktop" type="info" variant="tonal" density="compact">
      Password protection for this server is managed by its administrator.
      Operators can run <code>librarium admin set-password &lt;username&gt;</code>
      on the machine hosting Librarium.
    </v-alert>

    <template v-else>
      <v-alert v-if="message" type="success" class="mb-4" density="compact" closable @click:close="message = ''">
        {{ message }}
      </v-alert>
      <v-alert v-if="error" type="error" class="mb-4" density="compact" closable @click:close="error = ''">
        {{ error }}
      </v-alert>

      <!-- ── Password protection is ON ─────────────────────────────────── -->
      <template v-if="authEnabled">
        <div class="text-subtitle-2 mb-2">Change your password</div>
        <v-text-field
          v-model="currentPassword"
          label="Current password"
          type="password"
          density="compact"
          data-testid="security-current-password"
        />
        <v-text-field
          v-model="newPassword"
          label="New password"
          type="password"
          density="compact"
          data-testid="security-new-password"
        />
        <v-text-field
          v-model="confirmPassword"
          label="Confirm new password"
          type="password"
          density="compact"
          data-testid="security-confirm-password"
        />
        <v-btn
          color="primary"
          size="small"
          :loading="busy"
          data-testid="security-change-btn"
          @click="changePassword"
        >
          Change password
        </v-btn>

        <v-divider class="my-6" />

        <div class="text-subtitle-2 mb-2">Turn off password protection</div>
        <p class="text-body-2 text-medium-emphasis mb-3">
          Librarium will stop asking for a password on this computer.
        </p>
        <v-btn
          color="error"
          variant="outlined"
          size="small"
          :loading="busy"
          data-testid="security-disable-btn"
          @click="disableAuth"
        >
          Turn off
        </v-btn>
      </template>

      <!-- ── Password protection is OFF ────────────────────────────────── -->
      <template v-else>
        <div class="text-subtitle-2 mb-2">Turn on password protection</div>
        <p class="text-body-2 text-medium-emphasis mb-3">
          Require a password when Librarium starts. Your vault files themselves
          are not encrypted — anyone with access to this computer's files can
          still read them.
        </p>
        <v-text-field
          v-model="enableUsername"
          label="Username"
          density="compact"
          data-testid="security-enable-username"
        />
        <v-text-field
          v-model="newPassword"
          label="Password"
          type="password"
          density="compact"
          data-testid="security-enable-password"
        />
        <v-text-field
          v-model="confirmPassword"
          label="Confirm password"
          type="password"
          density="compact"
          data-testid="security-enable-confirm"
        />
        <v-btn
          color="primary"
          size="small"
          :loading="busy"
          data-testid="security-enable-btn"
          @click="enableAuth"
        >
          Turn on
        </v-btn>
      </template>

      <v-alert type="info" variant="tonal" density="compact" class="mt-6">
        Turning password protection on or off takes effect the next time
        Librarium starts.
      </v-alert>

      <p class="text-caption text-medium-emphasis mt-4">
        Forgot your password? Choose “Forgot password?” on the sign-in screen,
        or run <code>librarium admin set-password &lt;username&gt;</code> on this
        computer.
      </p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useAuthStore } from '@/stores/auth';
import { isTauri, setLocalAuthEnabled } from '@/utils/tauri';

const authStore = useAuthStore();

const isDesktop = isTauri();
const authEnabled = ref(false);
const busy = ref(false);
const error = ref('');
const message = ref('');

const currentPassword = ref('');
const newPassword = ref('');
const confirmPassword = ref('');
const enableUsername = ref('admin');

onMounted(async () => {
  if (!isDesktop) return;
  try {
    authEnabled.value = await authStore.checkServerAuthEnabled();
  } catch {
    // Leave the panel in its "off" state; the actions below surface any real
    // failure with the server's own message.
  }
});

function reset() {
  currentPassword.value = '';
  newPassword.value = '';
  confirmPassword.value = '';
}

/** Shared guard for the two password-setting flows. */
function passwordsValid(): boolean {
  if (!newPassword.value) {
    error.value = 'Enter a new password.';
    return false;
  }
  if (newPassword.value !== confirmPassword.value) {
    error.value = 'Passwords do not match.';
    return false;
  }
  return true;
}

async function changePassword() {
  error.value = '';
  message.value = '';
  if (!currentPassword.value) {
    error.value = 'Enter your current password.';
    return;
  }
  if (!passwordsValid()) return;

  busy.value = true;
  try {
    await authStore.changePassword(currentPassword.value, newPassword.value);
    message.value = 'Password changed. Other signed-in sessions were signed out.';
    reset();
  } catch (e: any) {
    // Verbatim: the policy text from validate_password_policy is user-facing.
    error.value = e?.message ?? 'Could not change the password.';
  } finally {
    busy.value = false;
  }
}

async function enableAuth() {
  error.value = '';
  message.value = '';
  if (!enableUsername.value) {
    error.value = 'Enter a username.';
    return;
  }
  if (!passwordsValid()) return;

  busy.value = true;
  try {
    await setLocalAuthEnabled(true, enableUsername.value, newPassword.value);
    authEnabled.value = true;
    message.value = 'Password protection is on. It takes effect the next time Librarium starts.';
    reset();
  } catch (e: any) {
    error.value = e?.message ?? 'Could not turn on password protection.';
  } finally {
    busy.value = false;
  }
}

async function disableAuth() {
  error.value = '';
  message.value = '';
  busy.value = true;
  try {
    await setLocalAuthEnabled(false);
    authEnabled.value = false;
    message.value = 'Password protection is off. It takes effect the next time Librarium starts.';
    reset();
  } catch (e: any) {
    error.value = e?.message ?? 'Could not turn off password protection.';
  } finally {
    busy.value = false;
  }
}
</script>
