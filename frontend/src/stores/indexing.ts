import { defineStore } from 'pinia';
import { ref, computed } from 'vue';

/**
 * Tracks background indexing activity per vault, driven by `IndexingStatus`
 * WebSocket messages. Each `active: true` increments a per-vault counter and
 * each `active: false` decrements it, so overlapping indexing operations
 * (startup index + entity reindex + watcher batches) are handled correctly and
 * the indicator only clears once they have all finished.
 */
// Safety net: if a vault stays "indexing" this long without clearing — e.g. a
// dropped `active:false` WS message — force-clear it so the indicator can't
// stick on indefinitely (LIB-094).
const STUCK_CLEAR_MS = 45_000;

export const useIndexingStore = defineStore('indexing', () => {
    const counts = ref<Map<string, number>>(new Map());
    // Per-vault watchdog timers (not reactive; purely a cleanup mechanism).
    const timers = new Map<string, ReturnType<typeof setTimeout>>();

    function clearTimer(vaultId: string) {
        const t = timers.get(vaultId);
        if (t) {
            clearTimeout(t);
            timers.delete(vaultId);
        }
    }

    function forceClear(vaultId: string) {
        clearTimer(vaultId);
        if (counts.value.has(vaultId)) {
            const next = new Map(counts.value);
            next.delete(vaultId);
            counts.value = next;
        }
    }

    function setActive(vaultId: string, active: boolean) {
        const next = new Map(counts.value);
        const current = next.get(vaultId) ?? 0;
        const updated = Math.max(0, current + (active ? 1 : -1));
        if (updated === 0) {
            next.delete(vaultId);
            clearTimer(vaultId);
        } else {
            next.set(vaultId, updated);
            // (Re)arm the stuck-clear watchdog on each level change.
            clearTimer(vaultId);
            timers.set(vaultId, setTimeout(() => forceClear(vaultId), STUCK_CLEAR_MS));
        }
        counts.value = next;
    }

    function isIndexing(vaultId: string | null | undefined): boolean {
        if (!vaultId) return false;
        return (counts.value.get(vaultId) ?? 0) > 0;
    }

    // Drop all indexing state — used on WebSocket reconnect, where missed
    // start/stop events would otherwise leave the indicator out of sync (LIB-095).
    function reset() {
        for (const id of [...timers.keys()]) clearTimer(id);
        if (counts.value.size > 0) counts.value = new Map();
    }

    const anyIndexing = computed(() => counts.value.size > 0);

    return { counts, setActive, isIndexing, reset, anyIndexing };
});
