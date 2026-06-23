import { defineStore } from 'pinia';
import { ref } from 'vue';

export interface RibbonItem {
    id: string;
    icon: string;
    tooltip: string;
    callback: () => void | Promise<void>;
}

export interface StatusBarItem {
    id: string;
    text: string;
    tooltip?: string;
    callback?: () => void | Promise<void>;
}

export const usePluginsStore = defineStore('plugins', () => {
    const activePlugins = ref<Set<string>>(new Set());
    const ribbonItems = ref<RibbonItem[]>([]);
    const statusBarItems = ref<StatusBarItem[]>([]);

    function addRibbonIcon(pluginId: string, icon: string, tooltip: string, callback: () => void) {
        ribbonItems.value.push({
            id: `${pluginId}-${icon}`,
            icon,
            tooltip,
            callback,
        });
    }

    function addStatusBarItem(pluginId: string, text: string, tooltip?: string, callback?: () => void) {
        statusBarItems.value.push({
            id: `${pluginId}-${statusBarItems.value.length}`,
            text,
            tooltip,
            callback,
        });
    }

    function clearPluginUi(pluginId: string) {
        ribbonItems.value = ribbonItems.value.filter(item => !item.id.startsWith(`${pluginId}-`));
        statusBarItems.value = statusBarItems.value.filter(item => !item.id.startsWith(`${pluginId}-`));
    }

    return {
        activePlugins,
        ribbonItems,
        statusBarItems,
        addRibbonIcon,
        addStatusBarItem,
        clearPluginUi,
    };
});
