<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, Copy, Check, QrCode, WifiOff, Cable, Globe, KeyRound, Eye, EyeOff, Trash2 } from "lucide-vue-next";
import { EmptyState } from "@piter/ui";
import type { AdminStatus } from "../../composables/useAdmin";

const { t } = useI18n();

const props = defineProps<{
  status: AdminStatus | null;
  loading: boolean;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
}>();

// Payload shapes of the gateway REST endpoints (see gateway/handlers/system.rs).
interface LanInfo {
  broker_ws_url: string;
  http_url: string;
  lan_urls: string[];
  qr_data: string;
}

interface HealthInfo {
  status: string;
  version: string;
  pi_version: string;
  lan_urls: string[];
  broker_url: string;
  uptime_secs: number;
}

const lanInfo = ref<LanInfo | null>(null);
const health = ref<HealthInfo | null>(null);
const qrSvg = ref("");
const fetching = ref(false);
const error = ref("");
const copied = ref(false);

// The Tauri-side status carries the gateway base URL; the admin frontend runs
// on a different origin (tauri.localhost / vite dev server), so /api/* must be
// reached through this absolute base.
const gatewayBase = computed(() => {
  const base = props.status?.broker_http_url ?? "";
  return base.endsWith("/") ? base : base ? `${base}/` : "";
});

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
  emit("refresh"); // refresh Tauri-side status so the gateway base stays fresh
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

// ─── LAN auth (PIN gate, 0.2.0 P3) ────────────────────────────────────────
// 配置走网关 REST；PIN 仅在"重新生成"时明文返回一次，UI 仅保留在内存中展示。
// 首次开启且尚无 PIN 时自动生成（一并展示）。
interface LanAuthConfigResponse {
  success: boolean;
  enabled?: boolean;
  pinSet?: boolean;
  pin?: string;
  error?: string;
}
interface LanDevice {
  token: string;
  createdAt: string;
  expiresAt: string;
}

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
</script>

