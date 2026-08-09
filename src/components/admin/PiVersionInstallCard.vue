<script setup lang="ts">
import { Download, Check, RotateCcw, Trash2, Loader2, HardDrive, Link, Globe } from "lucide-vue-next";
import type { PiInstallInfo, DownloadProgressEvent } from "../../composables/useAdmin";

// ─── Pi 运行时安装卡片（PiVersionsTab 子组件）───
// 纯展示：安装状态/版本信息/下载切换/进度条。
// 输入与动作逻辑在 usePiVersions（父级持有），经 props 传入；仅 refresh 事件在此发。

defineProps<{
  installInfo: PiInstallInfo;
  downloadProgress: DownloadProgressEvent | null;
  downloading: boolean;
  uninstalling: boolean;
  loading: boolean;
  downloadInput: string;
  networkHint: boolean;
  progressPercent: number;
  progressText: string;
  busy: () => boolean;
  handleDownload: () => void;
  handleUninstall: () => void;
  openReleases: () => void;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
  (e: "update:downloadInput", value: string): void;
}>();
</script>

<template>
  <div class="install-card" :class="{ 'install-card--installed': installInfo.binary_present }">
    <div v-if="networkHint" class="proxy-note">
      <Globe :size="14" class="proxy-note-icon" />
      <i18n-t keypath="admin.versionsNetworkNote" tag="span">
        <template #https><code>HTTPS_PROXY</code></template>
        <template #http><code>HTTP_PROXY</code></template>
      </i18n-t>
    </div>

    <div class="install-card-header">
      <div class="install-card-title">
        <template v-if="installInfo.binary_present">
          <Check :size="16" />
          <span>{{ $t("admin.piInstalled") }}</span>
        </template>
        <template v-else>
          <HardDrive :size="16" />
          <span>{{ $t("admin.piNotInstalled") }}</span>
        </template>
      </div>
    </div>

    <div class="install-card-body" v-if="installInfo.binary_present">
      <div class="info-grid">
        <div class="info-item">
          <span class="info-label">{{ $t("admin.infoVersion") }}</span>
          <span class="info-value mono">{{ installInfo.version ?? $t("admin.unknown") }}</span>
        </div>
        <div class="info-item">
          <span class="info-label">{{ $t("admin.infoOrigin") }}</span>
          <span class="info-value">
            <span class="badge" :class="installInfo.origin === 'downloaded' ? 'badge-success' : 'badge-accent'">
              <Link v-if="installInfo.origin === 'linked'" :size="10" />
              {{ installInfo.origin === "downloaded" ? $t("admin.originDownloaded") : $t("admin.originLinked") }}
            </span>
          </span>
        </div>
        <div class="info-item">
          <span class="info-label">{{ $t("admin.pinnedVersion") }}</span>
          <span class="info-value mono">v{{ installInfo.locked_version }}</span>
        </div>
      </div>
    </div>

    <div class="install-card-body" v-else>
      <p class="install-hint">
        <i18n-t keypath="admin.installHint" tag="span">
          <template #link>
            <a class="text-link" href="#" @click.prevent="openReleases">{{ $t("admin.releasesGithub") }}</a>
          </template>
        </i18n-t>
      </p>
    </div>

    <div class="install-card-footer">
      <template v-if="installInfo.binary_present">
        <!-- Download a different version (replaces current) -->
        <div class="switch-row">
          <input
            class="input switch-input"
            type="text"
            :value="downloadInput"
            :placeholder="$t('admin.pinPlaceholder', { v: installInfo.locked_version })"
            :disabled="busy()"
            @input="emit('update:downloadInput', ($event.target as HTMLInputElement).value)"
            @keydown.enter="handleDownload"
          />
          <button class="btn" :disabled="busy() || !downloadInput.trim()" @click="handleDownload">
            <RotateCcw :size="12" />
            <span>{{ $t("admin.switchVersion") }}</span>
          </button>
        </div>
        <div class="footer-actions">
          <button class="btn btn-danger btn-sm" :disabled="busy()" @click="handleUninstall">
            <Trash2 :size="12" />
            <span>{{ $t("admin.uninstallPi") }}</span>
          </button>
          <span class="origin-hint" v-if="installInfo.origin === 'linked'">
            {{ $t("admin.uninstallLinkHint") }}
          </span>
        </div>
      </template>
      <template v-else>
        <div class="download-row">
          <input
            class="input download-input"
            type="text"
            :value="downloadInput"
            :placeholder="$t('admin.versionPlaceholder')"
            :disabled="busy()"
            @input="emit('update:downloadInput', ($event.target as HTMLInputElement).value)"
            @keydown.enter="handleDownload"
          />
          <button class="btn btn-primary" :disabled="busy() || !downloadInput.trim()" @click="handleDownload">
            <Download v-if="!downloading" :size="14" />
            <Loader2 v-else :size="14" class="spin" />
            <span>{{ downloading ? $t("admin.downloading") : $t("admin.download") }}</span>
          </button>
        </div>
        <button class="btn btn-ghost btn-sm" :disabled="loading" @click="emit('refresh')" style="margin-top: 8px;">
          <span>{{ $t("admin.refresh") }}</span>
        </button>
      </template>
    </div>

    <div v-if="downloading" class="progress-wrap">
      <div class="progress-track">
        <div class="progress-fill" :style="{ width: progressPercent + '%' }"></div>
      </div>
      <span class="progress-label">{{ progressText }}</span>
    </div>
  </div>
