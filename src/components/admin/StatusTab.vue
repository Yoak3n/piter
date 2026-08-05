<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw } from "lucide-vue-next";
import type { AdminStatus, PiSettings } from "../../composables/useAdmin";

const { t } = useI18n();

const props = defineProps<{
  status: AdminStatus | null;
  loading: boolean;
  piSettings: PiSettings;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
  (e: "restart-pi"): void;
  (e: "stop-pi"): void;
  (e: "open-path", path: string): void;
  (e: "update-pi-settings", settings: PiSettings): void;
}>();

const actionLabel = ref("");

// Editable copy of Piter's Pi process-management settings.
const localPiSettings = ref<PiSettings>({ ...props.piSettings });
const piSettingsSaved = ref(false);
watch(() => props.piSettings, (s) => {
  localPiSettings.value = { ...s };
}, { immediate: true });

function handleSavePiSettings() {
  piSettingsSaved.value = false;
  emit("update-pi-settings", { ...localPiSettings.value });
  piSettingsSaved.value = true;
  setTimeout(() => (piSettingsSaved.value = false), 2000);
}

// Track when we last fetched uptime, so we can add elapsed seconds locally
let refreshTime = Date.now();
let baseUptime = 0;
const now = ref(Date.now());

// Sync baseUptime when status changes (new fetch)
watch(() => props.status?.uptime_secs, (u) => {
  if (u !== undefined) {
    baseUptime = u;
    refreshTime = Date.now();
  }
}, { immediate: true });

const liveUptime = computed(() => {
  if (!props.status) return 0;
  return baseUptime + Math.floor((now.value - refreshTime) / 1000);
});

function fmtUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function handleRestart() {
  actionLabel.value = t("common.restarting");
  emit("restart-pi");
  setTimeout(() => (actionLabel.value = ""), 3000);
}

function handleStop() {
  actionLabel.value = t("common.stopping");
  emit("stop-pi");
  setTimeout(() => (actionLabel.value = ""), 3000);
}

let timer: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  emit("refresh");
  timer = setInterval(() => { now.value = Date.now(); }, 1000);
});
onUnmounted(() => { if (timer) clearInterval(timer); });
</script>

