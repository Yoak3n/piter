<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RefreshCw, Copy, Check, QrCode, WifiOff, Globe, MonitorSmartphone, Users, FolderOpen } from "lucide-vue-next";
import { EmptyState } from "@piter/ui";
import LanAuthPanel from "./LanAuthPanel.vue";
import ShareInfoCard from "./ShareInfoCard.vue";
import { useLanShare } from "../../composables/useLanShare";
import type { AdminStatus } from "../../composables/useAdmin";

// ─── LAN 分享（QR + 复制 URL + 连接信息 + PIN 鉴权）──
// 数据/拉取/操作全部集中在 useLanShare；本组件只做模板组合，
// 鉴权卡片（LanAuthPanel）与连接信息卡（ShareInfoCard）为纯展示子组件。

const props = defineProps<{
  status: AdminStatus | null;
  loading: boolean;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
}>();

// The Tauri-side status carries the gateway base URL; the admin frontend runs
// on a different origin (tauri.localhost / vite dev server), so /api/* must be
// reached through this absolute base.
const gatewayBase = computed(() => {
  const base = props.status?.broker_http_url ?? "";
  return base.endsWith("/") ? base : base ? `${base}/` : "";
});

const {
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
  workUrl,
  workQrSvg,
  connections,
  mdns,
  handleRefresh,
  copyUrl,
  authEnabled,
  pinSet,
  pin,
  pinVisible,
  devices,
  authBusy,
  authError,
  pinCopied,
  toggleAuth,
  regeneratePin,
  revokeDevice,
  revokeAll,
  copyPin,
  wsBaseDir,
  wsBaseDirBusy,
  wsBaseDirError,
  setWsBaseDir,
} = useLanShare(gatewayBase, () => emit("refresh"));

// ── 工作空间存储：基目录配置 + 迁移（0.3.0）──
const wsDirInput = ref("");
const wsDirTouched = ref(false);
watch(wsBaseDir, (info) => {
  if (info && !wsDirTouched.value) {
    wsDirInput.value = info.configured || info.baseDir || "";
  }
});
async function saveWsBaseDir() {
  const ok = await setWsBaseDir(wsDirInput.value.trim());
  if (ok) wsDirTouched.value = true;
}
async function resetWsBaseDir() {
  const ok = await setWsBaseDir("");
  if (ok) {
    wsDirTouched.value = true;
    wsDirInput.value = "";
  }
}

// ── Work 卡片：复制 work URL（与 chat 的 copyUrl 分开）──
const workCopied = ref(false);
function copyWorkUrl() {
  if (!workUrl.value) return;
  const done = () => {
    workCopied.value = true;
    setTimeout(() => (workCopied.value = false), 2000);
  };
  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(workUrl.value).then(done).catch(() => fallbackCopyText(done));
  } else {
    fallbackCopyText(done);
  }
}

function fallbackCopyText(done: () => void) {
  const ta = document.createElement("textarea");
  ta.value = workUrl.value;
  ta.style.cssText = "position:fixed;left:-9999px";
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  document.body.removeChild(ta);
  done();
}

