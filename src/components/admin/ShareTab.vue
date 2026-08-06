<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, Copy, Check, QrCode, WifiOff, Cable, Globe } from "lucide-vue-next";
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

watch(gatewayBase, (base) => {
  if (base) {
    qrUrlFetched = "";
    fetchAll();
  }
});

onMounted(() => {
  fetchAll();
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

@media (max-width: 900px) {
  .share-grid {
    grid-template-columns: 1fr;
  }
  .guide-grid {
    grid-template-columns: 1fr;
  }
}
</style>
