<script setup lang="ts">
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import { X, Download, RefreshCw } from "lucide-vue-next";
import { useAdmin, type UpdateCheckInfo } from "../../composables/useAdmin";

const { t } = useI18n();
const { installUpdate } = useAdmin();

defineProps<{
  info: UpdateCheckInfo;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

type Phase = "idle" | "downloading" | "error";
const phase = ref<Phase>("idle");
const progress = ref(0); // 0..100; -1 when total is unknown
const errorMsg = ref("");

const progressText = computed(() => {
  if (progress.value < 0) return t("common.loading");
  return t("admin.downloadProgress", { pct: progress.value });
});

async function handleInstall() {
  if (phase.value === "downloading") return;
  phase.value = "downloading";
  progress.value = 0;
  errorMsg.value = "";
  const ok = await installUpdate((p) => {
    if (p.total && p.total > 0) {
      progress.value = Math.min(100, Math.round((p.downloaded / p.total) * 100));
    } else {
      progress.value = -1;
    }
  });
  if (!ok) {
    phase.value = "error";
    // The app relaunches on success, so reaching here always means failure.
  }
}
</script>

<template>
  <div class="update-overlay" @click.self="emit('close')">
    <div class="update-modal">
      <header class="update-modal-header">
        <span class="update-modal-title">{{ $t("admin.updateAvailableTitle") }}</span>
        <button class="btn-close" :disabled="phase === 'downloading'" @click="emit('close')">
          <X :size="14" />
        </button>
      </header>

      <div class="update-modal-body">
        <p class="update-desc">{{ $t("admin.updateAvailableDesc") }}</p>

        <div class="version-row">
          <div class="version-col">
            <span class="version-label">{{ $t("admin.currentVersion") }}</span>
            <code class="version-value">{{ info.current_version }}</code>
          </div>
          <span class="version-arrow">→</span>
          <div class="version-col">
            <span class="version-label">{{ $t("admin.latestVersion") }}</span>
            <code class="version-value version-latest">{{ info.latest_version }}</code>
          </div>
        </div>

        <div v-if="info.notes" class="notes-block">
          <span class="notes-title">{{ $t("admin.releaseNotes") }}</span>
          <pre class="notes-body">{{ info.notes }}</pre>
        </div>

        <div v-if="phase === 'downloading'" class="download-block">
          <div class="progress-track">
            <div class="progress-fill" :style="{ width: progress >= 0 ? `${progress}%` : '100%' }" />
          </div>
          <span class="download-hint">
            <RefreshCw :size="11" class="spin" />
            {{ progressText }}
          </span>
        </div>

        <div v-if="phase === 'error'" class="update-error">{{ errorMsg || $t("admin.updateInstallFailed", { msg: "" }) }}</div>
      </div>

      <footer class="update-modal-footer">
        <button class="btn" :disabled="phase === 'downloading'" @click="emit('close')">
          {{ $t("admin.updateLater") }}
        </button>
        <button
          v-if="phase !== 'downloading'"
          class="btn btn-primary"
          @click="handleInstall"
        >
          <Download :size="13" />
          {{ $t("admin.downloadAndInstall") }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.update-overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay-backdrop);
}

.update-modal {
  width: 400px;
  max-width: calc(100vw - 32px);
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-modal);
  padding: var(--space-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.update-modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.update-modal-title {
  font-size: var(--font-size-body);
  font-weight: 600;
  color: var(--text);
}

.btn-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: none;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: var(--radius-sm);
}
.btn-close:hover {
  background: var(--bg-hover);
}
.btn-close:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.update-modal-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.update-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
}

.version-row {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-md);
}

.version-col {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}

.version-label {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

.version-value {
  font-family: var(--font-mono);
  font-size: var(--font-size-body);
  color: var(--text-secondary);
}

.version-latest {
  color: var(--accent-strong);
  font-weight: 600;
}

.version-arrow {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.notes-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.notes-title {
  font-size: var(--font-size-caption);
  font-weight: 500;
  color: var(--text-secondary);
}

.notes-body {
  margin: 0;
  font-size: var(--font-size-caption);
  font-family: var(--font);
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 160px;
  overflow-y: auto;
  background: var(--bg-muted);
  border-radius: var(--radius-sm);
  padding: var(--space-sm) var(--space-md);
  line-height: 1.6;
}

.download-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.progress-track {
  height: 6px;
  border-radius: var(--radius-pill);
  background: var(--bg-muted);
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  border-radius: var(--radius-pill);
  background: var(--accent);
  transition: width 0.2s var(--ease);
}

.download-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

.update-error {
  font-size: var(--font-size-caption);
  color: var(--danger);
  background: var(--danger-soft);
  border-radius: var(--radius-sm);
  padding: var(--space-sm) var(--space-md);
}

.update-modal-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-sm);
}
</style>
