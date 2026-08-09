import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import type { Ref } from "vue";
import { useI18n } from "vue-i18n";

// ─── LAN 分享 + 鉴权 数据与逻辑（ShareTab 专属）────────────────────────
// 聚合两块 REST 数据：① LAN 分享（/api/lan-info、/api/lan-qr、/api/health，
// 含 5s 轮询以便 Wi-Fi 切换后 IP/QR 自动刷新）；② LAN 鉴权（config/devices，
// PIN 仅在"重新生成"时明文返回一次，UI 仅保留在内存中展示）。

// Payload shapes of the gateway REST endpoints (see gateway/handlers/system.rs).
export interface LanInfo {
  broker_ws_url: string;
  http_url: string;
  lan_urls: string[];
  qr_data: string;
}

export interface HealthInfo {
  status: string;
  version: string;
  pi_version: string;
  lan_urls: string[];
  broker_url: string;
  uptime_secs: number;
}

export interface LanAuthConfigResponse {
  success: boolean;
  enabled?: boolean;
  pinSet?: boolean;
  pin?: string;
  error?: string;
}

export interface LanDevice {
  token: string;
  createdAt: string;
  expiresAt: string;
}

export function useLanShare(gatewayBase: Ref<string>, onRefresh?: () => void) {
  const { t } = useI18n();

  // ── LAN 分享（QR + URL + 连接信息）──
  const lanInfo = ref<LanInfo | null>(null);
  const health = ref<HealthInfo | null>(null);
  const qrSvg = ref("");
  const fetching = ref(false);
  const error = ref("");
  const copied = ref(false);

  const displayUrl = computed(() => lanInfo.value?.qr_data || lanInfo.value?.lan_urls?.[0] || "");

  const gatewayPort = computed(() => {
    const url = lanInfo.value?.http_url || health.value?.broker_url || "";
    const m = url.match(/:(\d+)/);
    return m ? m[1] : "";
  });

  const manualExample = computed(() => lanInfo.value?.lan_urls?.[0] || displayUrl.value);

  const online = computed(() => health.value?.status === "ok" || !!lanInfo.value);

  let qrUrlFetched = "";
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  async function fetchLanInfo() {
    if (!gatewayBase.value) return;
    const resp = await fetch(`${gatewayBase.value}api/lan-info`);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    lanInfo.value = await resp.json();
    // Refresh the QR only when its payload URL changed (e.g. after a wifi
    // switch the backend redisovers the LAN IP within its 2s TTL).
    const next = displayUrl.value;
    if (next && next !== qrUrlFetched) {
      qrUrlFetched = next;
      await fetchQr();
    }
  }

  async function fetchQr() {
    if (!gatewayBase.value) return;
    try {
      const resp = await fetch(`${gatewayBase.value}api/lan-qr`);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const svg = await resp.text();
      if (svg.trim()) qrSvg.value = svg;
    } catch (e) {
      error.value = t("admin.qrLoadError", { msg: `${e}` });
    }
  }

  async function fetchHealth() {
    if (!gatewayBase.value) return;
    const resp = await fetch(`${gatewayBase.value}api/health`);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    health.value = await resp.json();
  }

  async function fetchAll(silent = false) {
    if (!gatewayBase.value) {
      error.value = "";
      return;
    }
    if (!silent) fetching.value = true;
    error.value = "";
    try {
      const [lan] = await Promise.allSettled([fetchLanInfo(), fetchHealth()]);
      if (lan.status === "rejected") {
        error.value = t("admin.lanInfoLoadError", { msg: `${lan.reason}` });
      }
      // health is best-effort; a failure here doesn't block the share card
    } finally {
      if (!silent) fetching.value = false;
    }
  }

  async function handleRefresh() {
    qrUrlFetched = "";
    onRefresh?.(); // refresh Tauri-side status so the gateway base stays fresh
    await fetchAll(false);
  }

  function copyUrl() {
    if (!displayUrl.value) return;
    const done = () => {
      copied.value = true;
      setTimeout(() => (copied.value = false), 2000);
    };
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(displayUrl.value).then(done).catch(() => fallbackCopy(done));
    } else {
      fallbackCopy(done);
    }
  }

  function fallbackCopy(done: () => void) {
    const ta = document.createElement("textarea");
    ta.value = displayUrl.value;
    ta.style.cssText = "position:fixed;left:-9999px";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    document.body.removeChild(ta);
    done();
  }

  function fmtUptime(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }

  // ── LAN auth (PIN gate, 0.2.0 P3) ────────────────────────────────────────
  const authEnabled = ref(false);
  const pinSet = ref(false);
  /** 内存中的当前 PIN（重新生成后明文展示一次；刷新页面后回到未知态） */
  const pin = ref<string | null>(null);
  const pinVisible = ref(false);
  const devices = ref<LanDevice[]>([]);
  const authBusy = ref(false);
  const authError = ref("");
  const pinCopied = ref(false);

  async function fetchLanAuth() {
    if (!gatewayBase.value) return;
    try {
      const [cfgRes, devRes] = await Promise.all([
        fetch(`${gatewayBase.value}api/lan/auth/config`),
        fetch(`${gatewayBase.value}api/lan/auth/devices`),
      ]);
      const cfg: LanAuthConfigResponse = await cfgRes.json();
      const dev = await devRes.json();
      if (cfg.success) {
        authEnabled.value = !!cfg.enabled;
        pinSet.value = !!cfg.pinSet;
      }
      if (dev.success) devices.value = dev.devices ?? [];
    } catch (e) {
      authError.value = t("admin.lanLoadFailed", { msg: `${e}` });
    }
  }

  async function saveAuth(body: Record<string, unknown>): Promise<LanAuthConfigResponse | null> {
    if (!gatewayBase.value) return null;
    authBusy.value = true;
    authError.value = "";
    try {
      const res = await fetch(`${gatewayBase.value}api/lan/auth/config`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      const data: LanAuthConfigResponse = await res.json();
      if (data.success !== true) throw new Error(data.error ?? "save failed");
      pinSet.value = !!data.pinSet;
      return data;
    } catch (e) {
      authError.value = t("admin.lanSaveFailed", { msg: `${e}` });
      return null;
    } finally {
      authBusy.value = false;
    }
  }

  async function toggleAuth() {
    const target = !authEnabled.value;
    // 首次开启且还没有 PIN → 一并生成并展示（启用才有意义）
    const data = await saveAuth({ enabled: target, regenerate: target && !pinSet.value });
    if (!data) return;
    authEnabled.value = target;
    if (data.pin) {
      pin.value = data.pin;
      pinVisible.value = true;
    }
    if (target) await fetchLanAuth();
  }

  async function regeneratePin() {
    const data = await saveAuth({ regenerate: true });
    if (!data) return;
    pin.value = data.pin ?? null;
    pinVisible.value = true;
  }

  async function revokeDevice(token: string) {
    if (!gatewayBase.value) return;
    if (!window.confirm(t("admin.lanRevokeConfirm"))) return;
    authBusy.value = true;
    authError.value = "";
    try {
      const res = await fetch(
        `${gatewayBase.value}api/lan/auth/devices/${encodeURIComponent(token)}`,
        { method: "DELETE" },
      );
      const data = await res.json();
      if (data.success !== true) throw new Error(data.error ?? "revoke failed");
      devices.value = devices.value.filter((d) => d.token !== token);
    } catch (e) {
      authError.value = t("admin.lanSaveFailed", { msg: `${e}` });
    } finally {
      authBusy.value = false;
    }
  }

  async function revokeAll() {
    if (!gatewayBase.value) return;
    if (!window.confirm(t("admin.lanRevokeAllConfirm"))) return;
    authBusy.value = true;
    authError.value = "";
    try {
      const res = await fetch(`${gatewayBase.value}api/lan/auth/revoke`, { method: "POST" });
      const data = await res.json();
      if (data.success !== true) throw new Error(data.error ?? "revoke failed");
      devices.value = [];
    } catch (e) {
      authError.value = t("admin.lanSaveFailed", { msg: `${e}` });
    } finally {
      authBusy.value = false;
    }
  }

  function copyPin() {
    if (!pin.value) return;
    const done = () => {
      pinCopied.value = true;
      setTimeout(() => (pinCopied.value = false), 2000);
    };
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(pin.value).then(done).catch(() => fallbackCopyText(pin.value!, done));
    } else {
      fallbackCopyText(pin.value, done);
    }
  }

  function fallbackCopyText(text: string, done: () => void) {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.cssText = "position:fixed;left:-9999px";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    document.body.removeChild(ta);
    done();
  }

  function fmtDate(iso: string): string {
    const d = new Date(iso);
    return Number.isFinite(d.getTime())
      ? d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" })
      : "—";
  }

  // ── 生命周期：网关基址变化时重新拉取；5s 轮询让 IP/QR 在 Wi-Fi 切换后自动刷新 ──
  watch(gatewayBase, (base) => {
    if (base) {
      qrUrlFetched = "";
      fetchAll();
      fetchLanAuth();
    }
  });

  onMounted(() => {
    fetchAll();
    fetchLanAuth();
    // Poll so the LAN URL / QR refresh automatically after the backend
    // rediscovers the IP (2s TTL) — no restart needed on wifi change.
    pollTimer = setInterval(() => fetchAll(true), 5000);
  });
  onUnmounted(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  return {
    // LAN 分享
    lanInfo,
    health,
    qrSvg,
    fetching,
    error,
    copied,
    displayUrl,
    gatewayPort,
    manualExample,
    online,
    fetchAll,
    handleRefresh,
    copyUrl,
    fmtUptime,
    // LAN 鉴权
    authEnabled,
    pinSet,
    pin,
    pinVisible,
    devices,
    authBusy,
    authError,
    pinCopied,
    fetchLanAuth,
    toggleAuth,
    regeneratePin,
    revokeDevice,
    revokeAll,
    copyPin,
    fmtDate,
  };
}
