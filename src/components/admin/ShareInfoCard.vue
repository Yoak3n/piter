<script setup lang="ts">
import { Cable } from "lucide-vue-next";
import type { LanInfo, HealthInfo } from "../../composables/useLanShare";

// ─── 连接信息卡（broker 端点 + 健康状态）──
// 纯展示组件：数据全部来自父级 useLanShare。

defineProps<{
  lanInfo: LanInfo | null;
  health: HealthInfo | null;
  online: boolean;
  gatewayPort: string;
}>();

function fmtUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}
</script>

<template>
  <div class="share-card">
    <div class="share-card-header">
      <span class="share-card-title">
        <Cable :size="14" />
        {{ $t("admin.connectionInfo") }}
      </span>
      <span class="share-card-desc">{{ $t("admin.connectionInfoDesc") }}</span>
    </div>

    <div class="info-block">
      <div class="info-row">
        <span class="info-key">{{ $t("admin.brokerWsUrl") }}</span>
        <code class="info-value">{{ lanInfo?.broker_ws_url || "—" }}</code>
      </div>
      <div class="info-row">
        <span class="info-key">{{ $t("admin.brokerHttpUrl") }}</span>
        <code class="info-value">{{ lanInfo?.http_url || "—" }}</code>
      </div>
      <div class="info-row">
        <span class="info-key">{{ $t("admin.gatewayPort") }}</span>
        <code class="info-value">{{ gatewayPort || "—" }}</code>
      </div>
      <div class="info-row">
        <span class="info-key">{{ $t("admin.healthStatus") }}</span>
        <span class="info-value">
          <span class="badge" :class="online ? 'badge-success' : 'badge-muted'">
            {{ online ? $t("admin.online") : $t("admin.offline") }}
          </span>
        </span>
      </div>
      <div class="info-row">
        <span class="info-key">{{ $t("admin.uptime") }}</span>
        <code class="info-value">{{ health ? fmtUptime(health.uptime_secs) : "—" }}</code>
      </div>
      <div class="info-row">
        <span class="info-key">{{ $t("admin.piVersion") }}</span>
        <code class="info-value">{{ health?.pi_version || "—" }}</code>
      </div>
    </div>
  </div>
</template>

<style scoped>
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

.info-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.info-row {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  padding: var(--space-xxs) 0;
}

.info-key {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  min-width: 120px;
  flex-shrink: 0;
}

.info-value {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
  word-break: break-all;
}
</style>
