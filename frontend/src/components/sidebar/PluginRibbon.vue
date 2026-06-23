<template>
  <div class="plugin-ribbon d-flex flex-column align-center py-2 ga-2">
    <v-tooltip
      v-for="item in pluginsStore.ribbonItems"
      :key="item.id"
      :text="item.tooltip"
      location="right"
    >
      <template #activator="{ props }">
        <v-btn
          v-bind="props"
          icon
          variant="text"
          size="small"
          class="ribbon-btn"
          @click="item.callback"
        >
          <v-icon :icon="`mdi-${item.icon}`" size="20" />
        </v-btn>
      </template>
    </v-tooltip>
  </div>
</template>

<script setup lang="ts">
import { usePluginsStore } from '@/stores/plugins';

const pluginsStore = usePluginsStore();
</script>

<style scoped>
.plugin-ribbon {
  width: 40px;
  height: 100%;
  border-right: 1px solid rgb(var(--v-theme-border));
  background: rgb(var(--v-theme-surface));
  flex-shrink: 0;
}
.ribbon-btn {
  opacity: 0.7;
}
.ribbon-btn:hover {
  opacity: 1;
  background: rgba(var(--v-theme-primary), 0.1);
}
</style>
