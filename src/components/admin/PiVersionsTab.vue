<script setup lang="ts">
import { toRefs } from "vue";
import { Loader2 } from "lucide-vue-next";
import PiVersionInstallCard from "./PiVersionInstallCard.vue";
import { usePiVersions } from "../../composables/usePiVersions";
import type { PiInstallInfo, DownloadProgressEvent } from "../../composables/useAdmin";

// ─── Pi 运行时 Tab ──────────────────────────────────────────────────────
// 安装/下载/进度逻辑在 usePiVersions；安装卡片展示在 PiVersionInstallCard。
// 本组件只做模板组装（状态 + 卡片 + 帮助 + 反馈）。

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

const { installInfo, downloadProgress, loading, downloading, uninstalling } = toRefs(props);

const {
  downloadInput,
  actionFeedback,
  networkHint,
  openReleases,
  openHomepage,
  handleDownload,
  handleUninstall,
  busy,
  progressPercent,
  progressText,
} = usePiVersions({
  installInfo,
  downloadProgress,
  downloading,
  uninstalling,
  download: props.download,
  onUninstall: () => emit("uninstall"),
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

    <PiVersionInstallCard
      v-else-if="installInfo"
      :install-info="installInfo"
      :download-progress="downloadProgress"
      :downloading="downloading"
      :uninstalling="uninstalling"
      :loading="loading"
      v-model:download-input="downloadInput"
      :network-hint="networkHint"
      :progress-percent="progressPercent"
      :progress-text="progressText"
      :busy="busy"
      :handle-download="handleDownload"
      :handle-uninstall="handleUninstall"
      :open-releases="openReleases"
      @refresh="emit('refresh')"
    />

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
              <a class="text-link" href="#" @click.prevent="openHomepage">{{ $t("admin.piDevLink") }}</a>
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
