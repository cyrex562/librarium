<template>
  <div class="outgoing-links-panel">
    <div
      class="outgoing-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">OUTGOING LINKS</span>
      <span v-if="links.length" class="text-caption text-secondary ml-1">({{ links.length }})</span>
    </div>
    <div v-if="expanded">
      <div v-if="links.length" class="links-list">
        <div
          v-for="(link, i) in links"
          :key="i"
          class="link-item d-flex align-center px-2 py-1 text-caption"
          :title="link.target"
          @click="openLink(link)"
        >
          <v-icon
            :icon="link.isExternal ? 'mdi-open-in-new' : 'mdi-link-variant'"
            size="x-small"
            class="mr-1 flex-shrink-0"
            :color="link.isExternal ? 'secondary' : 'primary'"
          />
          <span class="text-truncate">{{ link.label || link.target }}</span>
        </div>
      </div>
      <div v-else class="pa-2 text-caption text-secondary text-center">
        No outgoing links
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { useTabsStore } from '@/stores/tabs';
import { useVaultsStore } from '@/stores/vaults';
import { apiResolveWikiLink } from '@/api/client';

const props = defineProps<{ content: string }>();

const expanded = ref(false); // start collapsed
const tabsStore = useTabsStore();
const vaultsStore = useVaultsStore();

interface OutgoingLink {
  label: string;
  target: string;
  isExternal: boolean;
  isWiki: boolean;
}

const links = computed((): OutgoingLink[] => {
  const seen = new Set<string>();
  const results: OutgoingLink[] = [];

  // Wiki links: [[target]] or [[target|label]]
  const wikiRe = /\[\[([^\]|#]+)(?:#[^\]|]*)?\|?([^\]]*)\]\]/g;
  let m: RegExpExecArray | null;
  while ((m = wikiRe.exec(props.content)) !== null) {
    const target = m[1].trim();
    const label = m[2].trim() || target;
    if (!seen.has(target)) {
      seen.add(target);
      results.push({ label, target, isExternal: false, isWiki: true });
    }
  }

  // Markdown links: [label](url) — skip images ![...]
  const mdRe = /(?<!!)\[([^\]]*)\]\(([^)]+)\)/g;
  while ((m = mdRe.exec(props.content)) !== null) {
    const label = m[1].trim();
    const target = m[2].trim().split(/\s+/)[0]; // strip title attribute
    const isExternal = /^https?:\/\//i.test(target);
    if (!seen.has(target)) {
      seen.add(target);
      results.push({ label, target, isExternal, isWiki: false });
    }
  }

  return results;
});

async function openLink(link: OutgoingLink) {
  if (link.isExternal) {
    window.open(link.target, '_blank', 'noopener,noreferrer');
    return;
  }

  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) return;

  if (link.isWiki) {
    try {
      const resolved = await apiResolveWikiLink(vaultId, link.target);
      if (resolved.exists) {
        tabsStore.openTab(tabsStore.activePaneId, resolved.path, resolved.path.split('/').pop() ?? resolved.path);
      }
    } catch {
      // no-op
    }
  } else {
    tabsStore.openTab(tabsStore.activePaneId, link.target, link.target.split('/').pop() ?? link.target);
  }
}
</script>

<style scoped>
.outgoing-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.link-item {
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.link-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-primary));
}
</style>
