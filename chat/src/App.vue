<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { TitleBar } from "@piter/ui";
import ChatPane from "./components/chat/ChatPane.vue";
import SessionSidebar from "./components/layout/SessionSidebar.vue";
import GlobalHeader from "./components/layout/GlobalHeader.vue";
import NewSessionPane from "./components/session/NewSessionPane.vue";
import { usePiConnection } from "./composables/usePiConnection";
import { useSessions } from "./composables/useSessions";
import type { ModelRef } from "./types";

// ─── Theme ────────────────────────────────────────────────────────────────
// This app is served by the gateway as a plain web page and must not depend
// on the Tauri runtime. The desktop app injects the saved theme as a `theme`
// query param when navigating here; otherwise we follow the OS preference.
const darkMedia = window.matchMedia("(prefers-color-scheme: dark)");
let currentTheme = "system";

function applyTheme() {
  const dark =
    currentTheme === "dark" || (currentTheme === "system" && darkMedia.matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

function applySavedTheme() {
  const urlTheme = new URLSearchParams(window.location.search).get("theme");
  if (urlTheme === "light" || urlTheme === "dark" || urlTheme === "system") {
    currentTheme = urlTheme;
  }
  applyTheme();
}

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
  steeringQueue,
  outbox,
  connectWebSocket,
  sendPrompt,
  abortGeneration,
  cancelQueued,
  upgradeQueued,
  newSession,
  switchSession,
  restartPi,
  clearMessages,
  sendCommand,
  setActiveInstanceId,
} = usePiConnection();

const { sessions, fetchSessions } = useSessions();

const sidebarOpen = ref(window.innerWidth > 640);
const sessionName = ref("");
const modelId = ref<ModelRef | null>(null);
const showNewSession = ref(true);
const newSessionCwd = ref("");
const newSessionName = ref("");
const pendingFirstMessage = ref<string | null>(null);

// Per-session input drafts, keyed by instanceId.
const drafts = ref<Record<string, string>>({});

const activeDraft = computed(() =>
  activeInstanceId.value && activeInstanceId.value !== "NewSession"
    ? (drafts.value[activeInstanceId.value] ?? "")
    : "",
);

function handleDraftUpdate(text: string) {
  if (activeInstanceId.value && activeInstanceId.value !== "NewSession") {
    drafts.value[activeInstanceId.value] = text;
  }
}

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

// 插队：在流式输出中立即投递（steer）
function handleSteer(text: string) {
  sendPrompt(text, modelId.value, "steer");
}

// 终止当前生成
function handleAbort() {
  abortGeneration();
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

function handleDeleteSession(instanceId: string) {
  sessionName.value = "";
  clearMessages();
  showNewSession.value = true;
  delete drafts.value[instanceId];
}

// Global "+" or per-project "+" — show the new session pane
// (per-project "+" carries the project cwd+name so the pane preselects them)
function handleNewSession(cwd?: string, name?: string) {
  newSessionCwd.value = cwd || "";
  newSessionName.value = name || "";
  showNewSession.value = true;
  // BUG-011：进入"无激活会话"态——哨兵值，侧边栏无高亮、草稿隔离
  const prev = activeInstanceId.value;
  if (prev && prev !== "NewSession") {
    setActiveInstanceId("NewSession");
    // 通知后端去激活旧会话（subscribers.remove → 无订阅者则进入 disconnected_since 计时）
    sendCommand({ type: "deactivate_session" }, prev);
  }
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
  darkMedia.addEventListener("change", applyTheme);
  applySavedTheme();
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
    <!-- Window title bar: replaces the OS title bar (desktop). Spans the full
         window width, identical to the admin view's title bar. -->
    <TitleBar>
      <template #left>
        <span class="app-brand">Piter</span>
      </template>
    </TitleBar>

    <div class="app-shell__body">
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
          :initial-cwd="newSessionCwd"
          :initial-name="newSessionName"
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
          :draft="activeDraft"
          :outbox="outbox"
          :steering-queue="steeringQueue"
          @send="handleSend"
          @steer="handleSteer"
          @abort="handleAbort"
          @cancel-queued="cancelQueued"
          @upgrade-queued="upgradeQueued"
          @update:draft="handleDraftUpdate"
          @restart-pi="restartPi"
        />
      </main>
    </div>
  </div>
</template>

<style>
@import "./styles/design-system.css";

.app-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--color-bg-app);
}

.app-shell__body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.app-brand {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.08em;
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
