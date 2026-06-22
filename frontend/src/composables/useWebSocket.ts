import { ref, onUnmounted } from 'vue';
import type { WsMessage } from '@/api/types';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useVaultsStore } from '@/stores/vaults';
import { useAuthStore } from '@/stores/auth';
import { useNotifications } from '@/composables/useNotifications';

const WS_BASE_URL = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/ws`;
const MAX_RECONNECT_DELAY_MS = 30_000;

// Module-level singleton so only one WS connection exists regardless of how many
// components call useWebSocket().
let ws: WebSocket | null = null;
let reconnectAttempts = 0;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

const connected = ref(false);

function getReconnectDelay(): number {
    return Math.min(1_000 * Math.pow(2, reconnectAttempts), MAX_RECONNECT_DELAY_MS);
}

function handleMessage(event: MessageEvent) {
    let msg: WsMessage;
    try {
        msg = JSON.parse(event.data as string) as WsMessage;
    } catch {
        return;
    }

    // Forward all events to the notification composable (best-effort).
    const { handleWsMessage } = useNotifications();
    handleWsMessage(msg as { type: string; [key: string]: unknown });

    // IMPORTANT: this switch must handle every variant of WsMessage.
    // The exhaustiveness check at the bottom of the switch (the `never`
    // assignment) will cause a TypeScript compile error if a new variant is
    // added to the WsMessage union without a corresponding case here, keeping
    // frontend handling centralised in one place.
    switch (msg.type) {
        case 'FileChanged': {
            const filesStore = useFilesStore();
            const tabsStore = useTabsStore();
            const vaultsStore = useVaultsStore();

            if (typeof msg.event_type === 'object' && 'renamed' in msg.event_type) {
                tabsStore.remapTabPaths(msg.event_type.renamed.from, msg.event_type.renamed.to);
            }

            // Refresh the file tree for the affected vault.
            if (vaultsStore.activeVaultId === msg.vault_id) {
                void filesStore.loadTree(msg.vault_id);
            }

            // If the changed file is open in a tab and not dirty, reload it.
            tabsStore.tabs.forEach((tab, tabId) => {
                if (tab.filePath === msg.path && !tab.isDirty) {
                    void filesStore.readFile(msg.vault_id, msg.path).then((fc) => {
                        const t = tabsStore.tabs.get(tabId);
                        if (t && !t.isDirty) {
                            t.content = fc.content;
                            t.modified = fc.modified;
                            t.frontmatter = fc.frontmatter;
                        }
                    });
                }
            });
            break;
        }
        case 'ReindexComplete':
            // Notification already fired via handleWsMessage above;
            // no additional UI state update needed here.
            break;
        case 'SyncPing':
        case 'SyncPong':
            // Reserved for desktop sync heartbeat handling.
            break;
        case 'Error':
            console.warn('WebSocket error from server:', msg.message);
            break;
        default: {
            // Exhaustiveness check: if WsMessage gains a new variant and it is
            // not handled above, TypeScript will mark `msg` as `never` here and
            // the assignment will fail to compile.
            const _exhaustive: never = msg;
            console.warn('Unhandled WebSocket message type:', (_exhaustive as WsMessage).type);
        }
    }
}

async function connect() {
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
        return;
    }

    const authStore = useAuthStore();
    if (!authStore.isAuthenticated) {
        connected.value = false;
        return;
    }

    try {
        await authStore.ensureFresh();
    } catch {
        await authStore.logout();
        connected.value = false;
        return;
    }

    if (!authStore.accessToken) {
        connected.value = false;
        return;
    }

    const wsUrl = new URL(WS_BASE_URL);
    wsUrl.searchParams.set('access_token', authStore.accessToken);
    ws = new WebSocket(wsUrl.toString());

    ws.addEventListener('open', () => {
        connected.value = true;
        reconnectAttempts = 0;
        if (reconnectTimer !== null) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
    });

    ws.addEventListener('message', handleMessage);

    ws.addEventListener('close', (event) => {
        connected.value = false;
        ws = null;
        const authStore = useAuthStore();
        if (!authStore.isAuthenticated) {
            return;
        }

        // Avoid reconnect storms when server rejected auth/token.
        if (event.code === 1008) {
            return;
        }

        if (reconnectTimer !== null) {
            return;
        }

        const delay = getReconnectDelay();
        reconnectAttempts++;
        reconnectTimer = setTimeout(() => {
            reconnectTimer = null;
            void connect();
        }, delay);
    });

    ws.addEventListener('error', () => {
        ws?.close();
    });
}

function disconnect() {
    if (reconnectTimer !== null) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
    }
    ws?.close();
    ws = null;
    connected.value = false;
}

export function useWebSocket(autoConnect = true) {
    if (autoConnect) {
        void connect();
    }

    // Cleanup when the last consumer unmounts is not straightforward with a singleton,
    // so we intentionally keep the connection alive for the app lifetime.
    // disconnect() can be called explicitly (e.g. on logout).

    return { connected, disconnect, connect };
}
