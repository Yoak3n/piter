<script setup lang="ts">
import { ref, watch, computed } from "vue";
import { useI18n } from "vue-i18n";
import { Download, Check, RotateCcw, Trash2, Loader2, HardDrive, Link, Globe } from "lucide-vue-next";
import { invoke } from "@tauri-apps/api/core";
import type { PiInstallInfo, DownloadProgressEvent } from "../../composables/useAdmin";

const PI_RELEASES_URL = "https://github.com/earendil-works/pi/releases";
const PI_HOMEPAGE_URL = "https://pi.dev";

const { t } = useI18n();

const props = defineProps<{
  installInfo: PiInstallInfo | null;
  downloadProgress: DownloadProgressEvent | null;
  loading: boolean;
  downloading: boolean;
  uninstalling: boolean;
  download: (version: string) => Promise<boolean>;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
  (e: "uninstall"): void;
}>();

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

// Pre-fill the download input once pi is installed. Prefer the currently
// installed version (e.g. right after a user-triggered download) and fall
// back to the pinned version — otherwise the field would jump back to the
// pinned version after the user downloads a different one.
watch(
  () => props.installInfo,
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
  const ok = await props.download(v);
  if (!ok) networkHint.value = true;
}

async function handleUninstall() {
  actionFeedback.value = t("admin.feedbackUninstalling");
  emit("uninstall");
  setTimeout(() => { actionFeedback.value = ""; }, 3000);
}

const busy = () => props.downloading || props.uninstalling;

// ─── Download progress helpers ────────────────────────────────────────────────

const progressPercent = computed(() => {
  const p = props.downloadProgress;
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
  const p = props.downloadProgress;
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
</script>

<template>
  <div class="tab-content">
    <h3 class="tab-title">{{ $t("admin.piRuntime") }}</h3>
    <p class="tab-desc">{{ $t("admin.piRuntimeDesc") }}</p>

    <!-- Current install status -->
    <div v-if="loading" class="loading-state">
      <Loader2 :size="14" class="spin" />
      <span>{{ $t("admin.checkingInstall") }}</span>
    </div>

    <template v-else-if="installInfo">
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
                v-model="downloadInput"
                :placeholder="$t('admin.pinPlaceholder', { v: installInfo.locked_version })"
                :disabled="busy()"
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
                v-model="downloadInput"
                :placeholder="$t('admin.versionPlaceholder')"
                :disabled="busy()"
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

    <div class="help-card">
      <h4 class="help-title">{{ $t("admin.helpTitle") }}</h4>
      <ul class="help-list">
        <li>
          <i18n-t keypath="admin.tipFindVersion" tag="span">
            <template #link>
              <a class="text-link" href="#" @click.prevent="openReleases">{{ $t("admin.releasesLink") }}</a>
            </template>
          </i18n-t>
        </li>
        <li>
          <i18n-t keypath="admin.tipManualInstall" tag="span">
            <template #link>
              <a class="text-link" href="#" @click.prevent="openLink(PI_HOMEPAGE_URL)">{{ $t("admin.piDevLink") }}</a>
            </template>
            <template #code><code>npm i -g @earendil-works/pi-coding-agent</code></template>
          </i18n-t>
        </li>
      </ul>
    </div>

    <div v-if="actionFeedback" class="action-feedback">{{ actionFeedback }}</div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0 0 var(--space-xs) 0;
}

.tab-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0 0 var(--space-lg) 0;
  line-height: var(--line-height-caption);
}

.loading-state {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  color: var(--text-tertiary);
  font-size: var(--font-size-body);
  padding: var(--space-xl) 0;
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

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

.action-feedback {
  margin-top: var(--space-md);
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
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

.help-card {
  margin-top: var(--space-lg);
  background: var(--bg-panel);
  border: 1px dashed var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-md) var(--space-lg);
}

.help-title {
  margin: 0 0 var(--space-sm) 0;
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.help-list {
  margin: 0;
  padding-left: var(--space-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
  font-size: var(--font-size-caption);
  line-height: var(--line-height-caption);
  color: var(--text-secondary);
}
.help-list strong {
  color: var(--text);
  font-weight: 600;
}
.help-list code {
  font-family: var(--font-mono);
  color: var(--text);
  background: transparent;
}
</style>
