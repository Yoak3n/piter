<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import ChatPane from "./components/ChatPane.vue";
import SessionSidebar from "./components/SessionSidebar.vue";
import ModelSelector from "./components/ModelSelector.vue";
import LanShare from "./components/LanShare.vue";
import NewSessionPane from "./components/NewSessionPane.vue";
import { usePiConnection } from "./composables/usePiConnection";
import { useSessions } from "./composables/useSessions";

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
  sendCommand,
  restartPi,
  loadHistory,
  clearMessages,
} = usePiConnection();

const { sessions, fetchSessions, loadMessages, deleteSession } =
  useSessions();

const sidebarOpen = ref(window.innerWidth > 640);
const sessionName = ref("");
const modelId = ref("");
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
  sendPrompt(text);
}

async function handleSelectSession(filePath: string) {
  showNewSession.value = false;
  const allProjects = wsSessions.value.length > 0 ? wsSessions.value : sessions.value;
  let instanceId: string | undefined;
  for (const project of allProjects) {
    const s = project.sessions.find((s) => s.filePath === filePath);
    if (s) {
      sessionName.value = s.label || s.id;
      instanceId = s.instanceId;
      break;
    }
  }
  const msgs = await loadMessages(filePath);
  loadHistory(msgs);
  if (instanceId) {
    sendCommand({ type: "switch_session", instanceId });
  }
  if (mobileMode.value) closeSidebar();
}

async function handleDeleteSession(filePath: string) {
  sessionName.value = "";
  clearMessages();
  showNewSession.value = true;
  await deleteSession(filePath);
  fetchSessions();
}

// Global "+" or per-project "+" — show the new session pane
function handleNewSession(_cwd?: string) {
  showNewSession.value = true;
  if (mobileMode.value) closeSidebar();
}

// New session pane confirmed — create the session
function handleCreateSession(payload: { cwd: string; projectId?: string; name?: string; message?: string }) {
  const cmd: Record<string, unknown> = { type: "new_session", cwd: payload.cwd };
  if (payload.projectId) cmd.projectId = payload.projectId;
  if (payload.name) {
    sessionName.value = payload.name;
  }
  // Store the first message to send after session is created
  pendingFirstMessage.value = payload.message || null;
  sendCommand(cmd);
  clearMessages();
  showNewSession.value = false;
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

// When a new instance becomes active, switch to chat pane and send pending prompt
watch(activeInstanceId, (id) => {
  if (id) {
    showNewSession.value = false;
    // Send the first message if one was pending from session creation
    if (pendingFirstMessage.value) {
      const msg = pendingFirstMessage.value;
      pendingFirstMessage.value = null;
      setTimeout(() => sendPrompt(msg), 100);
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
        :active-session-path="null"
        :projects="wsSessions"
        :session-status="sessionStatus"
        :mobile-mode="mobileMode"
        @select-session="handleSelectSession"
        @delete-session="handleDeleteSession"
        @new-session="handleNewSession"
      />
    </aside>

    <!-- Main area: new session pane OR chat pane -->
    <main class="app-main">
      <NewSessionPane
        v-if="showNewSession"
        :projects="wsSessions.map(p => ({ path: p.path, dirName: p.dirName }))"
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
            :session-status="sessionStatus"
            @select-model="handleModelSelect"
          />
          <LanShare :mobile-mode="mobileMode" />
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
