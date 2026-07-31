<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { Activity, RefreshCw } from "lucide-vue-next";
import type { AdminStatus } from "../../composables/useAdmin";

const props = defineProps<{
  status: AdminStatus | null;
  loading: boolean;
}>();

const emit = defineEmits<{
  (e: "refresh"): void;
  (e: "restart-pi"): void;
  (e: "stop-pi"): void;
  (e: "open-path", path: string): void;
}>();

const actionLabel = ref("");

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
  actionLabel.value = "Restarting...";
  emit("restart-pi");
  setTimeout(() => (actionLabel.value = ""), 3000);
}

function handleStop() {
  actionLabel.value = "Stopping...";
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
        <h3 class="tab-title">System Status</h3>
        <p class="tab-desc">Pi runtime and application overview</p>
      </div>
      <button class="btn btn-sm" :disabled="loading" @click="emit('refresh')">
        <RefreshCw :size="12" :class="{ 'spin': loading }" />
        {{ loading ? "Refreshing..." : "Refresh" }}
      </button>
    </div>

    <div class="status-grid">
      <div class="status-card">
        <div class="status-card-label">Pi Status</div>
        <div class="status-card-value">
          <span
            v-if="status"
            class="badge"
            :class="status.pi_running ? 'badge-success' : 'badge-muted'"
          >
            {{ status.pi_running ? "Running" : "Stopped" }}
          </span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">Uptime</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ fmtUptime(liveUptime) }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">App Version</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ status.app_version }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">Pi Version</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ status.pi_version }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card status-card-wide clickable-card" @click="status && emit('open-path', status.data_dir)">
        <div class="status-card-label">Data Directory</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-path">{{ status.data_dir }}</span>
          <span v-else class="status-card-muted">&mdash;</span>
        </div>
      </div>

      <div class="status-card status-card-wide">
        <div class="status-card-label">Active Sessions ({{ status?.active_sessions.length ?? 0 }})</div>
        <div v-if="status && status.active_sessions.length" class="session-list">
          <div v-for="s in status.active_sessions" :key="s.instance_id" class="session-item">
            <span class="status-card-mono">{{ s.instance_id.slice(0, 8) }}</span>
            <span class="badge" :class="s.state === 'running' ? 'badge-success' : 'badge-muted'">{{ s.state }}</span>
            <span class="status-card-path">{{ s.cwd }}</span>
          </div>
        </div>
        <div v-else class="status-card-muted">No active sessions</div>
      </div>
    </div>

    <div class="section">
      <h3 class="tab-title">Pi Controls</h3>
      <div class="control-row">
        <button class="btn" @click="handleRestart">Restart Pi</button>
        <button class="btn btn-danger" @click="handleStop">Stop Pi</button>
        <span v-if="actionLabel" class="action-feedback">{{ actionLabel }}</span>
      </div>
    </div>

    <div class="section">
      <h3 class="tab-title">Broker URLs</h3>
      <div class="info-block" v-if="status">
        <div class="info-row">
          <span class="info-key">WebSocket</span>
          <code class="info-value">{{ status.broker_ws_url }}</code>
        </div>
        <div class="info-row">
          <span class="info-key">HTTP</span>
          <code class="info-value">{{ status.broker_http_url }}</code>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  max-width: 620px;
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
