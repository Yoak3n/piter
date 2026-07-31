<script setup lang="ts">
import { ref } from "vue";
import { Download, Check, RotateCcw, Trash2, Loader2, HardDrive, Link } from "lucide-vue-next";
import type { PiInstallInfo } from "../../composables/useAdmin";

const props = defineProps<{
  installInfo: PiInstallInfo | null;
  loading: boolean;
  downloading: boolean;
  uninstalling: boolean;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
  (e: "download", version: string): void;
  (e: "uninstall"): void;
}>();

const downloadInput = ref("");
const actionFeedback = ref("");

async function handleDownload() {
  const v = downloadInput.value.trim();
  if (!v) return;
  actionFeedback.value = `Downloading ${v}...`;
  emit("download", v);
  downloadInput.value = "";
  setTimeout(() => { actionFeedback.value = ""; }, 5000);
}

async function handleUninstall() {
  actionFeedback.value = "Uninstalling...";
  emit("uninstall");
  setTimeout(() => { actionFeedback.value = ""; }, 3000);
}

const busy = () => props.downloading || props.uninstalling;
</script>

<template>
  <div class="tab-content">
    <h3 class="tab-title">Pi Runtime</h3>
    <p class="tab-desc">Manage the Pi runtime installed in resources/pi/. Piter only manages this directory.</p>

    <!-- Current install status -->
    <div v-if="loading" class="loading-state">
      <Loader2 :size="14" class="spin" />
      <span>Checking installation...</span>
    </div>

    <template v-else-if="installInfo">
      <div class="install-card" :class="{ 'install-card--installed': installInfo.binary_present }">
        <div class="install-card-header">
          <div class="install-card-title">
            <template v-if="installInfo.binary_present">
              <Check :size="16" />
              <span>Pi is installed</span>
            </template>
            <template v-else>
              <HardDrive :size="16" />
              <span>Pi is not installed</span>
            </template>
          </div>
        </div>

        <div class="install-card-body" v-if="installInfo.binary_present">
          <div class="info-grid">
            <div class="info-item">
              <span class="info-label">Version</span>
              <span class="info-value mono">{{ installInfo.version ?? "unknown" }}</span>
            </div>
            <div class="info-item">
              <span class="info-label">Origin</span>
              <span class="info-value">
                <span class="badge" :class="installInfo.origin === 'downloaded' ? 'badge-success' : 'badge-accent'">
                  <Link v-if="installInfo.origin === 'linked'" :size="10" />
                  {{ installInfo.origin === "downloaded" ? "Downloaded by Piter" : "Linked from external" }}
                </span>
              </span>
            </div>
            <div class="info-item">
              <span class="info-label">Pinned version</span>
              <span class="info-value mono">v{{ installInfo.locked_version }}</span>
            </div>
          </div>
        </div>

        <div class="install-card-body" v-else>
          <p class="install-hint">
            Download Pi to enable runtime features. Pinned version: <code>v{{ installInfo.locked_version }}</code>
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
                :placeholder="`e.g. ${installInfo.locked_version}`"
                :disabled="busy()"
                @keydown.enter="handleDownload"
              />
              <button class="btn" :disabled="busy() || !downloadInput.trim()" @click="handleDownload">
                <RotateCcw :size="12" />
                <span>Switch Version</span>
              </button>
            </div>
            <div class="footer-actions">
              <button class="btn btn-danger btn-sm" :disabled="busy()" @click="handleUninstall">
                <Trash2 :size="12" />
                <span>Uninstall Pi</span>
              </button>
              <span class="origin-hint" v-if="installInfo.origin === 'linked'">
                Uninstall only removes the link, not the original install.
              </span>
            </div>
          </template>
          <template v-else>
            <div class="download-row">
              <input
                class="input download-input"
                type="text"
                v-model="downloadInput"
                placeholder="e.g. 0.80.3"
                :disabled="busy()"
                @keydown.enter="handleDownload"
              />
              <button class="btn btn-primary" :disabled="busy() || !downloadInput.trim()" @click="handleDownload">
                <Download v-if="!downloading" :size="14" />
                <Loader2 v-else :size="14" class="spin" />
                <span>{{ downloading ? "Downloading..." : "Download" }}</span>
              </button>
            </div>
            <button class="btn btn-ghost btn-sm" :disabled="loading" @click="emit('refresh')" style="margin-top: 8px;">
              <span>Refresh</span>
            </button>
          </template>
        </div>
      </div>
    </template>

    <div v-if="actionFeedback" class="action-feedback">{{ actionFeedback }}</div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  max-width: 560px;
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

.action-feedback {
  margin-top: var(--space-md);
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
}
</style>
