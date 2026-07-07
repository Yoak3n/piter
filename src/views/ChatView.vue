<script setup lang="ts">
import { onMounted } from "vue";
import ChatPane from "../components/ChatPane.vue";
import { usePiConnection } from "../composables/usePiConnection";

const {
  messages, isRunning, isStreaming, statusText, currentAssistantContent,
  connectWebSocket, sendPrompt, restartPi,
} = usePiConnection();

onMounted(() => {
  document.documentElement.dataset.theme =
    window.matchMedia?.("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  connectWebSocket();
});
</script>

<template>
  <div class="chat-view">
    <ChatPane
      :messages="messages"
      :is-running="isRunning"
      :is-streaming="isStreaming"
      :current-assistant-content="currentAssistantContent"
      :status-text="statusText"
      :sidebar-collapsed="true"
      :drawer-collapsed="true"
      :terminal-open="false"
      @send="sendPrompt"
      @restart-pi="restartPi"
      @toggle-sidebar="() => {}"
      @toggle-drawer="() => {}"
      @toggle-terminal="() => {}"
    />
  </div>
</template>

<style>
@import "../styles/design-system.css";

.chat-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--color-bg-app);
}
</style>
