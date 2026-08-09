import { ref, computed, watch } from "vue";
import type { Ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import type { PiInstallInfo, DownloadProgressEvent } from "./useAdmin";

// ─── Pi 运行时安装（PiVersionsTab）：版本下载/切换/卸载 + 进度 ────────
// 逻辑集中在 composable（deps 注入来自父级的 installInfo/download 等），
// 展示在 PiVersionInstallCard，PiVersionsTab 只做模板组装。

const PI_RELEASES_URL = "https://github.com/earendil-works/pi/releases";
const PI_HOMEPAGE_URL = "https://pi.dev";

export function usePiVersions(deps: {
  installInfo: Ref<PiInstallInfo | null>;
  downloadProgress: Ref<DownloadProgressEvent | null>;
  downloading: Ref<boolean>;
  uninstalling: Ref<boolean>;
  download: (version: string) => Promise<boolean>;
  onUninstall: () => void;
}) {
  const { t } = useI18n();

  const downloadInput = ref("");
  const actionFeedback = ref("");
  // Set when a download fails so the network hint only appears on failure.
  const networkHint = ref(false);

  async function openLink(url: string) {
    try {
      await invoke("open_path", { path: url });
    } catch {
      window.open(url, "_blank");
    }
  }

  function openReleases() {
    openLink(PI_RELEASES_URL);
  }

  function openHomepage() {
    openLink(PI_HOMEPAGE_URL);
  }

  // Pre-fill the download input once pi is installed. Prefer the currently
  // installed version (e.g. right after a user-triggered download) and fall
  // back to the pinned version — otherwise the field would jump back to the
  // pinned version after the user downloads a different one.
  watch(
    deps.installInfo,
    (info) => {
      if (info?.binary_present && info.locked_version && !downloadInput.value) {
        downloadInput.value = info.version ?? info.locked_version;
      }
    },
    { immediate: true }
  );

  async function handleDownload() {
    const v = downloadInput.value.trim();
    if (!v) return;
    downloadInput.value = "";
    networkHint.value = false;
    const ok = await deps.download(v);
    if (!ok) networkHint.value = true;
  }

  async function handleUninstall() {
    actionFeedback.value = t("admin.feedbackUninstalling");
    deps.onUninstall();
    setTimeout(() => { actionFeedback.value = ""; }, 3000);
  }

  const busy = () => deps.downloading.value || deps.uninstalling.value;

  // ─── Download progress helpers ────────────────────────────────────────

  const progressPercent = computed(() => {
    const p = deps.downloadProgress.value;
    if (!p) return 0;
    switch (p.stage) {
      case "downloading":
        return p.total
          ? Math.min(100, Math.round(((p.downloaded ?? 0) / p.total) * 100))
          : 0;
      case "extracting":
        return p.total_entries
          ? Math.min(100, Math.round(((p.current ?? 0) / p.total_entries) * 100))
          : 0;
      case "verifying":
      case "done":
        return 100;
      default:
        return 0;
    }
  });

  const progressText = computed(() => {
    const p = deps.downloadProgress.value;
    if (!p) return "";
    const mb = (n?: number) => (n !== undefined ? (n / 1024 / 1024).toFixed(1) : "?");
    switch (p.stage) {
      case "downloading": {
        const pct = p.total
          ? ` ${Math.round(((p.downloaded ?? 0) / p.total) * 100)}%`
          : "";
        return `${t("admin.progressDownloading", { current: mb(p.downloaded), total: mb(p.total) })}${pct}`;
      }
      case "extracting": {
        const pct = p.total_entries
          ? ` ${Math.round(((p.current ?? 0) / p.total_entries) * 100)}%`
          : "";
        return `${t("admin.progressExtracting")}${pct}`;
      }
      case "verifying":
        return t("admin.progressVerifying");
      case "done":
        return t("admin.progressDone");
      default:
        return "";
    }
  });

  return {
    downloadInput,
    actionFeedback,
    networkHint,
    openLink,
    openReleases,
    openHomepage,
    handleDownload,
    handleUninstall,
    busy,
    progressPercent,
    progressText,
  };
}
