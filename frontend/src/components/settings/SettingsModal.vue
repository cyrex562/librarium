<template>
  <v-dialog
    :model-value="modelValue"
    max-width="720"
    :fullscreen="isMobile"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        Settings
        <v-spacer />
        <v-btn icon="mdi-close" size="small" variant="plain" data-testid="settings-modal-close-btn" @click="close" />
      </v-card-title>

      <v-tabs v-model="activeTab">
        <v-tab value="api-keys">API Keys</v-tab>
        <v-tab v-if="isTauri()" value="sync">Sync</v-tab>
        <v-tab v-if="isLocalMode" value="offline-sync">Offline Sync</v-tab>
        <v-tab value="security">Security</v-tab>
        <v-tab value="about">About</v-tab>
      </v-tabs>

      <v-divider />

      <v-card-text :style="isMobile ? 'overflow-y: auto;' : 'max-height: 600px; overflow-y: auto;'">
        <v-tabs-window v-model="activeTab">
          <v-tabs-window-item value="api-keys">
            <ApiKeysPanel />
          </v-tabs-window-item>
          <v-tabs-window-item v-if="isTauri()" value="sync">
            <SyncSettingsPanel />
          </v-tabs-window-item>
          <v-tabs-window-item v-if="isLocalMode" value="offline-sync">
            <OfflineSyncPanel @close="close" />
          </v-tabs-window-item>
          <v-tabs-window-item value="security">
            <SecurityPanel />
          </v-tabs-window-item>
          <v-tabs-window-item value="about">
            <AboutPanel />
          </v-tabs-window-item>
        </v-tabs-window>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn @click="close">Close</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import ApiKeysPanel from '@/components/settings/ApiKeysPanel.vue';
import SyncSettingsPanel from '@/components/settings/sync/SyncSettingsPanel.vue';
import OfflineSyncPanel from '@/components/settings/sync/OfflineSyncPanel.vue';
import SecurityPanel from '@/components/settings/SecurityPanel.vue';
import AboutPanel from '@/components/settings/AboutPanel.vue';
import { isTauri } from '@/utils/tauri';
import { useMobile } from '@/composables/useMobile';
import { useCapabilities } from '@/composables/useCapabilities';

const { isMobile } = useMobile();
const { isLocalMode } = useCapabilities();

const props = defineProps<{ modelValue: boolean; initialTab?: string }>();
const emit = defineEmits<{ 'update:modelValue': [v: boolean] }>();

const activeTab = ref(props.initialTab ?? 'api-keys');

watch(
  () => props.modelValue,
  (open) => {
    if (open && props.initialTab) {
      activeTab.value = props.initialTab;
    }
  },
);

function close() {
  emit('update:modelValue', false);
}
</script>
