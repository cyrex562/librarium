import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { EditorMode } from '@/api/types';

export const useEditorStore = defineStore('editor', () => {
    const mode = ref<EditorMode>('formatted_raw');
    // Per-tab pending auto-save timers (tabId → timer handle)
    const autoSaveTimers = ref<Map<string, ReturnType<typeof setTimeout>>>(new Map());
    // The save each pending timer will run, so it can be run early.
    const autoSaveCallbacks = new Map<string, () => void | Promise<void>>();

    function setMode(newMode: EditorMode) {
        mode.value = newMode;
    }

    function scheduleAutoSave(tabId: string, delayMs: number, callback: () => void | Promise<void>) {
        const existing = autoSaveTimers.value.get(tabId);
        if (existing !== undefined) clearTimeout(existing);
        const handle = setTimeout(() => {
            autoSaveTimers.value.delete(tabId);
            autoSaveCallbacks.delete(tabId);
            void callback();
        }, delayMs);
        autoSaveTimers.value.set(tabId, handle);
        autoSaveCallbacks.set(tabId, callback);
    }

    /** Run a tab's pending auto-save now (e.g. before reading the file back). */
    async function flushAutoSave(tabId: string) {
        const callback = autoSaveCallbacks.get(tabId);
        if (!callback) return;
        cancelAutoSave(tabId);
        await callback();
    }

    function cancelAutoSave(tabId: string) {
        const handle = autoSaveTimers.value.get(tabId);
        if (handle !== undefined) {
            clearTimeout(handle);
            autoSaveTimers.value.delete(tabId);
        }
        autoSaveCallbacks.delete(tabId);
    }

    return { mode, setMode, scheduleAutoSave, cancelAutoSave, flushAutoSave };
});
