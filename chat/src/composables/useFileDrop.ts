import { ref, onUnmounted, type Ref } from "vue";

export interface UseFileDropOptions {
  /** 是否接受拖拽（例如 pi 未连接时返回 false 则忽略） */
  enabled: () => boolean;
  /** 放下文件时回调 */
  onFiles: (files: File[]) => void;
  /**
   * 可选：命中测试目标元素。Tauri 原生拖拽拿不到 DOM 目标，
   * 需要按坐标判断指针是否落在该元素内（用于精确高亮/接收）。
   * 不传则整窗接受。
   */
  target?: Ref<HTMLElement | null>;
}

const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** 按扩展名推断 MIME（原生拖拽只有路径，没有 File 的 type） */
function mimeFromName(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() || "";
  const imageExts: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    bmp: "image/bmp",
    svg: "image/svg+xml",
  };
  if (imageExts[ext]) return imageExts[ext];
  const textExts = new Set([
    "txt", "md", "json", "csv", "log",
    "js", "ts", "tsx", "jsx", "rs", "py", "go", "java", "c", "h",
    "cpp", "hpp", "html", "css", "xml", "yaml", "yml", "toml", "sh",
  ]);
  return textExts.has(ext) ? "text/plain" : "";
}

/** 把拖入的文件系统路径读成 File 对象（后续复用 filesToAttachments 处理） */
async function filesFromPaths(
  paths: string[],
  readFile: (path: string) => Promise<Uint8Array>,
): Promise<File[]> {
  const files: File[] = [];
  for (const p of paths) {
    try {
      const bytes = await readFile(p);
      const name = p.split(/[\\/]/).pop() || "file";
      files.push(new File([bytes], name, { type: mimeFromName(name) }));
    } catch {
      // 单个文件读取失败不影响其余文件
    }
  }
  return files;
}

/**
 * 文件拖拽支持（Composer / 新会话准备页共用），双通道：
 * - 普通浏览器：HTML5 dragenter/dragover/drop（OS 文件拖放可用）；
 * - Tauri 桌面：浏览器 drop 事件不会触发（原生窗口层拦截），
 *   改为订阅 getCurrentWebviewWindow().onDragDropEvent —— 事件只带
 *   文件系统路径，用 @tauri-apps/plugin-fs 读回字节后转成 File。
 * 只对"拖入文件"生效，不影响文本框内选中文本的内部拖拽。
 */
export function useFileDrop(options: UseFileDropOptions) {
  const isDragging = ref(false);
  /** enter/leave 嵌套计数（HTML5 模式）：在子元素间移动时不会闪烁 */
  let dragDepth = 0;
  let unlisten: (() => void) | null = null;

  // ── HTML5 通道（浏览器模式） ──────────────────────────────────────

  function hasFiles(e: DragEvent): boolean {
    return !!e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");
  }

  function onDragEnter(e: DragEvent) {
    if (!options.enabled() || !hasFiles(e)) return;
    e.preventDefault();
    dragDepth++;
    isDragging.value = true;
  }

  function onDragOver(e: DragEvent) {
    if (!options.enabled() || !hasFiles(e)) return;
    // 必须阻止默认行为，否则 drop 不会触发
    e.preventDefault();
  }

  function onDragLeave(e: DragEvent) {
    if (!hasFiles(e)) return;
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) isDragging.value = false;
  }

  function onDrop(e: DragEvent) {
    dragDepth = 0;
    isDragging.value = false;
    if (!options.enabled()) return;
    const files = Array.from(e.dataTransfer?.files || []);
    if (!files.length) return;
    e.preventDefault();
    options.onFiles(files);
  }

  // ── Tauri 原生通道 ────────────────────────────────────────────────

  /** 原生拖拽坐标（物理像素）换算成 CSS 像素后做命中测试 */
  function hitTarget(x: number, y: number): boolean {
    const el = options.target?.value;
    if (!el) return true; // 未指定目标元素：整窗接受
    const dpr = window.devicePixelRatio || 1;
    const node = document.elementFromPoint(x / dpr, y / dpr);
    return !!node && (el === node || el.contains(node));
  }

  async function setupNative() {
    try {
      const [{ getCurrentWebviewWindow }, { readFile }] = await Promise.all([
        import("@tauri-apps/api/webviewWindow"),
        import("@tauri-apps/plugin-fs"),
      ]);
      unlisten = await getCurrentWebviewWindow().onDragDropEvent(async (event) => {
        const { type } = event.payload;
        if (type === "over") {
          // 未启用时保持无高亮（如 pi 未连接）
          if (!options.enabled()) {
            isDragging.value = false;
            return;
          }
          const pos = event.payload.position;
          isDragging.value = hitTarget(pos.x, pos.y);
          return;
        }
        if (type === "leave") {
          isDragging.value = false;
          return;
        }
        if (type === "drop") {
          isDragging.value = false;
          if (!options.enabled()) return;
          const pos = event.payload.position;
          if (!hitTarget(pos.x, pos.y)) return; // 落在目标区域外不接收
          const files = await filesFromPaths(event.payload.paths, readFile);
          if (files.length) options.onFiles(files);
        }
      });
    } catch {
      // 非 Tauri 或缺少权限：退回 HTML5 通道（桌面下 OS 文件拖放本就无事件）
    }
  }

  if (isTauri) setupNative();
  onUnmounted(() => unlisten?.());

  return { isDragging, onDragEnter, onDragOver, onDragLeave, onDrop };
}
