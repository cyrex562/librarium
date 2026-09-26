<template>
  <div class="outline-panel">
    <div
      class="outline-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">OUTLINE</span>
    </div>
    <div v-if="expanded">
      <div v-if="headings.length" class="outline-list">
        <div
          v-for="(heading, i) in headings"
          :key="i"
          class="outline-item text-caption py-1 pr-2"
          :style="{ paddingLeft: `${(heading.level - 1) * 12 + 8}px` }"
          :title="heading.text"
          @click="jumpToHeading(heading.text)"
        >
          <span class="text-truncate d-block" style="max-width: 100%;">{{ heading.text }}</span>
        </div>
      </div>
      <div v-else class="pa-2 text-caption text-secondary text-center">
        No headings
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';

const props = defineProps<{ content: string }>();

const expanded = ref(false); // start collapsed

interface Heading { level: number; text: string }

const headings = computed((): Heading[] => {
  const results: Heading[] = [];
  const lines = props.content.split('\n');
  for (const line of lines) {
    const m = line.match(/^(#{1,6})\s+(.*)/);
    if (m) results.push({ level: m[1].length, text: m[2].trim() });
  }
  return results;
});

function jumpToHeading(text: string) {
  const preview = document.querySelector('.markdown-body');
  if (!preview) return;
  const headingEls = preview.querySelectorAll('h1,h2,h3,h4,h5,h6');
  for (const el of headingEls) {
    if (el.textContent?.trim() === text.trim()) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      return;
    }
  }
}
</script>

<style scoped>
.outline-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.outline-item {
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.outline-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-primary));
}
</style>
