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
 *
 * Only used for read-only observation (maximize state on resize) — all
 * window *operations* go through the Rust WindowManager via invoke so the
 * manager's cached WindowState stays in sync with the real window.
 */
async function getCurrentWindow(): Promise<Window | null> {
  if (!isTauriEnv()) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

/** Invoke a window-control command on the Rust WindowManager. */
async function windowCommand<T = void>(cmd: string): Promise<T | undefined> {
  if (!isTauriEnv()) return undefined;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<T>(cmd);
  } catch {
    return undefined;
  }
}

/**
 * Window-control API for the custom title bar. Every operation degrades to a
 * safe no-op when not running under Tauri, so the header renders identically
 * in a plain browser.
 *
 * Operations go through `src-tauri/src/base/cmd/window.rs` → WindowManager,
 * keeping the manager's cached WindowState consistent with the actual window
 * (tray toggle / lightweight-mode detection depend on it).
 */
export function useTauriWindow() {
  const isTauri = isTauriEnv();
  const isMaximized = ref(false);

  let alive = true;
  let unlisten: (() => void) | undefined;

  async function refreshMaximized() {
    const maximized = await windowCommand<boolean>("is_maximized_window");
    if (maximized !== undefined) isMaximized.value = maximized;
  }

  async function minimize() {
    await windowCommand("minimize_window");
  }

  async function toggleMaximize() {
    await windowCommand("toggle_maximize_window");
    void refreshMaximized();
  }

  async function close() {
    await windowCommand("close_window");
  }

  onMounted(() => {
    if (!isTauri) return;
    void (async () => {
      const win = await getCurrentWindow();
      if (!win || !alive) return;
      await refreshMaximized();
      // Read-only observation: keep the maximize icon in sync while the
      // window is resized (drag edges / double-click titlebar).
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
