<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { TitleBar } from "@piter/ui";
import ChatPane from "./components/chat/ChatPane.vue";
import SessionSidebar from "./components/layout/SessionSidebar.vue";
import GlobalHeader from "./components/layout/GlobalHeader.vue";
import NewSessionPane from "./components/session/NewSessionPane.vue";
import { usePiConnection } from "./composables/usePiConnection";
import { useSessions } from "./composables/useSessions";
import { i18n } from "./i18n";
import { supportsVision, registerModelCapabilities } from "./utils/modelCapability";
import { buildPromptPayload } from "./utils/attachments";
import type { ModelRef, Attachment, ImageContent } from "./types";

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
  setCurrentModel,
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
const showNewSession = ref(true);
const newSessionCwd = ref("");
const newSessionName = ref("");
const pendingFirstMessage = ref<string | null>(null);
// 新会话创建时携带的附件（准备页拖入），随首条消息一起发送。
const pendingFirstAttachments = ref<Attachment[] | null>(null);
// 新会话创建时携带的 model：首次激活时 seed 到该会话的 currentModel，
// 使新会话立即显示所选 model（pi 上报前不至于回退到默认）。
const pendingNewModel = ref<ModelRef | null>(null);

// 全局默认模型缓存（/api/pi/settings）：该 instance 未指定 model 时回退用它
const defaultModel = ref<ModelRef | null>(null);
async function ensureDefaultModel(): Promise<ModelRef | null> {
  if (defaultModel.value) return defaultModel.value;
  try {
    const res = await fetch("/api/pi/settings");
    const data = await res.json();
    if (data.success && data.default_model) {
      defaultModel.value = {
        id: data.default_model,
        provider: data.default_provider,
      };
    }
  } catch {
    // non-critical
  }
  return defaultModel.value;
}

// ── 视觉能力注册表（方案 B：以 pi 模态声明为准） ─────────────────────
// 数据来源全部"不额外启动 pi 进程"：
//   1. 启动时：只读本地动态目录缓存 /api/pi/model-catalog（零成本，覆盖 opencode-go 等）；
//   2. 会话激活后：pi 已在运行，get_available_models 带上该会话 instanceId 复用实例
//      （补齐内置目录 DeepSeek/OpenAI… 与自定义 provider），且只取一次；
//   3. 打开模型下拉时 ModelSelector 也会登记（最新/自定义 provider）。
// 判定入口 supportsVision(注册表 → 正则回退)，用于附加图片/切换模型时的弱提示。
let capabilitiesWarmedOnce = false;
async function warmModelCapabilities() {
  try {
    const res = await fetch("/api/pi/model-catalog");
    const data = await res.json();
    if (data.success && Array.isArray(data.models)) {
      registerModelCapabilities(data.models);
    }
  } catch {
    // non-critical
  }
}

async function refreshModelCapabilities(instanceId: string) {
  try {
    const res = await fetch("/api/rpc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ type: "get_available_models", instanceId }),
    });
    const data = await res.json();
    if (data.success && Array.isArray(data.data?.models)) {
      registerModelCapabilities(data.data.models);
    }
  } catch {
    // non-critical
  }
}

// Per-session input drafts, keyed by instanceId.
const drafts = ref<Record<string, string>>({});

// Per-session composer attachments, keyed by instanceId (lifted with drafts).
const attachmentDrafts = ref<Record<string, Attachment[]>>({});

// 多模态弱提示（选图时模型不支持 / 切换模型后仍带附图），传给 Composer 提示条
const visionHint = ref<{ text: string; key: number } | null>(null);
let visionHintTimer: ReturnType<typeof setTimeout> | null = null;
function showVisionHint(text: string) {
  visionHint.value = { text, key: (visionHint.value?.key ?? 0) + 1 };
  if (visionHintTimer) clearTimeout(visionHintTimer);
  visionHintTimer = setTimeout(() => {
    visionHint.value = null;
  }, 4000);
}

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

async function handleSelectSession(instanceId: string) {
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

onMounted(() => {
  darkMedia.addEventListener("change", applyTheme);
  applySavedTheme();
  connectWebSocket();
  fetchSessions();
  // 预热多模态能力注册表（读本地模型库，立即生效）
  warmModelCapabilities();
  // 预取全局默认模型：启动时（未选 session）就让它显示在 ModelSelector，
  // 供"该 instance 未指定 model"时回退。
  ensureDefaultModel().then((m) => {
    if (m && !currentModel.value) setCurrentModel(m);
  });
});

// When a new instance becomes active, switch to chat pane and send pending prompt
watch(activeInstanceId, (id) => {
  // "NewSession" 是未激活会话时的哨兵值（点击"+"进入准备页时设置），
  // 必须跳过：否则进入准备页的瞬间 watcher 就会把 showNewSession 置回 false，
  // 卡在"无真实会话 + 无准备页"的过渡状态。
  if (id && id !== "NewSession") {
    // 会话激活（pi 已在运行）：复用该实例拉取一次完整模型目录，补齐视觉能力注册表
    if (!capabilitiesWarmedOnce) {
      capabilitiesWarmedOnce = true;
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
          @send="handleSend"
          @steer="handleSteer"
          @abort="handleAbort"
          @cancel-queued="cancelQueued"
          @upgrade-queued="upgradeQueued"
          @update:draft="handleDraftUpdate"
          @update:attachments="handleAttachmentsUpdate"
          @restart-pi="restartPi"
        />
      </main>
    </div>
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
