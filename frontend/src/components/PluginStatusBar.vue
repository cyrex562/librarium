<template>
  <div class="plugin-status-bar d-flex align-center px-2 ga-4 text-caption text-secondary">
    <div
      v-for="item in pluginsStore.statusBarItems"
      :key="item.id"
      class="status-bar-item"
      :title="item.tooltip"
      @click="item.callback?.()"
    >
      {{ item.text }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { usePluginsStore } from '@/stores/plugins';

const pluginsStore = usePluginsStore();
</script>

<style scoped>
.plugin-status-bar {
  height: 22px;
  border-top: 1px solid rgb(var(--v-theme-border));
  background: rgb(var(--v-theme-surface));
  flex-shrink: 0;
}
.status-bar-item {
  cursor: default;
  white-space: nowrap;
}
.status-bar-item[onClick] {
  cursor: pointer;
}
.status-bar-item[onClick]:hover {
  color: rgb(var(--v-theme-primary));
}
</style>
