<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { TitleBar } from "@piter/ui";
import ChatPane from "./components/chat/ChatPane.vue";
import CommandPalette from "./components/chat/CommandPalette.vue";
import SessionSidebar from "./components/layout/SessionSidebar.vue";
import GlobalHeader from "./components/layout/GlobalHeader.vue";
import NewSessionPane from "./components/session/NewSessionPane.vue";
import { usePiConnection } from "./composables/usePiConnection";
import { useSessions } from "./composables/useSessions";
import { useRecentCommands } from "./composables/useRecentCommands";
import { useTheme } from "./composables/useTheme";
import { useDefaultModel } from "./composables/useDefaultModel";
import { useSearchJump } from "./composables/useSearchJump";
import { useCommandPalette } from "./composables/useCommandPalette";
import { useSessionActions } from "./composables/useSessionActions";
import { buildPromptPayload } from "./utils/attachments";
import { i18n } from "./i18n";
import type { ImageContent, Attachment } from "./types";
import type { ExtensionNotify } from "./composables/usePiConnection";

// ─── 连接与会话（usePiConnection：连接 + 会话 store + 通知 + 扩展卡片）──
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
  notifications,
  slashCommands,
  setCurrentModel,
  connectWebSocket,
  sendPrompt,
  abortGeneration,
  respondExtensionDialog,
  cancelQueued,
  upgradeQueued,
  newSession,
  switchSession,
  restartPi,
  clearMessages,
  sendCommand,
  fetchSlashCommands,
  setActiveInstanceId,
} = usePiConnection();

const { sessions, fetchSessions } = useSessions();

// ─── 主题（明暗）──
useTheme();

// ─── 默认模型 / 视觉能力注册表 / 多模态弱提示 ──
const {
  ensureDefaultModel,
  warmModelCapabilities,
  refreshModelCapabilities,
  capabilitiesWarmed,
  visionHint,
  showVisionHint,
} = useDefaultModel();

// ─── 侧栏 / 移动端 ──
const sidebarOpen = ref(window.innerWidth > 640);
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

// ─── 搜索跳转（跨会话搜索结果 → 切会话 + 滚动定位）──
// selectSession 为懒回调：运行时才解析到 useSessionActions 的 handleSelectSession。
const { pendingScrollTarget, relativeTime, handleSearchJump } = useSearchJump({
  selectSession: (iid, keepScroll) => handleSelectSession(iid, keepScroll),
});

// ─── 会话动作（select/delete/new/create/modelSelect）+ 会话级草稿 ──
const {
  sessionName,
  showNewSession,
  newSessionCwd,
  newSessionName,
  pendingFirstMessage,
  pendingFirstAttachments,
  pendingNewModel,
  activeDraft,
  activeAttachments,
  handleDraftUpdate,
  handleAttachmentsUpdate,
  handleSelectSession,
  handleDeleteSession,
  handleNewSession,
  handleCreateSession,
  handleModelSelect,
} = useSessionActions({
  wsSessions,
  sessions,
  activeInstanceId,
  currentModel,
  mobileMode,
  pendingScrollTarget,
  switchSession,
  setCurrentModel,
  newSession,
  clearMessages,
  sendCommand,
  setActiveInstanceId,
  ensureDefaultModel,
  showVisionHint,
  closeSidebar,
});

// ─── 命令面板（Ctrl+K）──
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** 桌面端跳转管理面板/设置（Tauri 事件，与 GlobalHeader 设置按钮同一路径） */
async function openAdmin() {
  if (!isTauri) return;
  try {
    const { emit } = await import("@tauri-apps/api/event");
    await emit("navigate-to-admin");
  } catch (e) {
    console.error("[nav] emit navigate-to-admin failed:", e);
  }
}

/** 最近使用 pi 命令（模块级单例，与斜杠补全共享同一份记录） */
const { record: recordRecentCommand, reorderByRecent } = useRecentCommands();

