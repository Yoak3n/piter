<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import ChatPane from "./components/ChatPane.vue";
import SessionSidebar from "./components/SessionSidebar.vue";
import ModelSelector from "./components/ModelSelector.vue";
import { usePiConnection } from "./composables/usePiConnection";
import { useSessions } from "./composables/useSessions";

const {
  messages,
  isRunning,
  isStreaming,
  statusText,
  currentAssistantContent,
  wsSessions,
  sessionStatus,
  connectWebSocket,
  sendPrompt,
  sendCommand,
  switchSession,
  setActiveSessionPath,
  restartPi,
  loadHistory,
  clearMessages,
} = usePiConnection();

const { sessions, fetchSessions, loadMessages, deleteSession } =
  useSessions();

const sidebarOpen = ref(window.innerWidth > 640);
const activeSessionPath = ref<string | null>(null);
const sessionName = ref("");
const modelId = ref("");
const mobileMode = ref(
  new URLSearchParams(window.location.search).get("mobile") === "1",
);

function toggleSidebar() {
  sidebarOpen.value = !sidebarOpen.value;
}

function closeSidebar() {
  sidebarOpen.value = false;
}

function handleSend(text: string) {
  sendPrompt(text);
}

async function handleSelectSession(filePath: string) {
  activeSessionPath.value = filePath;
  setActiveSessionPath(filePath);
  // Look up session name from cached data (check WS-pushed first, then REST)
  const allProjects = wsSessions.value.length > 0 ? wsSessions.value : sessions.value;
  for (const project of allProjects) {
    const s = project.sessions.find((s) => s.file_path === filePath);
    if (s) {
      sessionName.value = s.label || s.id;
      break;
    }
  }
  // Tell pi to switch to this session — pi is the source of truth
  switchSession(filePath);
  // Load history from REST for display
  const msgs = await loadMessages(filePath);
  loadHistory(msgs);
  if (mobileMode.value) closeSidebar();
}

async function handleDeleteSession(filePath: string) {
  if (activeSessionPath.value === filePath) {
    activeSessionPath.value = null;
    sessionName.value = "";
    clearMessages();
  }
  await deleteSession(filePath);
  fetchSessions();
}

function handleNewSession(cwd?: string) {
  sendCommand({ type: "new_session", cwd: cwd || "." });
  clearMessages();
  activeSessionPath.value = null;
  setActiveSessionPath(null);
  sessionName.value = "";
  if (mobileMode.value) closeSidebar();
}

function handleModelSelect(modelIdStr: string) {
  modelId.value = modelIdStr;
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

// Refresh session list when pi finishes processing (new session file created)
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
        :active-session-path="activeSessionPath"
        :projects="wsSessions"
        :session-status="sessionStatus"
        @select-session="handleSelectSession"
        @delete-session="handleDeleteSession"
        @new-session="handleNewSession"
      />
    </aside>

    <!-- Main chat area -->
    <main class="app-main">
      <ChatPane
        :messages="messages"
        :is-running="isRunning"
        :is-streaming="isStreaming"
        :current-assistant-content="currentAssistantContent"
        :status-text="statusText"
        :session-name="sessionName"
        :model-name="modelId"
        :sidebar-collapsed="!sidebarOpen"
        @send="handleSend"
        @restart-pi="restartPi"
        @toggle-sidebar="toggleSidebar"
      >
        <template #header-extra>
          <ModelSelector
            :model-id="modelId"
            @select-model="handleModelSelect"
          />
        </template>
      </ChatPane>
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