<template>
  <div class="tab-content">
    <div class="tab-header">
      <div class="tab-header-info">
        <h3 class="tab-title">{{ $t("admin.systemStatus") }}</h3>
        <p class="tab-desc">{{ $t("admin.statusDesc") }}</p>
      </div>
      <button class="btn btn-sm" :disabled="loading" @click="emit('refresh')">
        <RefreshCw :size="12" :class="{ 'spin': loading }" />
        {{ loading ? $t("common.refreshing") : $t("admin.refresh") }}
      </button>
    </div>

    <div class="status-grid">
      <div class="status-card">
        <div class="status-card-label">{{ $t("admin.piStatus") }}</div>
        <div class="status-card-value">
          <span
            v-if="status"
            class="badge"
            :class="status.pi_running ? 'badge-success' : 'badge-muted'"
          >
            {{ status.pi_running ? $t("admin.running") : $t("admin.stopped") }}
          </span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">{{ $t("admin.uptime") }}</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ fmtUptime(liveUptime) }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">{{ $t("admin.appVersion") }}</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ status.app_version }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">{{ $t("admin.piVersion") }}</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ status.pi_version }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card status-card-wide clickable-card" @click="status && emit('open-path', status.data_dir)">
        <div class="status-card-label">{{ $t("admin.dataDir") }}</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-path">{{ status.data_dir }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card status-card-wide">
        <div class="status-card-label">{{ $t("admin.activeSessions", { n: status?.active_sessions.length ?? 0 }) }}</div>
        <div v-if="status && status.active_sessions.length" class="session-list">
          <div v-for="s in status.active_sessions" :key="s.instance_id" class="session-item">
            <span class="status-card-mono">{{ s.instance_id.slice(0, 8) }}</span>
            <span class="badge" :class="s.state === 'running' ? 'badge-success' : 'badge-muted'">{{ s.state }}</span>
            <span class="status-card-path">{{ s.cwd }}</span>
          </div>
        </div>
        <div v-else class="status-card-muted">{{ $t("admin.noActiveSessions") }}</div>
      </div>
    </div>

    <div class="section">
      <h3 class="tab-title">{{ $t("admin.piControls") }}</h3>
      <div class="control-row">
        <button class="btn" @click="handleRestart">{{ $t("admin.restartPi") }}</button>
        <button class="btn btn-danger" @click="handleStop">{{ $t("admin.stopPi") }}</button>
        <span v-if="actionLabel" class="action-feedback">{{ actionLabel }}</span>
      </div>
    </div>

    <div class="section">
      <h3 class="tab-title">{{ $t("admin.piterSettings") }}</h3>
      <div class="settings-card">
        <div class="settings-row">
          <div class="settings-label">
            <span class="settings-label-title">{{ $t("admin.requestTimeout") }}</span>
            <span class="settings-label-desc">{{ $t("admin.requestTimeoutDesc") }}</span>
          </div>
          <input class="input number-input" type="number" v-model.number="localPiSettings.request_timeout_secs" min="30" max="3600" :disabled="disabled" />
        </div>

        <div class="settings-row">
          <div class="settings-label">
            <span class="settings-label-title">{{ $t("admin.autoRestart") }}</span>
            <span class="settings-label-desc">{{ $t("admin.autoRestartDesc") }}</span>
          </div>
          <label class="toggle" :class="{ on: localPiSettings.auto_restart_on_crash }">
            <input type="checkbox" v-model="localPiSettings.auto_restart_on_crash" :disabled="disabled" />
            <span class="toggle-track"></span>
          </label>
        </div>

        <div class="settings-footer">
          <button class="btn btn-primary" :disabled="disabled" @click="handleSavePiSettings">
            {{ piSettingsSaved ? $t("common.saved") : $t("admin.savePiterSettings") }}
          </button>
        </div>
      </div>
    </div>

    <div class="section">
      <h3 class="tab-title">{{ $t("admin.brokerUrls") }}</h3>
      <div class="info-block" v-if="status">
        <div class="info-row">
          <span class="info-key">{{ $t("admin.websocket") }}</span>
          <code class="info-value">{{ status.broker_ws_url }}</code>
        </div>
        <div class="info-row">
          <span class="info-key">{{ $t("admin.http") }}</span>
          <code class="info-value">{{ status.broker_http_url }}</code>
        </div>
      </div>
    </div>
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
  margin-top: var(--space-sm);
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

.status-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-sm);
  margin-bottom: var(--space-md);
}

.status-card {
  padding: var(--space-md);
  background: var(--bg-muted);
  border-radius: var(--radius-md);
}

.clickable-card {
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease);
}
.clickable-card:hover {
  background: var(--bg-hover);
}

.status-card-label {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin-bottom: var(--space-xs);
}

.status-card-value {
  display: flex;
  align-items: center;
}

.status-card-mono {
  font-family: var(--font-mono);
  font-size: var(--font-size-body);
  color: var(--text);
}

.status-card-path {
  font-family: var(--font-mono);
  font-size: var(--font-size-micro);
  color: var(--text-secondary);
  word-break: break-all;
}

.status-card-muted {
  color: var(--text-tertiary);
}

.status-card-wide {
  grid-column: 1 / -1;
}

.session-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.session-item {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: var(--font-size-caption);
}

.control-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.settings-card {
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-lg);
  display: flex;
  flex-direction: column;
}

.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-sm) 0;
}

.settings-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
}

.settings-label-title {
  font-size: var(--font-size-body);
  color: var(--text);
}

.settings-label-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.number-input {
  width: 100px;
  flex-shrink: 0;
}

.settings-footer {
  margin-top: var(--space-md);
  padding-top: var(--space-md);
  border-top: 1px solid var(--border);
}

.action-feedback {
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
  margin-left: var(--space-sm);
}

.info-block {
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-md);
}

.info-row {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  padding: var(--space-xs) 0;
}

.info-key {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  min-width: 80px;
}

.info-value {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
  background: transparent;
}
</style>
