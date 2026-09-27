import { isLocalTransportActive } from '@/api/client';

/**
 * The capability set gates UI *visibility* (not just availability) for
 * features that only work against a real librarium-server: admin panel,
 * groups/vault-sharing, plugin manager, ML/organize panels, entity graph and
 * relations, reindex, and archive import/export. None of these are
 * implemented by the local (Tauri command) transport (`localDispatcher.ts`,
 * #56/#57) — rather than let each one fail at runtime in a way a user reads
 * as a bug, this converts "not implemented" into "deliberately not offered"
 * (#58, the main defense against dispatcher sprawl).
 *
 * Driven off the active *transport*, not `useMobile`'s `isMobile` — those are
 * unrelated concepts. A narrow desktop window is mobile-*sized* but still
 * fully capable (talks to a real server); conflating the two would hide
 * these features on desktop too, which is a regression, not a fix.
 *
 * Hide rather than proxy to the remote: a paired mobile session's remote
 * credentials live in Rust secure storage (#54) and never cross into the
 * WebView, so there's no mechanism today for the frontend to reach the
 * remote's admin/plugin/ML endpoints directly. Building that proxy is a
 * separate, larger feature — #58 only asks to hide what can't work yet.
 *
 * Not reactive (like `isTauri()`): the transport is selected once at boot
 * and never toggled mid-session in practice, so a plain boolean read once
 * per component setup is enough — no `computed`/ref plumbing needed.
 */
export function useCapabilities() {
  const isLocalMode = isLocalTransportActive();
  return {
    isLocalMode,
    canUseAdmin: !isLocalMode,
    canUseGroupsAndSharing: !isLocalMode,
    canUsePlugins: !isLocalMode,
    canUseMlOrganize: !isLocalMode,
    canUseEntityGraph: !isLocalMode,
    canUseReindex: !isLocalMode,
    canUseArchiveImportExport: !isLocalMode,
    /** Typst Preview and PDF export compile on the server (#139, #140). */
    canUseTypstRendering: !isLocalMode,
  };
}