</template>

<style scoped>
.install-card {
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  overflow: hidden;
}

.proxy-note {
  display: flex;
  align-items: flex-start;
  gap: var(--space-sm);
  padding: var(--space-sm) var(--space-lg);
  background: var(--warning-soft);
  color: var(--warning);
  font-size: var(--font-size-caption);
  line-height: var(--line-height-caption);
  border-bottom: 1px solid var(--border);
}

.proxy-note-icon {
  flex-shrink: 0;
  margin-top: 1px;
}

.proxy-note code {
  font-family: var(--font-mono);
  background: transparent;
  color: inherit;
}

.install-card-header {
  padding: var(--space-md) var(--space-lg);
  border-bottom: 1px solid var(--border);
}

.install-card-title {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: var(--font-size-base);
  font-weight: 500;
  color: var(--text);
}

.install-card--installed .install-card-title {
  color: var(--success);
}

.install-card-body {
  padding: var(--space-lg);
}

.install-card-footer {
  padding: var(--space-md) var(--space-lg);
  border-top: 1px solid var(--border);
  background: var(--bg-panel);
}

.info-grid {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.info-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.info-label {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.info-value {
  font-size: var(--font-size-body);
  color: var(--text);
}

.info-value.mono,
.mono {
  font-family: var(--font-mono);
}

.install-hint {
  margin: 0;
  font-size: var(--font-size-body);
  color: var(--text-secondary);
}
.install-hint code {
  font-family: var(--font-mono);
  color: var(--text);
  background: transparent;
}

.switch-row {
  display: flex;
  gap: var(--space-sm);
  align-items: center;
}

.switch-input {
  flex: 1;
  font-family: var(--font-mono);
}

.footer-actions {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  margin-top: var(--space-md);
}

.origin-hint {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.download-row {
  display: flex;
  gap: var(--space-sm);
  align-items: center;
}

.download-input {
  flex: 1;
  font-family: var(--font-mono);
}

.progress-wrap {
  margin-top: var(--space-md);
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.progress-track {
  flex: 1;
  height: 6px;
  border-radius: 3px;
  background: var(--bg-muted);
  border: 1px solid var(--border);
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 3px;
  transition: width 0.2s ease;
}

.progress-label {
  font-size: var(--font-size-caption);
  font-family: var(--font-mono);
  color: var(--text-secondary);
  white-space: nowrap;
}

.text-link {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  color: var(--accent);
  text-decoration: none;
  cursor: pointer;
}
.text-link:hover {
  text-decoration: underline;
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