const {
  paletteOpen,
  paletteSearchResults,
  paletteSearching,
  paletteItems,
  openPalette,
  closePalette,
  onPaletteQuery,
  handlePaletteRun,
  onGlobalKeydown,
} = useCommandPalette({
  isTauri,
  openAdmin,
  handleNewSession,
  handleSelectSession,
  slashCommands,
  wsSessions,
  recordRecentCommand,
  reorderByRecent,
  sendPrompt,
  fetchSlashCommands,
  handleSearchJump,
  relativeTime,
});

// ─── 扩展通知 toast 双容器（0.2.0 P3）────────────────────────
// 扩展 notify 保持默认 bottom（底部居中，不挡输入区指针）；会话完成等走 top（顶部，可点击跳转）。
const bottomNotifs = computed(() => notifications.value.filter((n) => n.placement !== "top"));
const topNotifs = computed(() => notifications.value.filter((n) => n.placement === "top"));

/** 点击可跳转 toast（会话完成）→ 切换到目标会话 */
function handleToastClick(n: ExtensionNotify) {
  if (n.targetInstanceId) switchSession(n.targetInstanceId);
}

// ─── ChatPane 事件转发 ──

function handleSend(payload: { text: string; images: ImageContent[] }) {
  sendPrompt(payload.text, undefined, undefined, payload.images);
}

// 插队：在流式输出中立即投递（steer）
function handleSteer(payload: { text: string; images: ImageContent[] }) {
  sendPrompt(payload.text, undefined, "steer", payload.images);
}

// 终止当前生成
function handleAbort() {
  abortGeneration();
}

// 消息流中的扩展 UI 卡片作答：回传 extension_ui_response 到对应会话
function handleRespondExtension(payload: {
  id: string;
  answer: { value?: string; confirmed?: boolean; cancelled?: boolean };
}) {
  respondExtensionDialog(payload.id, payload.answer);
}

// ─── 生命周期 ──

onMounted(() => {
  connectWebSocket();
  fetchSessions();
  // 预热多模态能力注册表（读本地模型库，立即生效）
  warmModelCapabilities();
  // 预取全局默认模型：启动时（未选 session）就让它显示在 ModelSelector，
  // 供"该 instance 未指定 model"时回退。
  ensureDefaultModel().then((m) => {
    if (m && !currentModel.value) setCurrentModel(m);
  });
  // 命令面板：Ctrl/Cmd+K 全局监听
  window.addEventListener("keydown", onGlobalKeydown);
});

onUnmounted(() => {
  window.removeEventListener("keydown", onGlobalKeydown);
});