function fmtClientTime(ms: number): string {
  const d = new Date(ms);
  return Number.isFinite(d.getTime()) ? d.toLocaleTimeString() : "—";
}
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

        <!-- Work share: QR + URL + service discovery (0.3.0) -->
        <div class="share-card">
          <div class="share-card-header">
            <span class="share-card-title">
              <MonitorSmartphone :size="14" />
              {{ $t("admin.workShare") }}
            </span>
            <span class="share-card-desc">{{ $t("admin.workShareDesc") }}</span>
          </div>

          <div v-if="workQrSvg" class="qr-frame">
            <div class="qr-svg" v-html="workQrSvg" />
          </div>
          <div v-else class="qr-placeholder">
            <RefreshCw :size="18" class="spin" />
          </div>

          <div v-if="workUrl" class="url-row">
            <code class="url-text">{{ workUrl }}</code>
            <button
              class="btn btn-ghost btn-icon btn-sm"
              :title="workCopied ? $t('common.copied') : $t('admin.copyWorkUrlHint')"
              @click="copyWorkUrl"
            >
              <Check v-if="workCopied" :size="14" />
              <Copy v-else :size="14" />
            </button>
          </div>
          <div v-else class="share-error">{{ $t("admin.workUnavailable") }}</div>

          <!-- 服务发现信息（供移动端 App mDNS 解析接入） -->
          <div class="info-block">
            <div class="info-row">
              <span class="info-key">{{ $t("admin.serviceDiscovery") }}</span>
              <span class="info-value">
                <span v-if="mdns?.enabled" class="badge badge-success">
                  {{ mdns?.serviceType }} · {{ mdns?.instanceName }} · :{{ mdns?.port }} · v{{ mdns?.proto }}
                </span>
                <span v-else class="badge badge-muted">{{ $t("admin.mdnsDisabled") }}</span>
              </span>
            </div>
            <div v-if="authEnabled" class="info-row">
              <span class="info-key">PIN</span>
              <span class="info-value pin-hint">{{ $t("admin.pinRequiredHint") }}</span>
            </div>
          </div>
        </div>

        <!-- Connection info: broker endpoints + health -->
        <ShareInfoCard :lan-info="lanInfo" :health="health" :online="online" :gateway-port="gatewayPort" />
      </div>

      <!-- LAN access PIN gate -->
      <LanAuthPanel
        v-model:pin-visible="pinVisible"
        :enabled="authEnabled"
        :pin-set="pinSet"
        :pin="pin"
        :devices="devices"
        :busy="authBusy"
        :error="authError"
        :pin-copied="pinCopied"
        @toggle="toggleAuth"
        @regenerate="regeneratePin"
        @revoke="revokeDevice"
        @revoke-all="revokeAll"
        @copy-pin="copyPin"
      />

      <!-- Connected clients (0.3.0): 5s 轮询 /api/connections 刷新 -->
      <div class="section">
        <div class="clients-header">
          <h3 class="tab-title clients-title">
            <Users :size="13" />
            {{ $t("admin.connectedClients") }}
          </h3>
        </div>
        <p class="tab-desc section-desc">{{ $t("admin.connectedClientsDesc") }}</p>

        <div v-if="connections.length === 0" class="clients-empty">{{ $t("admin.noClients") }}</div>
        <table v-else class="clients-table">
          <thead>
            <tr>
              <th>{{ $t("admin.clientKind") }}</th>
              <th>{{ $t("admin.clientForm") }}</th>
              <th>{{ $t("admin.clientIp") }}</th>
              <th>{{ $t("admin.clientUserAgent") }}</th>
              <th>{{ $t("admin.clientConnectedAt") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="c in connections" :key="c.id">
              <td>
                <span class="badge" :class="c.kind === 'work' ? 'badge-accent' : 'badge-muted'">
                  {{ c.kind === "work" ? $t("admin.kindWork") : c.kind === "chat" ? $t("admin.kindChat") : $t("admin.kindUi") }}
                </span>
              </td>
              <td>{{ c.form === "app" ? $t("admin.formApp") : $t("admin.formWeb") }}</td>
              <td class="mono">{{ c.ip }}</td>
              <td class="mono ua-cell" :title="c.userAgent">{{ c.userAgent || "—" }}</td>
              <td>{{ fmtClientTime(c.connectedAtMs) }}</td>
            </tr>
          </tbody>
        </table>
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

      <!-- Workspace storage: base dir config + migration (0.3.0) -->
      <div class="section">
        <h3 class="tab-title">
          <FolderOpen :size="13" />
          {{ $t("admin.wsBaseDirTitle") }}
        </h3>
        <p class="tab-desc section-desc">{{ $t("admin.wsBaseDirDesc") }}</p>

        <div v-if="wsBaseDir" class="ws-dir-card">
          <div class="info-block">
            <div class="info-row">
              <span class="info-key">{{ $t("admin.wsBaseDirCurrent") }}</span>
              <code class="mono info-value">{{ wsBaseDir.baseDir }}</code>
            </div>
            <div class="info-row">
              <span class="info-key">{{ $t("admin.wsBaseDirDefault") }}</span>
              <code class="mono info-value">{{ wsBaseDir.defaultBaseDir }}</code>
            </div>
            <div class="info-row">
              <span class="info-key">{{ $t("admin.wsBaseDirWritable") }}</span>
              <span class="badge" :class="wsBaseDir.writable ? 'badge-success' : 'badge-error'">
                {{ wsBaseDir.writable ? $t("admin.wsBaseDirWritableYes") : $t("admin.wsBaseDirWritableNo") }}
              </span>
            </div>
          </div>

          <div class="ws-dir-edit">
            <input
              v-model="wsDirInput"
              class="input"
              :placeholder="wsBaseDir.defaultBaseDir"
              @input="wsDirTouched = true"
            />
            <button class="btn btn-sm" :disabled="wsBaseDirBusy" @click="saveWsBaseDir">
              {{ wsBaseDirBusy ? $t("common.saving") : $t("admin.wsBaseDirSave") }}
            </button>
            <button v-if="wsBaseDir.configured" class="btn btn-ghost btn-sm" @click="resetWsBaseDir">
              {{ $t("admin.wsBaseDirReset") }}
            </button>
          </div>
          <p class="ws-dir-hint">{{ $t("admin.wsBaseDirHint") }}</p>
          <div v-if="wsBaseDirError" class="share-error">{{ wsBaseDirError }}</div>

          <div v-if="wsBaseDir.migration.migrating" class="ws-dir-migrating">
            <RefreshCw :size="12" class="spin" /> {{ $t("admin.wsBaseDirMigrating") }}
          </div>
          <div v-if="wsBaseDir.migration.pending.length" class="ws-dir-pending">
            <div v-for="p in wsBaseDir.migration.pending" :key="p.id" class="info-row">
              <span class="info-key">{{ p.id }}</span>
              <span class="mono info-value">
                {{ p.oldPath }} → {{ p.newPath }}
                <span v-if="p.waiting" class="badge badge-warning">{{ $t("admin.wsBaseDirWaiting") }}</span>
              </span>
            </div>
          </div>
          <div v-if="wsBaseDir.migration.errors.length" class="ws-dir-errors">
            <div v-for="(e, i) in wsBaseDir.migration.errors" :key="i" class="share-error">
              {{ e.id }}: {{ e.error }}
            </div>
          </div>
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

/* ── 0.3.0：Work 卡片服务发现 + 连接客户端列表 ── */
.pin-hint {
  color: var(--text-tertiary);
  font-family: var(--font-sans);
}

.clients-header {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.clients-title {
  display: flex;
  align-items: center;
  gap: 6px;
}

.clients-empty {
  padding: var(--space-lg);
  text-align: center;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  background: var(--bg-muted);
  border-radius: var(--radius-md);
}

.clients-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--font-size-caption);
}

.clients-table th,
.clients-table td {
  text-align: left;
  padding: var(--space-xs) var(--space-sm);
  border-bottom: 1px solid var(--border);
  color: var(--text-secondary);
  vertical-align: middle;
}

.clients-table th {
  font-weight: 600;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.clients-table .mono {
  font-family: var(--font-mono);
  word-break: break-all;
}

.clients-table .ua-cell {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ── 0.3.0：工作空间存储（基目录配置 + 迁移）── */
.ws-dir-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.ws-dir-edit {
  display: flex;
  gap: var(--space-sm);
  align-items: center;
  flex-wrap: wrap;
}

.ws-dir-edit .input {
  flex: 1 1 320px;
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  padding: var(--space-xs) var(--space-sm);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-elevated);
  color: var(--text-primary);
}

.ws-dir-hint {
  margin: 0;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.ws-dir-migrating {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-caption);
  color: var(--accent-strong);
}

.ws-dir-pending {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.ws-dir-errors {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

@media (max-width: 900px) {
  .share-grid {
    grid-template-columns: 1fr;
  }
  .guide-grid {
    grid-template-columns: 1fr;
  }
  .clients-table .ua-cell {
    display: none;
  }
}
</style>
