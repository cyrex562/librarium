import { defineStore } from 'pinia';
import { ref } from 'vue';
import { apiGetPreferences, apiUpdatePreferences, apiResetPreferences } from '@/api/client';
import type { UserPreferences } from '@/api/types';

const DEFAULT_PREFS: UserPreferences = {
    theme: 'dark',
    editor_mode: 'formatted_raw',
    font_size: 14,
    icon_map: {},
    color_map: {},
};

/** Move a single path's entry and all descendant entries from one prefix to another. */
function remapMapPaths(map: Record<string, string>, fromPath: string, toPath: string) {
    const direct = map[fromPath];
    if (direct !== undefined) {
        map[toPath] = direct;
        delete map[fromPath];
    }
    const prefix = `${fromPath}/`;
    const newPrefix = `${toPath}/`;
    for (const key of Object.keys(map)) {
        if (!key.startsWith(prefix)) continue;
        map[`${newPrefix}${key.slice(prefix.length)}`] = map[key];
        delete map[key];
    }
}

/** Delete a path's entry and all descendant entries. */
function clearMapUnderPath(map: Record<string, string>, path: string) {
    delete map[path];
    const prefix = `${path}/`;
    for (const key of Object.keys(map)) {
        if (key.startsWith(prefix)) delete map[key];
    }
}

export const usePreferencesStore = defineStore('preferences', () => {
    const prefs = ref<UserPreferences>({ ...DEFAULT_PREFS });
    const loaded = ref(false);
    const saving = ref(false);

    async function load() {
        try {
            prefs.value = await apiGetPreferences();
            loaded.value = true;
        } catch {
            // Server not yet ready or no auth — use defaults
            prefs.value = { ...DEFAULT_PREFS };
            loaded.value = true;
        }
    }

    async function save() {
        saving.value = true;
        try {
            prefs.value = await apiUpdatePreferences(prefs.value);
        } finally {
            saving.value = false;
        }
    }

    async function reset() {
        prefs.value = await apiResetPreferences();
    }

    function set<K extends keyof UserPreferences>(key: K, value: UserPreferences[K]) {
        prefs.value[key] = value;
    }

    function getIcon(path: string): string | undefined {
        return prefs.value.icon_map?.[path];
    }

    function setIcon(path: string, icon: string) {
        if (!prefs.value.icon_map) prefs.value.icon_map = {};
        prefs.value.icon_map[path] = icon;
    }

    function clearIcon(path: string) {
        if (!prefs.value.icon_map) return;
        delete prefs.value.icon_map[path];
    }

    // Path lifecycle (delete/rename/move) applies to every per-path map so icons
    // and colors stay in sync. Callers invoke these at the same FS-mutation sites.
    function clearIconsUnderPath(path: string) {
        if (prefs.value.icon_map) clearMapUnderPath(prefs.value.icon_map, path);
        if (prefs.value.color_map) clearMapUnderPath(prefs.value.color_map, path);
    }

    function remapPathIcon(fromPath: string, toPath: string) {
        if (prefs.value.icon_map) remapMapPaths(prefs.value.icon_map, fromPath, toPath);
        if (prefs.value.color_map) remapMapPaths(prefs.value.color_map, fromPath, toPath);
    }

    function getColor(path: string): string | undefined {
        return prefs.value.color_map?.[path];
    }

    function setColor(path: string, color: string) {
        if (!prefs.value.color_map) prefs.value.color_map = {};
        prefs.value.color_map[path] = color;
    }

    function clearColor(path: string) {
        if (!prefs.value.color_map) return;
        delete prefs.value.color_map[path];
    }

    return {
        prefs,
        loaded,
        saving,
        load,
        save,
        reset,
        set,
        getIcon,
        setIcon,
        clearIcon,
        clearIconsUnderPath,
        remapPathIcon,
        getColor,
        setColor,
        clearColor,
    };
});
