<script setup lang="ts">
import { computed } from "vue";
import { RefreshCw, Copy, Check, QrCode, WifiOff, Globe } from "lucide-vue-next";
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
} = useLanShare(gatewayBase, () => emit("refresh"));
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
