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
import { supportsVision } from "./utils/modelCapability";
import { buildPromptPayload } from "./utils/attachments";
import { i18n } from "./i18n";
import type { ModelRef, Attachment, ImageContent } from "./types";
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

// ─── 搜索跳转（跨会话搜索结果 → 切会话 + 滚动定位）──
const { pendingScrollTarget, relativeTime, handleSearchJump } = useSearchJump({
  selectSession: (iid, keepScroll) => handleSelectSession(iid, keepScroll),
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

// ─── 侧栏 / 新会话准备 / 会话级草稿 ──
const sidebarOpen = ref(window.innerWidth > 640);
const sessionName = ref("");
const showNewSession = ref(true);
const newSessionCwd = ref("");
const newSessionName = ref("");
const pendingFirstMessage = ref<string | null>(null);
// 新会话创建时携带的附件（准备页拖入），随首条消息一起发送。
const pendingFirstAttachments = ref<Attachment[] | null>(null);
// 新会话创建时携带的 model：首次激活时 seed 到该会话的 currentModel，
// 使新会话立即显示所选 model（pi 上报前不至于回退到默认）。
const pendingNewModel = ref<ModelRef | null>(null);

// Per-session input drafts, keyed by instanceId.
const drafts = ref<Record<string, string>>({});

// Per-session composer attachments, keyed by instanceId (lifted with drafts).
const attachmentDrafts = ref<Record<string, Attachment[]>>({});

const activeDraft = computed(() =>
  activeInstanceId.value && activeInstanceId.value !== "NewSession"
    ? (drafts.value[activeInstanceId.value] ?? "")
    : "",
);

const activeAttachments = computed<Attachment[]>(() =>
  activeInstanceId.value && activeInstanceId.value !== "NewSession"
    ? (attachmentDrafts.value[activeInstanceId.value] ?? [])
    : [],
);

function handleDraftUpdate(text: string) {
  if (activeInstanceId.value && activeInstanceId.value !== "NewSession") {
    drafts.value[activeInstanceId.value] = text;
  }
}

function handleAttachmentsUpdate(atts: Attachment[]) {
  if (activeInstanceId.value && activeInstanceId.value !== "NewSession") {
    attachmentDrafts.value[activeInstanceId.value] = atts;
  }
}

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

// ─── 会话动作 handlers ──

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

async function handleSelectSession(instanceId: string, keepScroll = false) {
  // 普通会话切换（侧边栏/面板会话项）会清掉搜索跳转目标；搜索跳转传入 keepScroll
  if (!keepScroll) pendingScrollTarget.value = null;
  showNewSession.value = false;
  const allProjects = wsSessions.value.length > 0 ? wsSessions.value : sessions.value;
  // 重启后前端内存中没有 per-instance model：从会话列表（runtime 优先、DB 兜底）
  // 恢复该会话自己的模型，切过去即 seed，ModelSelector 显示与发送都跟随会话。
  let sessionModel: { id: string; provider?: string } | null = null;
  for (const project of allProjects) {
    const s = project.sessions.find((s) => (s.instanceId ?? s.id) === instanceId);
    if (s) {
      sessionName.value = s.label || s.id;
      if (s.model) {
        sessionModel = { id: s.model, provider: s.modelProvider };
      }
      break;
    }
  }
  switchSession(instanceId);
  if (sessionModel) {
    setCurrentModel(sessionModel);
  } else {
    // 该 instance 没有持久化 model（或未找到）→ 回退全局默认模型
    const fallback = await ensureDefaultModel();
    if (fallback) setCurrentModel(fallback);
  }
  if (mobileMode.value) closeSidebar();
}

function handleDeleteSession(instanceId: string) {
  sessionName.value = "";
  clearMessages();
  showNewSession.value = true;
  delete drafts.value[instanceId];
  delete attachmentDrafts.value[instanceId];
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
async function handleCreateSession(payload: {
  cwd: string;
  name: string;
  message?: string;
  attachments?: Attachment[];
}) {
  sessionName.value = payload.name;
  pendingFirstMessage.value = payload.message || null;
  pendingFirstAttachments.value = payload.attachments || null;
  // 记录本次创建携带的 model（per-session 真源），首次激活时 seed 到新会话；
  // 当前会话无 model 时回退全局默认。
  const m = currentModel.value ?? (await ensureDefaultModel());
  pendingNewModel.value = m;
  newSession(payload.cwd, payload.name, m);
  clearMessages();
  showNewSession.value = false;
}

function handleModelSelect(model: ModelRef) {
  // 写回当前会话的 per-session model 状态（不再只改全局）
  setCurrentModel(model);
  // 切到可能不支持图片的模型且当前会话已带图 → 弱提示（不拦截）
  if (
    activeAttachments.value.some((a) => a.type === "image") &&
    !supportsVision(model)
  ) {
    showVisionHint(i18n.global.t("chat.imageUnsupported"));
  }
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