<template>
  <div class="tab-content">
    <div class="tab-header">
      <div class="tab-header-info">
        <h3 class="tab-title">{{ $t("admin.shareTitle") }}</h3>
        <p class="tab-desc">{{ $t("admin.shareDesc") }}</p>
      </div>
      <button class="btn btn-sm" :disabled="loading || !gatewayBase" @click="handleRefresh">
        <RefreshCw :size="12" :class="{ spin: fetching || loading }" />
        {{ fetching || loading ? $t("common.refreshing") : $t("admin.refresh") }}
      </button>
    </div>

    <EmptyState
      v-if="!gatewayBase"
      :title="$t('admin.shareUnavailableTitle')"
      :hint="$t('admin.shareUnavailableHint')"
    >
      <template #icon><WifiOff :size="28" /></template>
    </EmptyState>

    <template v-else>
      <div class="share-grid">
        <!-- LAN share: QR + copyable URL -->
        <div class="share-card">
          <div class="share-card-header">
            <span class="share-card-title">
              <QrCode :size="14" />
              {{ $t("admin.lanShare") }}
            </span>
            <span class="share-card-desc">{{ $t("admin.lanShareDesc") }}</span>
          </div>

          <div v-if="qrSvg" class="qr-frame">
            <div class="qr-svg" v-html="qrSvg" />
          </div>
          <div v-else-if="error" class="share-error">{{ error }}</div>
          <div v-else class="qr-placeholder">
            <RefreshCw :size="18" class="spin" />
          </div>

          <div v-if="displayUrl" class="url-row">
            <code class="url-text">{{ displayUrl }}</code>
            <button
              class="btn btn-ghost btn-icon btn-sm"
              :title="copied ? $t('common.copied') : $t('admin.copyUrlHint')"
              @click="copyUrl"
            >
              <Check v-if="copied" :size="14" />
              <Copy v-else :size="14" />
            </button>
          </div>
        </div>

        <!-- Connection info: broker endpoints + health -->
        <div class="share-card">
          <div class="share-card-header">
            <span class="share-card-title">
              <Cable :size="14" />
              {{ $t("admin.connectionInfo") }}
            </span>
            <span class="share-card-desc">{{ $t("admin.connectionInfoDesc") }}</span>
          </div>

          <div class="info-block">
            <div class="info-row">
              <span class="info-key">{{ $t("admin.brokerWsUrl") }}</span>
              <code class="info-value">{{ lanInfo?.broker_ws_url || "—" }}</code>
            </div>
            <div class="info-row">
              <span class="info-key">{{ $t("admin.brokerHttpUrl") }}</span>
              <code class="info-value">{{ lanInfo?.http_url || "—" }}</code>
            </div>
            <div class="info-row">
              <span class="info-key">{{ $t("admin.gatewayPort") }}</span>
              <code class="info-value">{{ gatewayPort || "—" }}</code>
            </div>
            <div class="info-row">
              <span class="info-key">{{ $t("admin.healthStatus") }}</span>
              <span class="info-value">
                <span class="badge" :class="online ? 'badge-success' : 'badge-muted'">
                  {{ online ? $t("admin.online") : $t("admin.offline") }}
                </span>
              </span>
            </div>
            <div class="info-row">
              <span class="info-key">{{ $t("admin.uptime") }}</span>
              <code class="info-value">{{ health ? fmtUptime(health.uptime_secs) : "—" }}</code>
            </div>
            <div class="info-row">
              <span class="info-key">{{ $t("admin.piVersion") }}</span>
              <code class="info-value">{{ health?.pi_version || "—" }}</code>
            </div>
          </div>
        </div>
      </div>

      <!-- LAN access PIN gate -->
      <div class="share-card lan-auth-card">
        <div class="share-card-header">
          <span class="share-card-title">
            <KeyRound :size="14" />
            {{ $t("admin.lanAuth") }}
          </span>
          <span class="share-card-desc">{{ $t("admin.lanAuthDesc") }}</span>
        </div>

        <div v-if="authError" class="lan-auth-error">{{ authError }}</div>

        <div class="lan-auth-main">
          <!-- PIN block -->
          <div class="lan-pin-block">
            <div class="lan-pin-row">
              <span class="lan-pin-code">
                <!-- pin 在内存中：明文/模糊切换；仅已设置但刷新后不可见（pinSet
                     && !pin）→ 提示"已设置 · 重新生成可查看"，避免误判为未设置 -->
                {{ pin ? (pinVisible ? pin : "••••••") : (pinSet ? $t("admin.lanPinMasked") : $t("admin.lanPinNotSet")) }}
              </span>
              <button
                v-if="pin"
                class="btn btn-ghost btn-icon btn-sm"
                :title="pinVisible ? $t('admin.lanHidePin') : $t('admin.lanShowPin')"
                @click="pinVisible = !pinVisible"
              >
                <EyeOff v-if="pinVisible" :size="14" />
                <Eye v-else :size="14" />
              </button>
              <button
                v-if="pin"
                class="btn btn-ghost btn-icon btn-sm"
                :title="pinCopied ? $t('common.copied') : $t('admin.copyUrlHint')"
                @click="copyPin"
              >
                <Check v-if="pinCopied" :size="14" />
                <Copy v-else :size="14" />
              </button>
            </div>
            <p class="lan-pin-hint">{{ $t("admin.lanPinHint") }}</p>
            <div class="lan-auth-actions">
              <label class="lan-toggle">
                <input
                  type="checkbox"
                  :checked="authEnabled"
                  :disabled="authBusy"
                  @change="toggleAuth"
                />
                <span>{{ $t("admin.lanPinEnabled") }}</span>
              </label>
              <button class="btn btn-sm" :disabled="authBusy" @click="regeneratePin">
                {{ $t("admin.lanRegeneratePin") }}
              </button>
            </div>
          </div>

          <!-- Authorized devices -->
          <div class="lan-devices">
            <div class="lan-devices-head">
              <span class="lan-devices-title">{{ $t("admin.lanDevices") }}</span>
              <button
                v-if="devices.length"
                class="btn btn-ghost btn-sm"
                :disabled="authBusy"
                @click="revokeAll"
              >
                <Trash2 :size="12" />
                {{ $t("admin.lanRevokeAll") }}
              </button>
            </div>
            <div v-if="!devices.length" class="lan-empty">{{ $t("admin.lanDevicesEmpty") }}</div>
            <div v-else class="lan-device-list">
              <div v-for="d in devices" :key="d.token" class="lan-device-row">
                <div class="lan-device-meta">
                  <code class="lan-device-id">{{ d.token.slice(0, 8) }}…</code>
                  <span class="lan-device-date">{{ fmtDate(d.createdAt) }} → {{ fmtDate(d.expiresAt) }}</span>
                </div>
                <button
                  class="btn btn-ghost btn-icon btn-sm"
                  :disabled="authBusy"
                  :title="$t('admin.lanRevoke')"
                  @click="revokeDevice(d.token)"
                >
                  <Trash2 :size="13" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Connection guide: phone steps + manual fallback -->
      <div class="section">
        <h3 class="tab-title">{{ $t("admin.connectGuide") }}</h3>
        <p class="tab-desc section-desc">{{ $t("admin.connectGuideDesc") }}</p>

        <div class="guide-grid">
          <div class="guide-card">
            <span class="guide-step">1</span>
            <span class="guide-text">{{ $t("admin.step1") }}</span>
          </div>
          <div class="guide-card">
            <span class="guide-step">2</span>
            <span class="guide-text">{{ $t("admin.step2") }}</span>
          </div>
          <div class="guide-card">
            <span class="guide-step">3</span>
            <span class="guide-text">{{ $t("admin.step3") }}</span>
          </div>
        </div>

        <div class="manual-card">
          <span class="manual-title">
            <Globe :size="13" />
            {{ $t("admin.manualEntry") }}
          </span>
          <span class="manual-desc">{{ $t("admin.manualEntryDesc") }}</span>
          <code v-if="manualExample" class="manual-example">{{ $t("admin.manualEntryExample", { url: manualExample }) }}</code>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
}

