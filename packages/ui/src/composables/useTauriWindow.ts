import { onBeforeUnmount, onMounted, ref } from "vue";
import type { Window } from "@tauri-apps/api/window";

/** Detect whether we are running inside the Tauri webview (vs. a plain browser). */
export function isTauriEnv(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Resolve the current window, or `null` outside Tauri. `@tauri-apps/api` is
 * loaded via dynamic import so a plain-web build never pulls it into the
 * eager bundle (and never executes it at runtime in a browser).
 */
async function getCurrentWindow(): Promise<Window | null> {
  if (!isTauriEnv()) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

/**
 * Window-control API for the custom title bar. Every operation degrades to a
 * safe no-op when not running under Tauri, so the header renders identically
 * in a plain browser.
 */
export function useTauriWindow() {
  const isTauri = isTauriEnv();
  const isMaximized = ref(false);

  let alive = true;
  let unlisten: (() => void) | undefined;

  async function refreshMaximized() {
    const win = await getCurrentWindow();
    if (win) isMaximized.value = await win.isMaximized();
  }

  async function minimize() {
    const win = await getCurrentWindow();
    if (win) await win.minimize();
  }

  async function toggleMaximize() {
    const win = await getCurrentWindow();
    if (win) await win.toggleMaximize();
  }

  async function close() {
    const win = await getCurrentWindow();
    if (win) await win.close();
  }

  onMounted(() => {
    if (!isTauri) return;
    void (async () => {
      const win = await getCurrentWindow();
      if (!win || !alive) return;
      isMaximized.value = await win.isMaximized();
      unlisten = await win.onResized(() => {
        if (alive) void refreshMaximized();
      });
    })();
  });

  onBeforeUnmount(() => {
    alive = false;
    unlisten?.();
    unlisten = undefined;
  });

  return { isTauri, isMaximized, minimize, toggleMaximize, close };
}
