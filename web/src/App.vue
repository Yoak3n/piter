<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import ChatPane from "./components/ChatPane.vue";
import SessionSidebar from "./components/SessionSidebar.vue";
import GlobalHeader from "./components/GlobalHeader.vue";
import NewSessionPane from "./components/NewSessionPane.vue";
import { usePiConnection } from "./composables/usePiConnection";
import { useSessions } from "./composables/useSessions";
import type { ModelRef } from "./types";

const {
  messages,
  isRunning,
  isStreaming,
  statusText,
  currentAssistantContent,
  currentThinking,
  toolExecutions,
  activeInstanceId,
  wsSessions,
  sessionStatus,
  currentModel,
  connectWebSocket,
  sendPrompt,
  newSession,
  switchSession,
  restartPi,
  clearMessages,
} = usePiConnection();

const { sessions, fetchSessions } = useSessions();

const sidebarOpen = ref(window.innerWidth > 640);
const sessionName = ref("");
const modelId = ref<ModelRef | null>(null);
const showNewSession = ref(true);
const pendingFirstMessage = ref<string | null>(null);

// Sync model from WS events into the modelId ref
watch(currentModel, (m) => {
  if (m) modelId.value = m;
});

const mobileMode = ref(
  new URLSearchParams(window.location.search).get("mobile") === "1" ||
  /Mobi|Android|iPhone/i.test(navigator.userAgent),
);

function toggleSidebar() {
  sidebarOpen.value = !sidebarOpen.value;
}

function closeSidebar() {
  sidebarOpen.value = false;
}

function handleSend(text: string) {
  sendPrompt(text, modelId.value);
}

async function handleSelectSession(instanceId: string) {
  showNewSession.value = false;
  const allProjects = wsSessions.value.length > 0 ? wsSessions.value : sessions.value;
  for (const project of allProjects) {
    const s = project.sessions.find((s) => (s.instanceId ?? s.id) === instanceId);
    if (s) {
      sessionName.value = s.label || s.id;
      break;
    }
  }
  switchSession(instanceId);
  if (mobileMode.value) closeSidebar();
}

function handleDeleteSession(_instanceId: string) {
  sessionName.value = "";
  clearMessages();
  showNewSession.value = true;
}

// Global "+" or per-project "+" — show the new session pane
function handleNewSession(_cwd?: string) {
  showNewSession.value = true;
  if (mobileMode.value) closeSidebar();
}

// New session pane confirmed — create the session
async function handleCreateSession(payload: { cwd: string; name: string; message?: string }) {
  sessionName.value = payload.name;
  pendingFirstMessage.value = payload.message || null;
  newSession(payload.cwd, payload.name, modelId.value);
  clearMessages();
  showNewSession.value = false;
}

function handleModelSelect(model: ModelRef) {
  modelId.value = model;
}

onMounted(() => {
  document.documentElement.dataset.theme = window.matchMedia?.(
    "(prefers-color-scheme: dark)",
  ).matches
    ? "dark"
    : "light";
  connectWebSocket();
  fetchSessions();
});

// When a new instance becomes active, switch to chat pane and send pending prompt
watch(activeInstanceId, (id) => {
  if (id) {
    showNewSession.value = false;
    // Send the first message if one was pending from session creation
    if (pendingFirstMessage.value) {
      const msg = pendingFirstMessage.value;
      pendingFirstMessage.value = null;
      setTimeout(() => sendPrompt(msg, modelId.value), 100);
    }
  }
});

// Refresh session list when pi finishes processing
watch(sessionStatus, (status) => {
  if (status === "idle") {
    setTimeout(() => fetchSessions(), 500);
    setTimeout(() => fetchSessions(), 2000);
  }
});
</script>

<template>
  <div class="app-shell">
    <!-- Sidebar overlay for mobile -->
    <div
      v-if="sidebarOpen && mobileMode"
      class="sidebar-overlay"
      @click="closeSidebar"
    />

    <!-- Session sidebar -->
    <aside class="app-sidebar" :class="{ open: sidebarOpen, closed: !sidebarOpen }">
      <SessionSidebar
        :active-session-id="activeInstanceId"
        :projects="wsSessions"
        :session-status="sessionStatus"
        :mobile-mode="mobileMode"
        @select-session="handleSelectSession"
        @delete-session="handleDeleteSession"
        @new-session="handleNewSession"
      />
    </aside>

    <!-- Main area: global header + new session pane OR chat pane -->
    <main class="app-main">
      <GlobalHeader
        :session-name="sessionName"
        :show-session-name="!showNewSession"
        :is-running="isRunning"
        :status-text="statusText"
        :model-id="modelId"
        :session-status="sessionStatus"
        :mobile-mode="mobileMode"
        @toggle-sidebar="toggleSidebar"
        @select-model="handleModelSelect"
      />

      <NewSessionPane
        v-if="showNewSession"
        :projects="wsSessions.map(p => ({ path: p.path, name: p.name }))"
        :mobile-mode="mobileMode"
        @create="handleCreateSession"
      />
      <ChatPane
        v-else
        :messages="messages"
        :is-running="isRunning"
        :is-streaming="isStreaming"
        :current-assistant-content="currentAssistantContent"
        :current-thinking="currentThinking"
        :tool-executions="toolExecutions"
        @send="handleSend"
        @restart-pi="restartPi"
      />
    </main>
  </div>
</template>

<style>
@import "./styles/design-system.css";

.app-shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
  background: var(--color-bg-app);
}

.app-sidebar {
  width: 260px;
  flex-shrink: 0;
  transition: margin-left 0.25s var(--ease);
}

.app-sidebar.closed {
  margin-left: -260px;
}

.app-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.sidebar-overlay {
  display: none;
}

@media (max-width: 640px) {
  .app-sidebar {
    position: fixed;
    inset: 0;
    z-index: 40;
    width: 100%;
    max-width: 300px;
    transition: transform 0.25s var(--ease);
  }

  .app-sidebar.closed {
    margin-left: 0;
    transform: translateX(-100%);
  }

  .sidebar-overlay {
    display: block;
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    z-index: 39;
  }
}
</style>
