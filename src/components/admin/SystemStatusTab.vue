<script setup lang="ts">
import { ref, onMounted } from "vue";
import type { AdminStatus } from "../../composables/useAdmin";

const emit = defineEmits<{
  (e: "refresh"): void;
  (e: "restart-pi"): void;
  (e: "stop-pi"): void;
}>();

const actionLabel = ref("");

defineProps<{
  status: AdminStatus | null;
  loading: boolean;
}>();

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

onMounted(() => emit("refresh"));
</script>

<template>
  <div class="tab-content">
    <div class="status-header">
      <h3 class="tab-title">System Status</h3>
      <button class="btn btn-sm btn-ghost" :disabled="loading" @click="emit('refresh')">
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
          <span v-else class="status-card-muted">—</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">Uptime</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ fmtUptime(status.uptime_secs) }}</span>
          <span v-else class="status-card-muted">—</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">Pi Instance</div>
        <div class="status-card-value">
          <span v-if="status && status.pi_instance_id" class="status-card-mono">{{ status.pi_instance_id.slice(0, 8) }}</span>
          <span v-else class="status-card-muted">—</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">Session</div>
        <div class="status-card-value">
          <span v-if="status && status.pi_session_path" class="status-card-path">{{ status.pi_session_path }}</span>
          <span v-else class="status-card-muted">—</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">App Version</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ status.app_version }}</span>
          <span v-else class="status-card-muted">—</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">Pi Version</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-mono">{{ status.pi_version }}</span>
          <span v-else class="status-card-muted">—</span>
        </div>
      </div>

      <div class="status-card">
        <div class="status-card-label">Data Directory</div>
        <div class="status-card-value">
          <span v-if="status" class="status-card-path">{{ status.data_dir }}</span>
          <span v-else class="status-card-muted">—</span>
        </div>
      </div>
    </div>

    <h3 class="tab-title">Pi Controls</h3>

    <div class="control-row">
      <button class="btn" @click="handleRestart">Restart Pi</button>
      <button class="btn btn-danger" @click="handleStop">Stop Pi</button>
      <span v-if="actionLabel" class="action-feedback">{{ actionLabel }}</span>
    </div>

    <h3 class="tab-title">Broker URLs</h3>

    <div class="info-block" v-if="status">
      <div class="info-row">
        <span class="info-key">WebSocket</span>
        <code class="info-value">{{ status.broker_url }}</code>
      </div>
      <div class="info-row">
        <span class="info-key">HTTP</span>
        <code class="info-value">{{ status.broker_http_url }}</code>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  max-width: 600px;
}

.status-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--space-md);
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0 0 var(--space-md) 0;
  padding-top: var(--space-lg);
}

.tab-title:first-child {
  padding-top: 0;
}

.status-header .tab-title {
  margin-bottom: 0;
  padding-top: 0;
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
