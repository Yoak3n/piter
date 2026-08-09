import { ref, computed } from "vue";
import type { Ref } from "vue";
import { i18n } from "../i18n";
import { supportsVision } from "../utils/modelCapability";
import type { ModelRef, Attachment, ProjectGroup, Message } from "../types";

// ─── 会话管理 handler（select/delete/new/create/modelSelect）+ 会话级草稿 ──
// 从 App.vue 下沉：本 composable 自持会话级 UI 状态（sessionName/草稿/待创建
// 会话的 message+attachments+model），依赖经 deps 注入（连接层 + 数据源 + 回调）。

export interface SessionActionsDeps {
  wsSessions: Ref<ProjectGroup[]>;
  sessions: Ref<ProjectGroup[]>;
  activeInstanceId: Ref<string | null>;
  currentModel: Ref<ModelRef | null>;
  mobileMode: Ref<boolean>;
  /** 搜索跳转目标（可写：普通切换时清除，搜索跳转保留） */
  pendingScrollTarget: Ref<{ sessionId: string; timestamp?: number; query: string } | null>;
  switchSession: (instanceId: string, initialMessages?: Message[]) => void;
  setCurrentModel: (model: ModelRef) => void;
  newSession: (cwd: string, name: string, model?: ModelRef | null) => void;
  clearMessages: () => void;
  sendCommand: (cmd: Record<string, unknown>, targetInstanceId?: string) => boolean;
  setActiveInstanceId: (id: string | null) => void;
  ensureDefaultModel: () => Promise<ModelRef | null>;
  showVisionHint: (text: string) => void;
  closeSidebar: () => void;
}

export function useSessionActions(deps: SessionActionsDeps) {
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
    deps.activeInstanceId.value && deps.activeInstanceId.value !== "NewSession"
      ? (drafts.value[deps.activeInstanceId.value] ?? "")
      : "",
  );

  const activeAttachments = computed<Attachment[]>(() =>
    deps.activeInstanceId.value && deps.activeInstanceId.value !== "NewSession"
      ? (attachmentDrafts.value[deps.activeInstanceId.value] ?? [])
      : [],
  );

  function handleDraftUpdate(text: string) {
    if (deps.activeInstanceId.value && deps.activeInstanceId.value !== "NewSession") {
      drafts.value[deps.activeInstanceId.value] = text;
    }
  }

  function handleAttachmentsUpdate(atts: Attachment[]) {
    if (deps.activeInstanceId.value && deps.activeInstanceId.value !== "NewSession") {
      attachmentDrafts.value[deps.activeInstanceId.value] = atts;
    }
  }

  async function handleSelectSession(instanceId: string, keepScroll = false) {
    // 普通会话切换（侧边栏/面板会话项）会清掉搜索跳转目标；搜索跳转传入 keepScroll
    if (!keepScroll) deps.pendingScrollTarget.value = null;
    showNewSession.value = false;
    const allProjects = deps.wsSessions.value.length > 0 ? deps.wsSessions.value : deps.sessions.value;
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
    deps.switchSession(instanceId);
    if (sessionModel) {
      deps.setCurrentModel(sessionModel);
    } else {
      // 该 instance 没有持久化 model（或未找到）→ 回退全局默认模型
      const fallback = await deps.ensureDefaultModel();
      if (fallback) deps.setCurrentModel(fallback);
    }
    if (deps.mobileMode.value) deps.closeSidebar();
  }

  function handleDeleteSession(instanceId: string) {
    sessionName.value = "";
    deps.clearMessages();
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
    const prev = deps.activeInstanceId.value;
    if (prev && prev !== "NewSession") {
      deps.setActiveInstanceId("NewSession");
      // 通知后端去激活旧会话（subscribers.remove → 无订阅者则进入 disconnected_since 计时）
      deps.sendCommand({ type: "deactivate_session" }, prev);
    }
    if (deps.mobileMode.value) deps.closeSidebar();
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
    const m = deps.currentModel.value ?? (await deps.ensureDefaultModel());
    pendingNewModel.value = m;
    deps.newSession(payload.cwd, payload.name, m);
    deps.clearMessages();
    showNewSession.value = false;
  }

  function handleModelSelect(model: ModelRef) {
    // 写回当前会话的 per-session model 状态（不再只改全局）
    deps.setCurrentModel(model);
    // 切到可能不支持图片的模型且当前会话已带图 → 弱提示（不拦截）
    if (
      activeAttachments.value.some((a) => a.type === "image") &&
      !supportsVision(model)
    ) {
      deps.showVisionHint(i18n.global.t("chat.imageUnsupported"));
    }
  }

  return {
    sessionName,
    showNewSession,
    newSessionCwd,
    newSessionName,
    pendingFirstMessage,
    pendingFirstAttachments,
    pendingNewModel,
    drafts,
    attachmentDrafts,
    activeDraft,
    activeAttachments,
    handleDraftUpdate,
    handleAttachmentsUpdate,
    handleSelectSession,
    handleDeleteSession,
    handleNewSession,
    handleCreateSession,
    handleModelSelect,
  };
}
