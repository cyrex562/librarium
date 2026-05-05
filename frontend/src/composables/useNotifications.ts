import { isTauri } from '@/utils/tauri';

type NotificationChannel = 'reindex' | 'error' | 'conflict';

export interface AppNotification {
    title: string;
    body: string;
    channel: NotificationChannel;
}

// ── Tauri native notification ─────────────────────────────────────────────────

async function sendNativeNotification(title: string, body: string): Promise<void> {
    try {
        // Dynamically import so the module is only loaded inside Tauri.
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('notify', { title, body });
    } catch {
        // Silently ignore — notification is best-effort.
    }
}

// ── Browser Notification API ──────────────────────────────────────────────────

async function sendBrowserNotification(title: string, body: string): Promise<void> {
    if (!('Notification' in window) || !window.Notification) return;

    if (Notification.permission === 'default') {
        await Notification.requestPermission();
    }

    if (Notification.permission === 'granted') {
        new Notification(title, { body, icon: '/favicon.ico' });
    }
}

// ── Public composable ─────────────────────────────────────────────────────────

/**
 * Fire a desktop notification (Tauri) or a browser notification (web).
 *
 * In both contexts the call is best-effort: errors are swallowed so that a
 * missing permission or disabled plugin never crashes the app.
 */
export async function fireNotification(notification: AppNotification): Promise<void> {
    const { title, body } = notification;

    if (isTauri()) {
        await sendNativeNotification(title, body);
    } else {
        await sendBrowserNotification(title, body);
    }
}

/**
 * `useNotifications` — composable that wires WebSocket events to desktop or
 * browser notifications.
 *
 * Call `handleWsMessage(msg)` with any parsed `WsMessage` object; this
 * composable decides which events deserve a notification and fires them.
 *
 * @example
 * ```ts
 * import { useNotifications } from '@/composables/useNotifications';
 * const { handleWsMessage } = useNotifications();
 * // In your WS message handler:
 * handleWsMessage(parsedMsg);
 * ```
 */
export function useNotifications() {
    function handleWsMessage(msg: { type: string; [key: string]: unknown }): void {
        switch (msg.type) {
            case 'ReindexComplete': {
                const fileCount = (msg.file_count as number) ?? 0;
                const durationSec = (((msg.duration_ms as number) ?? 0) / 1000).toFixed(1);
                void fireNotification({
                    title: 'Reindex complete',
                    body: `${fileCount} file${fileCount !== 1 ? 's' : ''} indexed in ${durationSec}s`,
                    channel: 'reindex',
                });
                break;
            }
            case 'Error': {
                const message = (msg.message as string) ?? 'An unknown error occurred';
                void fireNotification({
                    title: 'Librarium error',
                    body: message,
                    channel: 'error',
                });
                break;
            }
            default:
                break;
        }
    }

    return { handleWsMessage, fireNotification };
}
