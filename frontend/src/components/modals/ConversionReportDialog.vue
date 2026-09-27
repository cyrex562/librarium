<template>
  <v-dialog :model-value="!!report" max-width="560" @update:model-value="(open) => { if (!open) close(); }">
    <v-card v-if="report" data-testid="conversion-report">
      <v-card-title>Converted to Typst</v-card-title>
      <v-card-text>
        <p class="mb-3">
          Created <strong>{{ report.target }}</strong> from {{ report.source }}.
          {{ report.warnings.length === 1 ? 'One thing' : `${report.warnings.length} things` }}
          couldn't be carried over exactly:
        </p>
        <v-list density="compact" class="conversion-warnings">
          <v-list-item v-for="(w, i) in report.warnings" :key="i" prepend-icon="mdi-alert-outline">
            <v-list-item-title class="text-wrap">
              <span v-if="w.line" class="font-weight-medium">Line {{ w.line }}: </span>{{ w.message }}
            </v-list-item-title>
          </v-list-item>
        </v-list>
        <p class="mt-3 text-caption text-secondary">The Markdown note is unchanged.</p>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn color="primary" @click="close">OK</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useUiStore } from '@/stores/ui';

const uiStore = useUiStore();
const report = computed(() => uiStore.conversionReport);

function close() {
  uiStore.conversionReport = null;
}
</script>

<style scoped>
.conversion-warnings {
  max-height: 320px;
  overflow-y: auto;
}
</style>