.tab-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: var(--space-lg);
}

.tab-header-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0;
  padding-top: var(--space-lg);
}

.tab-title:first-child {
  padding-top: 0;
}

.tab-header .tab-title {
  padding-top: 0;
  margin: 0;
}

.tab-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
}

.section {
  margin-top: var(--space-md);
}

.section-desc {
  margin-top: var(--space-xs);
  margin-bottom: var(--space-sm);
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

.share-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-md);
}

.share-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-lg);
}

.share-card-header {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.share-card-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-body);
  font-weight: 500;
  color: var(--text);
}

.share-card-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.qr-frame {
  display: flex;
  justify-content: center;
  padding: var(--space-sm);
  background: #fff;
  border-radius: var(--radius-sm);
}

.qr-svg :deep(svg) {
  width: 200px;
  height: 200px;
  display: block;
}

.qr-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 216px;
  color: var(--text-tertiary);
}

.share-error {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 216px;
  font-size: var(--font-size-caption);
  color: var(--danger);
  text-align: center;
}

.url-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-xs) var(--space-sm);
}

.url-text {
  flex: 1;
  font-family: var(--font-mono);
  font-size: var(--font-size-micro);
  color: var(--text-secondary);
  word-break: break-all;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.info-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.info-row {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  padding: var(--space-xxs) 0;
}

.info-key {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  min-width: 120px;
  flex-shrink: 0;
}

.info-value {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
  word-break: break-all;
}

.guide-grid {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: var(--space-sm);
}

.guide-card {
  display: flex;
  align-items: flex-start;
  gap: var(--space-sm);
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-md);
}

.guide-step {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent-strong);
  font-size: var(--font-size-caption);
  font-weight: 600;
}

.guide-text {
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
  line-height: 1.5;
}

.manual-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
  margin-top: var(--space-sm);
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-md);
}

.manual-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-body);
  font-weight: 500;
  color: var(--text);
}

.manual-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.manual-example {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--accent-strong);
  word-break: break-all;
}

/* ── LAN auth card ── */
.lan-auth-card {
  margin-top: var(--space-md);
}

.lan-auth-error {
  padding: var(--space-xs) var(--space-sm);
  background: var(--danger-soft);
  color: var(--danger);
  border-radius: var(--radius-sm);
  font-size: var(--font-size-caption);
}

.lan-auth-main {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-lg);
  align-items: start;
}

.lan-pin-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.lan-pin-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.lan-pin-code {
  font-family: var(--font-mono);
  font-size: 22px;
  font-weight: 600;
  letter-spacing: 0.35em;
  color: var(--text);
  font-variant-numeric: tabular-nums;
  min-height: 28px;
}

.lan-pin-hint {
  margin: 0;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  line-height: 1.5;
}

.lan-auth-actions {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  margin-top: var(--space-xs);
  flex-wrap: wrap;
}

.lan-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: var(--font-size-control);
  color: var(--text);
  cursor: pointer;
}

.lan-devices {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
  min-width: 0;
}

.lan-devices-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-md);
}

.lan-devices-title {
  font-size: var(--font-size-control);
  font-weight: 500;
  color: var(--text);
}

.lan-empty {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.lan-device-list {
  display: grid;
  gap: var(--space-xs);
}

.lan-device-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-sm);
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-xs) var(--space-sm);
}

.lan-device-meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.lan-device-id {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
}

.lan-device-date {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

@media (max-width: 900px) {
  .share-grid {
    grid-template-columns: 1fr;
  }
  .guide-grid {
    grid-template-columns: 1fr;
  }
  .lan-auth-main {
    grid-template-columns: 1fr;
  }
}
</style>
