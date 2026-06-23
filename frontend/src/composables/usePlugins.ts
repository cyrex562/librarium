import { usePluginsStore } from '@/stores/plugins';
import { useVaultsStore } from '@/stores/vaults';
import { apiListPlugins, apiReadFile, apiWriteFile } from '@/api/client';

export function usePlugins() {
    const pluginsStore = usePluginsStore();
    const vaultsStore = useVaultsStore();

    async function loadPlugins() {
        try {
            const { plugins } = await apiListPlugins() as { plugins: any[] };
            
            for (const plugin of plugins) {
                if (plugin.enabled && !pluginsStore.activePlugins.has(plugin.manifest.id)) {
                    await loadPlugin(plugin);
                }
            }
        } catch (e) {
            console.error('Failed to load plugins:', e);
        }
    }

    async function loadPlugin(plugin: any) {
        const pluginId = plugin.manifest.id;
        const scriptUrl = `/api/plugins/${pluginId}/assets/main.js`;

        try {
            // Using dynamic import for ES modules
            // We append a timestamp to avoid caching during development
            const module = await import(`${scriptUrl}?t=${Date.now()}`);
            const PluginClass = module.default;
            
            if (!PluginClass) {
                console.error(`Plugin ${pluginId} does not have a default export`);
                return;
            }

            // Create a plugin API object
            const api = {
                addRibbonIcon: (icon: string, tooltip: string, callback: () => void) => {
                    pluginsStore.addRibbonIcon(pluginId, icon, tooltip, callback);
                },
                addStatusBarItem: (text: string, tooltip?: string, callback?: () => void) => {
                    pluginsStore.addStatusBarItem(pluginId, text, tooltip, callback);
                },
                storage_get: async (key: string) => {
                    const stored = localStorage.getItem(`plugin_storage_${pluginId}_${key}`);
                    return stored ? JSON.parse(stored) : null;
                },
                storage_set: async (key: string, value: any) => {
                    localStorage.setItem(`plugin_storage_${pluginId}_${key}`, JSON.stringify(value));
                },
                read_file: async (vault_id: string, path: string) => {
                    const res = await apiReadFile(vault_id, path);
                    return res.content;
                },
                write_file: async (vault_id: string, path: string, content: string) => {
                    return await apiWriteFile(vault_id, path, { content });
                },
                show_notice: (msg: string) => {
                    // TODO: Integrate with a notification system
                    console.log(`[Plugin Notice] ${msg}`);
                    alert(msg);
                },
                getContext: () => ({
                    vault_id: vaultsStore.activeVaultId,
                }),
                register_command: async (cmd: any) => {
                    console.log(`Plugin ${pluginId} registered command:`, cmd);
                },
            };

            const instance = new PluginClass(api);

            if (instance && typeof instance.onLoad === 'function') {
                await instance.onLoad({ vault_id: vaultsStore.activeVaultId });
            }

            pluginsStore.activePlugins.add(pluginId);
            console.log(`Loaded plugin: ${plugin.manifest.name}`);
        } catch (e) {
            console.error(`Failed to load plugin ${pluginId}:`, e);
        }
    }

    return {
        loadPlugins,
    };
}
