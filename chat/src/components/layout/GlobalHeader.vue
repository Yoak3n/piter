<script setup lang="ts">
import { TitleBar } from "@piter/ui";
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
  <TitleBar>
    <template #left>
      <button class="hamburger-btn" @click="emit('toggle-sidebar')">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg>
      </button>
      <span v-if="sessionName && showSessionName" class="session-label">{{ sessionName }}</span>
    </template>

    <template #right>
      <ModelSelector
        :model-id="modelId?.id"
        :session-status="sessionStatus"
        @select-model="emit('select-model', $event)"
      />
      <LanShare :mobile-mode="mobileMode" />
      <button
        v-if="isTauri"
        class="hamburger-btn"
        title="Desktop settings"
        @click="openAdmin"
      >
        <Settings :size="14" />
      </button>
      <span class="status-dot" :class="{ connected: isRunning, disconnected: !isRunning }" :title="isRunning ? 'Connected' : 'Disconnected'" />
      <span v-if="!isRunning" class="status-label disconnected-label">{{ statusText }}</span>
    </template>
  </TitleBar>
</template>

<style scoped>
.hamburger-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: none;
  cursor: pointer;
  color: var(--color-text-secondary);
  border-radius: 6px;
  flex-shrink: 0;
}
.hamburger-btn:hover {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
}
.session-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 200px;
}
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #999;
  flex-shrink: 0;
  transition: background 0.3s, box-shadow 0.3s;
}
.status-dot.connected {
  background: #34d399;
  box-shadow: 0 0 6px 2px rgba(52, 211, 153, 0.5);
}
.status-dot.disconnected {
  background: #ef4444;
}
.status-label {
  font-size: 10px;
  color: var(--color-text-tertiary);
}
.disconnected-label {
  color: var(--color-danger);
}
</style>
