import { onBeforeUnmount, onMounted, ref } from "vue";

/** Detect whether we are running inside the Tauri webview (vs. a plain browser). */
export function isTauriEnv(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Window-control API for the custom title bar.
 *
 * 通过 Tauri 事件与 Rust 侧通信（而不是 invoke 自定义命令）：chat 前端运行在
 * 网关的远程源（http://127.0.0.1:PORT），Tauri 的命令 ACL 只放行本地源，
 * invoke 自定义命令会被静默拒绝；事件通道不受命令 ACL 限制（与 navigate-to-*
 * 同一思路）。Rust 侧收到事件后走 WindowManager，WM 缓存状态保持与实际窗口一致。
 */
export function useTauriWindow() {
  const isTauri = isTauriEnv();
  const isMaximized = ref(false);

  let alive = true;
  let unlistenMaximized: (() => void) | undefined;

  /** 向 Rust 侧发窗口控制事件；非 Tauri 环境下安全降级为 no-op。 */
  async function windowEvent(event: string) {
    if (!isTauriEnv()) return;
    try {
      const { emit } = await import("@tauri-apps/api/event");
      await emit(event);
    } catch {
      // non-critical
    }
  }

  function minimize() {
    return windowEvent("window-minimize");
  }

  function toggleMaximize() {
    return windowEvent("window-toggle-maximize");
  }

  function close() {
    return windowEvent("window-close");
  }

  onMounted(() => {
    if (!isTauri) return;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      // Rust 侧在最大化状态变化（toggle / resize 等）时回发事件同步图标
      unlistenMaximized = await listen<boolean>("window-maximized-changed", (e) => {
        if (alive) isMaximized.value = !!e.payload;
      });
      // 主动查询一次当前状态（替代被 ACL 拦截的 is_maximized_window invoke）
      await windowEvent("window-query-maximized");
    })();
  });

  onBeforeUnmount(() => {
    alive = false;
    unlistenMaximized?.();
    unlistenMaximized = undefined;
  });

  return { isTauri, isMaximized, minimize, toggleMaximize, close };
}
