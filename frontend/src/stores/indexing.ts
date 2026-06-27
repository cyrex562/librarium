import { defineStore } from 'pinia';
import { ref, computed } from 'vue';

/**
 * Tracks background indexing activity per vault, driven by `IndexingStatus`
 * WebSocket messages. Each `active: true` increments a per-vault counter and
 * each `active: false` decrements it, so overlapping indexing operations
 * (startup index + entity reindex + watcher batches) are handled correctly and
 * the indicator only clears once they have all finished.
 */
export const useIndexingStore = defineStore('indexing', () => {
    const counts = ref<Map<string, number>>(new Map());

    function setActive(vaultId: string, active: boolean) {
        const next = new Map(counts.value);
        const current = next.get(vaultId) ?? 0;
        const updated = Math.max(0, current + (active ? 1 : -1));
        if (updated === 0) {
            next.delete(vaultId);
        } else {
            next.set(vaultId, updated);
        }
        counts.value = next;
    }

    function isIndexing(vaultId: string | null | undefined): boolean {
        if (!vaultId) return false;
        return (counts.value.get(vaultId) ?? 0) > 0;
    }

    const anyIndexing = computed(() => counts.value.size > 0);

    return { counts, setActive, isIndexing, anyIndexing };
});