// When a new instance becomes active, switch to chat pane and send pending prompt
watch(activeInstanceId, (id) => {
  // "NewSession" 是未激活会话时的哨兵值（点击"+"进入准备页时设置），
  // 必须跳过：否则进入准备页的瞬间 watcher 就会把 showNewSession 置回 false，
  // 卡在"无真实会话 + 无准备页"的过渡状态。
  if (id && id !== "NewSession") {
    // 会话激活（pi 已在运行）：复用该实例拉取一次完整模型目录，补齐视觉能力注册表
    if (!capabilitiesWarmed.value) {
      capabilitiesWarmed.value = true;
      refreshModelCapabilities(id);
    }
    // 新会话创建后首次激活：把创建时携带的 model seed 到该会话
    if (pendingNewModel.value) {
      setCurrentModel(pendingNewModel.value);
      pendingNewModel.value = null;
    }
    showNewSession.value = false;
    // Send the first message if one was pending from session creation
    if (pendingFirstMessage.value || pendingFirstAttachments.value) {
      const msg = pendingFirstMessage.value;
      const atts = pendingFirstAttachments.value || [];
      pendingFirstMessage.value = null;
      pendingFirstAttachments.value = null;
      // 首条消息与普通消息同一套载荷组装（文本附件拼进 prompt，图片走 images）
      const payload = buildPromptPayload(msg || "", atts, (k) => i18n.global.t(k));
      setTimeout(() => sendPrompt(payload.text, undefined, undefined, payload.images), 100);
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
         window width, identical to the admin view's title bar. Phones don't
         have draggable chrome, so the whole bar is skipped in mobile mode. -->
    <TitleBar v-if="!mobileMode">
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
          :model-ref="currentModel"
          :session-status="sessionStatus"
          :mobile-mode="mobileMode"
          @toggle-sidebar="toggleSidebar"
          @select-model="handleModelSelect"
          @open-palette="openPalette"
        />

        <NewSessionPane
          v-if="showNewSession"
          :projects="wsSessions.map(p => ({ path: p.path, name: p.name }))"
          :initial-cwd="newSessionCwd"
          :initial-name="newSessionName"
          :is-running="isRunning"
          :current-model="currentModel"
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
          :attachments="activeAttachments"
          :current-model="currentModel"
          :vision-hint="visionHint"
          :slash-commands="slashCommands"
          :scroll-target="pendingScrollTarget"
          @scroll-handled="pendingScrollTarget = null"
          @send="handleSend"
          @steer="handleSteer"
          @abort="handleAbort"
          @cancel-queued="cancelQueued"
          @upgrade-queued="upgradeQueued"
          @update:draft="handleDraftUpdate"
          @update:attachments="handleAttachmentsUpdate"
          @restart-pi="restartPi"
          @respond-extension="handleRespondExtension"
          @fetch-slash-commands="fetchSlashCommands"
        />
      </main>
    </div>

    <!-- 扩展通知 toast（notify，即发即弃）→ 双容器：底部 = 扩展 notify（原位置，点击穿透）；
         顶部 = 会话完成等需要提示的场景（可点击跳转） -->
    <div
      v-if="bottomNotifs.length"
      class="ext-toasts ext-toasts--bottom"
      aria-live="polite"
    >
      <div
        v-for="n in bottomNotifs"
        :key="n.id"
        class="ext-toast"
        :class="`ext-toast--${n.type}`"
      >
        {{ n.message }}
      </div>
    </div>
    <div
      v-if="topNotifs.length"
      class="ext-toasts ext-toasts--top"
      aria-live="polite"
    >
      <div
        v-for="n in topNotifs"
        :key="n.id"
        class="ext-toast ext-toast--clickable"
        :class="`ext-toast--${n.type}`"
        @click="handleToastClick(n)"
      >
        {{ n.message }}
      </div>
    </div>

    <!-- 命令面板（Ctrl+K / 搜索按钮） -->
    <CommandPalette
      :open="paletteOpen"
      :items="paletteItems"
      :search-results="paletteSearchResults"
      :searching="paletteSearching"
      @close="closePalette"
      @run="handlePaletteRun"
      @update:query="onPaletteQuery"
    />
  </div>
</template>

<style>
@import "@piter/ui/styles/design-system.css";

.app-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--bg);
}

.app-shell__body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.app-brand {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
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

/* ─── 扩展通知 toast ─── */
.ext-toasts {
  position: fixed;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  z-index: 90;
  pointer-events: none;
}
.ext-toasts--bottom {
  bottom: 24px;
}
.ext-toasts--top {
  /* 顶部 toast 置于标题栏（44px，--titlebar-h 可覆盖）下方，避免遮挡拖拽区与窗口控制按钮 */
  top: calc(var(--titlebar-h, 44px) + 12px);
}
.ext-toast {
  max-width: min(420px, calc(100vw - 32px));
  padding: 8px 16px;
  border-radius: var(--radius-pill);
  border: 1px solid var(--border);
  background: var(--bg-panel);
  box-shadow: var(--shadow-md);
  font-size: 12px;
  line-height: 1.5;
  color: var(--text);
  animation: extToastIn 0.2s var(--ease);
}
.ext-toast--warning {
  border-color: color-mix(in srgb, var(--warning) 45%, transparent);
  color: var(--warning);
}
.ext-toast--error {
  border-color: color-mix(in srgb, var(--danger) 45%, transparent);
  color: var(--danger);
}
/* 可点击跳转的 toast（会话完成）：恢复指针事件 + hover/active 反馈。
   底部扩展 toast 保持 pointer-events:none，避免挡住输入区点击。 */
.ext-toast--clickable {
  pointer-events: auto;
  cursor: pointer;
  transition: border-color 0.15s var(--ease), transform 0.15s var(--ease);
}
.ext-toast--clickable:hover {
  border-color: var(--primary);
}
.ext-toast--clickable:active {
  transform: scale(0.97);
}
@keyframes extToastIn {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
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
