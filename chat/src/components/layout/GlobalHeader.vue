<script setup lang="ts">
import ModelSelector from "../model/ModelSelector.vue";
import LanShare from "./LanShare.vue";
import { Settings } from "lucide-vue-next";
import type { ModelRef } from "../../types";

defineProps<{
  sessionName?: string;
  showSessionName: boolean;
  isRunning: boolean;
  statusText: string;
  modelId?: ModelRef | null;
  sessionStatus: "running" | "idle" | null;
  mobileMode: boolean;
}>();

const emit = defineEmits<{
  (e: "toggle-sidebar"): void;
  (e: "select-model", model: ModelRef): void;
}>();

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function openAdmin() {
  try {
    const { emit } = await import("@tauri-apps/api/event");
    await emit("navigate-to-admin");
  } catch (e) {
    console.error("[nav] emit navigate-to-admin failed:", e);
  }
}
</script>

<template>
  <header class="global-header">
    <button class="hamburger-btn" @click="emit('toggle-sidebar')">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg>
    </button>
    <div class="header-info">
      <span v-if="sessionName && showSessionName" class="session-label">{{ sessionName }}</span>
    </div>
    <div class="header-right">
      <ModelSelector
        :model-id="modelId"
        :session-status="sessionStatus"
        @select-model="emit('select-model', $event)"
      />
      <LanShare :mobile-mode="mobileMode" />
      <button
        v-if="isTauri"
        class="hamburger-btn"
        :title="$t('chat.settings')"
        @click="openAdmin"
      >
        <Settings :size="14" />
      </button>
      <span class="status-dot" :class="{ connected: isRunning, disconnected: !isRunning }" :title="isRunning ? $t('common.connected') : $t('chat.disconnected')" />
      <span v-if="!isRunning" class="status-label disconnected-label">{{ statusText }}</span>
    </div>
  </header>
</template>

<style scoped>
.global-header {
  display: flex;
  align-items: center;
  padding: 0 12px;
  height: 44px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel);
}
.header-info {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
}
.header-right {
  display: flex;
  align-items: center;
  gap: 8px;
}
.hamburger-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: none;
  cursor: pointer;
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}
.hamburger-btn:hover {
  background: var(--bg-hover);
  color: var(--text);
}
.session-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 200px;
}
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-tertiary);
  flex-shrink: 0;
  transition: background var(--duration) var(--ease), box-shadow var(--duration) var(--ease);
}
.status-dot.connected {
  background: var(--state-idle);
  box-shadow: 0 0 6px 2px color-mix(in srgb, var(--state-idle) 50%, transparent);
}
.status-dot.disconnected {
  background: var(--state-error);
}
.status-label {
  font-size: 10px;
  color: var(--text-tertiary);
}
.disconnected-label {
  color: var(-danger);
}
</style>
