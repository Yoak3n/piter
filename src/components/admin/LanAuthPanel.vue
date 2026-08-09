<script setup lang="ts">
import { KeyRound, Eye, EyeOff, Trash2, Check, Copy } from "lucide-vue-next";
import type { LanDevice } from "../../composables/useLanShare";

// ─── LAN 鉴权卡片（PIN 开关/重新生成 + 设备列表）──
// 纯展示组件：状态与操作全部来自父级 useLanShare，此处只做模板与事件转发。

defineProps<{
  enabled: boolean;
  pinSet: boolean;
  pin: string | null;
  pinVisible: boolean;
  devices: LanDevice[];
  busy: boolean;
  error: string;
  pinCopied: boolean;
}>();

defineEmits<{
  (e: "toggle"): void;
  (e: "regenerate"): void;
  (e: "revoke", token: string): void;
  (e: "revokeAll"): void;
  (e: "copyPin"): void;
  (e: "update:pinVisible", value: boolean): void;
}>();

function fmtDate(iso: string): string {
  const d = new Date(iso);
  return Number.isFinite(d.getTime())
    ? d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" })
    : "—";
}
</script>

<template>
  <div class="share-card lan-auth-card">
    <div class="share-card-header">
      <span class="share-card-title">
        <KeyRound :size="14" />
        {{ $t("admin.lanAuth") }}
      </span>
      <span class="share-card-desc">{{ $t("admin.lanAuthDesc") }}</span>
    </div>

    <div v-if="error" class="lan-auth-error">{{ error }}</div>

    <div class="lan-auth-main">
      <!-- PIN block -->
      <div class="lan-pin-block">
        <div class="lan-pin-row">
          <span class="lan-pin-code">
            <!-- pin 在内存中：明文/模糊切换；仅已设置但刷新后不可见（pinSet
                 && !pin）→ 提示"已设置 · 重新生成可查看"，避免误判为未设置 -->
            {{ pin ? (pinVisible ? pin : "••••••") : (pinSet ? $t("admin.lanPinMasked") : $t("admin.lanPinNotSet")) }}
          </span>
          <button
            v-if="pin"
            class="btn btn-ghost btn-icon btn-sm"
            :title="pinVisible ? $t('admin.lanHidePin') : $t('admin.lanShowPin')"
            @click="$emit('update:pinVisible', !pinVisible)"
          >
            <EyeOff v-if="pinVisible" :size="14" />
            <Eye v-else :size="14" />
          </button>
          <button
            v-if="pin"
            class="btn btn-ghost btn-icon btn-sm"
            :title="pinCopied ? $t('common.copied') : $t('admin.copyUrlHint')"
            @click="$emit('copyPin')"
          >
            <Check v-if="pinCopied" :size="14" />
            <Copy v-else :size="14" />
          </button>
        </div>
        <p class="lan-pin-hint">{{ $t("admin.lanPinHint") }}</p>
        <div class="lan-auth-actions">
          <label class="lan-toggle">
            <input
              type="checkbox"
              :checked="enabled"
              :disabled="busy"
              @change="$emit('toggle')"
            />
            <span>{{ $t("admin.lanPinEnabled") }}</span>
          </label>
          <button class="btn btn-sm" :disabled="busy" @click="$emit('regenerate')">
            {{ $t("admin.lanRegeneratePin") }}
          </button>
        </div>
      </div>

      <!-- Authorized devices -->
      <div class="lan-devices">
        <div class="lan-devices-head">
          <span class="lan-devices-title">{{ $t("admin.lanDevices") }}</span>
          <button
            v-if="devices.length"
            class="btn btn-ghost btn-sm"
            :disabled="busy"
            @click="$emit('revokeAll')"
          >
            <Trash2 :size="12" />
            {{ $t("admin.lanRevokeAll") }}
          </button>
        </div>
        <div v-if="!devices.length" class="lan-empty">{{ $t("admin.lanDevicesEmpty") }}</div>
        <div v-else class="lan-device-list">
          <div v-for="d in devices" :key="d.token" class="lan-device-row">
            <div class="lan-device-meta">
              <code class="lan-device-id">{{ d.token.slice(0, 8) }}…</code>
              <span class="lan-device-date">{{ fmtDate(d.createdAt) }} → {{ fmtDate(d.expiresAt) }}</span>
            </div>
            <button
              class="btn btn-ghost btn-icon btn-sm"
              :disabled="busy"
              :title="$t('admin.lanRevoke')"
              @click="$emit('revoke', d.token)"
            >
              <Trash2 :size="13" />
            </button>
          </div>
        </div>
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

.lan-auth-card {
  margin-top: var(--space-md);
}

.lan-auth-error {
  padding: var(--space-xs) var(--space-sm);
  background: var(--danger-soft);
  color: var(--danger);
  border-radius: var(--radius-sm);
  font-size: var(--font-size-caption);
}

.lan-auth-main {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-lg);
  align-items: start;
}

.lan-pin-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.lan-pin-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.lan-pin-code {
  font-family: var(--font-mono);
  font-size: 22px;
  font-weight: 600;
  letter-spacing: 0.35em;
  color: var(--text);
  font-variant-numeric: tabular-nums;
  min-height: 28px;
}

.lan-pin-hint {
  margin: 0;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  line-height: 1.5;
}

.lan-auth-actions {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  margin-top: var(--space-xs);
  flex-wrap: wrap;
}

.lan-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: var(--font-size-control);
  color: var(--text);
  cursor: pointer;
}

.lan-devices {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
  min-width: 0;
}

.lan-devices-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-md);
}

.lan-devices-title {
  font-size: var(--font-size-control);
  font-weight: 500;
  color: var(--text);
}

.lan-empty {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.lan-device-list {
  display: grid;
  gap: var(--space-xs);
}

.lan-device-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-sm);
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-xs) var(--space-sm);
}

.lan-device-meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.lan-device-id {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
}

.lan-device-date {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

@media (max-width: 900px) {
  .lan-auth-main {
    grid-template-columns: 1fr;
  }
}
</style>
