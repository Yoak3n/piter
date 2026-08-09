import { ref, reactive, computed, onUnmounted } from "vue";
import { i18n } from "../i18n";
import type {
  Message,
  ToolExecution,
  ProjectGroup,
  ModelRef,
  ImageContent,
  ExtensionUiCard,
  SlashCommand,
} from "../types";
import {
  extractTextContent,
  extractThinkingContent,
  extractImages,
  formatToolOutput,
} from "../utils/message";
import { mapProjectGroups } from "../utils/projects";

// ─── Per-session state ─────────────────────────────────────────────────────

/** A message queued locally (outbox) while the agent is streaming. */
export interface PendingItem {
  id: number;
  text: string;
  model?: ModelRef;
  images?: ImageContent[];
  /** 投递时附加到 user 消息的 meta（如面板执行的 slash 命令灰显标记） */
  meta?: Record<string, unknown>;
}

/** 即发即弃的通知（extension_ui_request 的 notify 方法），不阻塞 pi */
export interface ExtensionNotify {
  id: number;
  message: string;
  type: "info" | "warning" | "error";
  /** 展示位置：top 用于会话完成等需要提示的场景；扩展 notify 保持默认 bottom */
  placement?: "top" | "bottom";
  /** 存在时 toast 可点击跳转到该会话 */
  targetInstanceId?: string;
}

interface SessionState {
  /** 运行实例 ID（broker_command 路由用；注意：不是 DB session id） */
  instanceId: string;
  messages: Message[];
  msgId: number;
  isStreaming: boolean;
  currentAssistantContent: string;
  currentThinking: string;
  toolExecutions: ToolExecution[];
  currentModel: ModelRef | null;
  /** pi 原生 steer 队列（只读展示，无法取消） */
  queue: { steering: string[] };
  /** 本地 followUp 队列：流式中发送的消息先进这里，agent_end 后自动投递，可取消/升级 */
  outbox: PendingItem[];
  /** 用户点击停止后，等待 pi 停稳再投递 outbox 最新一条 */
  abortFlushPending: boolean;
  abortTimer: ReturnType<typeof setTimeout> | null;
  /** 最近一次"内容进展"时间戳（流式 delta/工具调用/重试都算），watchdog 用 */
  lastProgressAt: number;
  /** 本轮生成是否已提示过"长时间无响应"（防重复，agent_start 时复位） */
  warnedNoOutput: boolean;
  /** 扩展 UI 卡片超时自动取消的定时器（卡片本身以消息形式存在于 messages 中） */
  dialogTimer: ReturnType<typeof setTimeout> | null;
  /** 该会话的 pi 斜杠命令列表缓存；null = 未加载 / 加载失败（触发时重试） */
  slashCommands: SlashCommand[] | null;
}

/** 流式发送的显式行为：目前仅插队（steer）会走 pi 原生队列 */
export type DeliveryBehavior = "steer";

// ─── No-progress watchdog（BUG-013：pi 卡死时兜底提示）────────────
const WATCHDOG_INTERVAL_MS = 15_000;
const WATCHDOG_NO_PROGRESS_MS = 90_000;
let watchdogTimer: ReturnType<typeof setInterval> | null = null;

// Tauri 窗口生命周期监听防重复注册（参考 watchdogTimer 单例写法）
let windowLifecycleRegistered = false;

function createSessionState(instanceId: string): SessionState {
  return reactive({
    instanceId,
    messages: [],
    msgId: 0,
    isStreaming: false,
    currentAssistantContent: "",
    currentThinking: "",
    toolExecutions: [],
    currentModel: null,
    queue: { steering: [] },
    outbox: [],
    abortFlushPending: false,
    abortTimer: null,
    lastProgressAt: 0,
    warnedNoOutput: false,
    dialogTimer: null,
    slashCommands: null,
  });
}

export function usePiConnection() {
  const sessionStates = new Map<string, SessionState>();
  const activeInstanceId = ref<string | null>(null);

  // ── Global connection state ──
  const isRunning = ref(false);
  const statusText = ref(i18n.global.t("chat.connecting"));
  const wsSessions = ref<ProjectGroup[]>([]);
  const sessionStatus = ref<"running" | "idle" | null>(null);

  let ws: WebSocket | null = null;
  let reconnectAttempts = 0;
  const MAX_RECONNECT_ATTEMPTS = 3;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Tauri 窗口隐藏：暂停 WS 重连（BUG-011 衍生）────────────
  // 窗口关闭（隐藏到托盘）时主动断 WS → 后端 onclose 立即清理订阅；
  // 隐藏期间暂停自动重连，恢复可见时重连（重新订阅）。
  let suspendReconnect = false;

  // ── Active session helpers ──

  // 未选会话时的哨兵 state（activeInstanceId === null）。必须缓存同一个对象：
  // 否则 setCurrentModel 写入的 transient 与 computed 读取的不是同一实例，
  // 启动时 seed 的默认模型永远无法反映到 ModelSelector。
  let transientState: SessionState | null = null;

  /** Get the active session state, creating it if needed. */
  function getOrCreateState(instanceId: string | null): SessionState {
    if (!instanceId) {
      if (!transientState) transientState = createSessionState("__transient__");
      return transientState;
    }
    let s = sessionStates.get(instanceId);
    if (!s) {
      s = createSessionState(instanceId);
      sessionStates.set(instanceId, s);
    }
    return s;
  }

  /** Get the active session state only if it already exists (for events). */
  function getState(instanceId: string | null): SessionState | undefined {
    if (!instanceId) return undefined;
    return sessionStates.get(instanceId);
  }

  // ── Derived refs (computed from active session) ──

  const activeSessionState = computed(() => getOrCreateState(activeInstanceId.value));

  const messages = computed(() => activeSessionState.value.messages);
  const isStreaming = computed(() => activeSessionState.value.isStreaming);
  const currentAssistantContent = computed(() => activeSessionState.value.currentAssistantContent);
  const currentThinking = computed(() => activeSessionState.value.currentThinking);
  const toolExecutions = computed(() => activeSessionState.value.toolExecutions);
  const currentModel = computed(() => activeSessionState.value.currentModel);
  const steeringQueue = computed(() => activeSessionState.value.queue.steering);
  const outbox = computed(() => activeSessionState.value.outbox);
  /** 当前活动会话的 pi 斜杠命令列表（null = 未加载/失败，输入 / 时懒加载） */
  const slashCommands = computed(() => activeSessionState.value.slashCommands);

  // ── 扩展通知（extension_ui_request notify，即发即弃）──
  const notifications = ref<ExtensionNotify[]>([]);
  let notifySeq = 0;
  // 通知节流：同内容去重 + 全局限频 + 可见条数上限。
  // piolium 等扩展会在 session_start / 阶段进度 / 重试时高频 notify，
  // 若全部弹 toast 会形成轰炸；去重窗口内同一条只出现一次（加载时的首次提示仍会显示）。
  const TOAST_DEDUP_MS = 30_000; // 同一 (type+message) 在 30s 内不再重复出现
  const TOAST_MIN_INTERVAL_MS = 1_000; // 全局至少间隔 1s 才允许新增一条
  const TOAST_MAX_VISIBLE = 3;
  const recentToasts = new Map<string, number>(); // `${type}:${message}` → lastShownAt
  let lastToastAt = 0;

  function pushNotify(
    type: ExtensionNotify["type"],
    message: string,
    opts?: { placement?: "top" | "bottom"; targetInstanceId?: string },
  ) {
    if (!message) return;
    const now = Date.now();
    const key = `${type}:${message}`;
    const lastShown = recentToasts.get(key) ?? 0;
    if (now - lastShown < TOAST_DEDUP_MS) return; // 去重：短时间内同一条不重复弹
    if (now - lastToastAt < TOAST_MIN_INTERVAL_MS) return; // 限频：防止多来源连发成串
    recentToasts.set(key, now);
    lastToastAt = now;
    if (recentToasts.size > 100) {
      for (const [k, ts] of recentToasts) {
        if (now - ts >= TOAST_DEDUP_MS) recentToasts.delete(k);
      }
    }
    const id = ++notifySeq;
    notifications.value = [
      ...notifications.value,
      { id, message, type, placement: opts?.placement, targetInstanceId: opts?.targetInstanceId },
    ].slice(-TOAST_MAX_VISIBLE);
    setTimeout(() => {
      notifications.value = notifications.value.filter((n) => n.id !== id);
    }, 4000);
  }

  // ── 会话完成通知（0.2.0 P3）────────────────────────────
  // 非活动会话 agent_end 时顶部 toast（可点击跳转），避免"切走就忘了回来看结果"。
  // 窗口不可见/失焦时的系统通知走 Rust 侧（gateway agent_end_hook），不依赖此开关。
  const NOTIFY_SESSION_COMPLETE_KEY = "piter:notifySessionComplete";

  /** 从 wsSessions 列表查会话 label；未命中回退 instance_id 前 8 位 */
  function getSessionLabel(instanceId: string): string {
    for (const g of wsSessions.value) {
      for (const s of g.sessions) {
        if (s.instanceId === instanceId && s.label) return s.label;
      }
    }
    return instanceId.slice(0, 8);
  }

  /** 会话完成时推送顶部 toast。受 localStorage 开关控制（默认开）。 */
  function notifySessionCompleted(instanceId: string) {
    try {
      if (localStorage.getItem(NOTIFY_SESSION_COMPLETE_KEY) === "false") return;
    } catch {
      // localStorage 不可用（隐私模式等）时默认开启
    }
    const label = getSessionLabel(instanceId);
    pushNotify("info", i18n.global.t("chat.sessionCompleted", { name: label }), {
      placement: "top",
      targetInstanceId: instanceId,
    });
  }

  // ── 月度预算提醒（0.2.0 P3）────────────────────────────
  // 拉取对比：定时轮询 GET /api/budget/status，与 localStorage 记录的"上次档位"
  // 比较，只有跨档上升（如 49%→52%）才 toast（50=info / 80=warning / 100=error）。
  // 月初重置后回落（100%→5%）不提醒；budget 未设置 / 拉取失败一律静默（percent 0）。
  const BUDGET_TIER_KEY = "piter:budgetTier";
  const BUDGET_POLL_MS = 5 * 60 * 1000; // 预算低频，5 分钟轮询足够
  let budgetTimer: ReturnType<typeof setInterval> | null = null;

  interface BudgetStatusPayload {
    used: number;
    budget: number;
    percent: number;
    tier: number;
    resetDay: number;
    cycleStart: string;
    cycleEnd: string;
  }

  function readBudgetTier(): number {
    try {
      const v = Number(localStorage.getItem(BUDGET_TIER_KEY) ?? "0");
      return Number.isFinite(v) ? v : 0;
    } catch {
      return 0;
    }
  }

  function writeBudgetTier(tier: number) {
    try {
      localStorage.setItem(BUDGET_TIER_KEY, String(tier));
    } catch {
      // localStorage 不可用时忽略（下次仍是"首见"语义）
    }
  }

  async function checkBudget() {
    try {
      const res = await fetch("/api/budget/status");
      if (!res.ok) return;
      const data = (await res.json()) as BudgetStatusPayload;
      const tier = Number(data.tier) || 0;
      const prev = readBudgetTier();
      if (tier > prev) {
        // 与会话完成通知同一 top 通道（顶部 toast）
        if (tier >= 3) pushNotify("error", i18n.global.t("chat.budgetTier100"), { placement: "top" });
        else if (tier === 2) pushNotify("warning", i18n.global.t("chat.budgetTier80"), { placement: "top" });
        else if (tier === 1) pushNotify("info", i18n.global.t("chat.budgetTier50"), { placement: "top" });
      }
      writeBudgetTier(tier);
    } catch {
      // 拉取失败静默，不打扰用户
    }
  }

  // 启动时立即检查一次（覆盖"打开应用时才跨档"的场景），随后按固定间隔轮询
  void checkBudget();
  budgetTimer = setInterval(checkBudget, BUDGET_POLL_MS);

  // ── 启动期会话诊断（扩展加载失败 / pi 启动失败）──
  // 事件可能在会话状态创建之前到达，且会话快照重建会整体替换 messages；
  // 先用 Map 暂存，快照重建时重新注入，保证用户打开该会话时仍能看到失败原因。
  const sessionWarnings = new Map<string, string[]>();

  function recordSessionWarning(iid: string | null, text: string) {
    if (!iid) return;
    const arr = sessionWarnings.get(iid) ?? [];
    if (arr.includes(text)) return; // 同一条不重复记录
    sessionWarnings.set(iid, [...arr, text]);
    const s = getState(iid);
    if (s && !s.messages.some((m) => m.role === "system" && m.content === text)) {
      addMessage(s, "system", text);
    }
  }

  // ── 未应答扩展 UI 卡片持久化（P0：切会话/刷新/重连后卡片不丢失，pi 不再永久阻塞）──
  // 切会话触发 session_snapshot → messages 整体重建；页面刷新则连 sessionStates 都重置。
  // 把未应答卡片按 instanceId 存 localStorage，快照重建时 merge 回消息流，回来仍可作答。
  const EXT_UI_STORAGE_KEY = "piter:extUiPending";

  function readStoredPendingCards(): Record<string, ExtensionUiCard[]> {
    try {
      const raw = localStorage.getItem(EXT_UI_STORAGE_KEY);
      return raw ? (JSON.parse(raw) as Record<string, ExtensionUiCard[]>) : {};
    } catch {
      return {};
    }
  }

  function writeStoredPendingCards(map: Record<string, ExtensionUiCard[]>) {
    try {
      localStorage.setItem(EXT_UI_STORAGE_KEY, JSON.stringify(map));
    } catch {
      // localStorage 不可用（隐私模式等）时静默降级：卡片仅存于内存
    }
  }

  /** 把某会话未应答卡片写入 localStorage（已应答/取消的不再持久化） */
  function persistPendingCards(s: SessionState) {
    if (!s.instanceId) return;
    const pending = s.messages
      .filter((m) => m.extUi && !m.extUi.answered)
      .map((m) => m.extUi!);
    const map = readStoredPendingCards();
    if (pending.length > 0) map[s.instanceId] = pending;
    else delete map[s.instanceId];
    writeStoredPendingCards(map);
  }

  // ── Message helpers (write to a specific session's state) ──

  /** Append a message and return its id (deduped against the last identical message). */
  function addMessage(state: SessionState, role: Message["role"], content: string, extras?: Partial<Message>): number {
    // Defensive dedup: skip if the identical (role, content) pair is already the
    // last message. Guards against a final answer being appended twice via
    // overlapping snapshot/event paths.
    const last = state.messages[state.messages.length - 1];
    // 卡片消息 content 为空，必须连 extUi.id 一起比较，否则连续两个对话框会去重漏掉第二个
    const sameExtId = (last?.extUi?.id ?? null) === (extras?.extUi?.id ?? null);
    if (last && last.role === role && last.content === content && sameExtId) {
      return last.id;
    }
    const id = state.msgId++;
    state.messages = [
      ...state.messages,
      { id, role, content, timestamp: Date.now(), ...extras },
    ];
    return id;
  }

  function getWsUrl(): string {
    const params = new URLSearchParams(window.location.search);
    const brokerWs = params.get("brokerWs");
    if (brokerWs) return brokerWs;
    const port = window.location.port;
    return `ws://${window.location.hostname}:${port}/ws`;
  }

  // ─── Event Handler ─────────────────────────────────────────────────

  /** 懒启动 watchdog：轮询所有 streaming session，超 90s 无进展提示一次 */
  function ensureWatchdog() {
    if (watchdogTimer) return;
    watchdogTimer = setInterval(() => {
      for (const s of sessionStates.values()) {
        if (!s.isStreaming) continue;
        if (Date.now() - s.lastProgressAt > WATCHDOG_NO_PROGRESS_MS && !s.warnedNoOutput) {
          s.warnedNoOutput = true;
          addMessage(s, "system", "[Warn] 长时间无响应，可点 ■ 停止");
        }
      }
    }, WATCHDOG_INTERVAL_MS);
  }

  function handlePiEvent(raw: Record<string, unknown>) {
    // ── Broker-level meta events ──
    if (raw.type === "capabilities") return;
    if (raw.type === "control_response") return;
    if (raw.type === "command_undeliverable") {
      const reason = raw.reason as string || "unknown";
      const command = raw.command as string || "unknown";
      const state = getOrCreateState(activeInstanceId.value);
      addMessage(state, "system", `[Delivery Error] Command "${command}" could not be delivered: ${reason}`);
      state.isStreaming = false;
      return;
    }

    // ── Session snapshot (from gateway, not pi) ──
    if (raw.type === "session_snapshot") {
      const iid = raw.instanceId as string;
      if (iid) {
        activeInstanceId.value = iid;
      }
      const msgs = raw.messages as Array<Record<string, unknown>> | undefined;
      if (Array.isArray(msgs) && msgs.length > 0) {
        loadMessagesIntoSession(iid || activeInstanceId.value, msgs);
      }
      return;
    }

    // ── Unwrap the event envelope ──
    const eventInstanceId = raw.instanceId as string | undefined;
    let data: Record<string, unknown>;
    if (raw.type === "event" && raw.event) {
      data = raw.event as Record<string, unknown>;
    } else if (raw.payload && typeof raw.payload === "object") {
      data = raw.payload as Record<string, unknown>;
    } else {
      data = raw;
    }

    const instanceId = eventInstanceId || activeInstanceId.value;

    switch (data.type) {
      case "pi_started":
        isRunning.value = true;
        statusText.value = i18n.global.t("common.connected");
        break;

      case "pi_exited":
        isRunning.value = false;
        statusText.value = i18n.global.t("chat.disconnected");
        // pi 进程已退出：卡片已无实例可回执，本地标成已取消并从持久化中移除
        for (const s of sessionStates.values()) {
          dismissDialog(s, false);
          persistPendingCards(s);
        }
        scheduleReconnect();
        break;

      case "disconnected":
        isRunning.value = false;
        statusText.value = i18n.global.t("chat.disconnected");
        // 仅前端 WS 断连（刷新/网络抖动/窗口恢复）：pi 进程在 gateway 里仍存活且阻塞。
        // 不清卡片、不回执 —— 卡片连同会话快照一起在重连后恢复，回来仍可作答。
        for (const s of sessionStates.values()) {
          // 断线期间定时器不应触发（发送必然失败且会把卡片误标成已取消）
          if (s.dialogTimer) {
            clearTimeout(s.dialogTimer);
            s.dialogTimer = null;
          }
          persistPendingCards(s);
        }
        scheduleReconnect();
        break;

      case "error": {
        const s = getOrCreateState(instanceId);
        const errText = (data.error as string) || (data.message as string) || "";
        const reason = (data.reason as string) || "";
        // Aborted generation: keep the partial output, just reset streaming state.
        if (/abort/i.test(errText) || reason === "aborted" || data.aborted === true) {
          if (s.currentThinking || s.currentAssistantContent || s.toolExecutions.length > 0) {
            addMessage(s, "assistant", s.currentAssistantContent, {
              thinking: s.currentThinking || undefined,
              toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
            });
          }
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
          s.isStreaming = false;
          s.warnedNoOutput = false;
          // 中止时若扩展对话框仍打开：以 cancelled 回执解除 pi 的阻塞
          dismissDialog(s, true);
          handleRunSettled(s);
        } else {
          // 空错误文案不渲染裸 `[Error]`（如失败信息只在 message 字段或已由其他事件展示）
          if (errText) {
            addMessage(s, "system", `[Error] ${errText}`);
          }
          // 本轮已终止：清理残留对话框（尽力回执 cancelled，防止 pi 永久阻塞）
          dismissDialog(s, true);
        }
        break;
      }

      case "agent_start": {
        const s = getOrCreateState(instanceId);
        s.isStreaming = true;
        s.currentAssistantContent = "";
        s.currentThinking = "";
        s.toolExecutions = [];
        // 新一轮生成：重置无进展计时与提示标记，启动 watchdog
        s.lastProgressAt = Date.now();
        s.warnedNoOutput = false;
        ensureWatchdog();
        break;
      }

      case "agent_end": {
        const s = getOrCreateState(instanceId);
        const msgs = data.messages as Array<Record<string, unknown>> | undefined;
        // message_end 未触发的兜底：从 agent_end 消息里提取 model 与图片
        let finalImages: ImageContent[] | undefined;
        if (Array.isArray(msgs)) {
          for (const m of msgs) {
            const modelId = m.model as string | undefined;
            if (modelId) {
              s.currentModel = { id: modelId, provider: s.currentModel?.provider };
            }
            if (!finalImages && (m.role as string) === "assistant") {
              const imgs = extractImages(m);
              if (imgs.length > 0) finalImages = imgs;
            }
          }
        }
        s.isStreaming = false;
        s.warnedNoOutput = false;
        // 防御：正常情况下对话框应答后即清除，这里兜底（如中止后 pi 停稳）
        dismissDialog(s, true);
        if (s.currentThinking || s.currentAssistantContent || s.toolExecutions.length > 0) {
          addMessage(s, "assistant", s.currentAssistantContent, {
            thinking: s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
            images: finalImages,
          });
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
        }
        // Agent is now idle — deliver queued outbox messages (or flush after abort).
        handleRunSettled(s);
        // 会话完成通知：非活动会话完成时顶部 toast（活动会话完成不打扰）
        if (instanceId && instanceId !== activeInstanceId.value) {
          notifySessionCompleted(instanceId);
        }
        break;
      }

      case "message_update": {
        const s = getState(instanceId);
        if (!s) break;
        const evt = data.assistantMessageEvent as Record<string, unknown> | undefined;
        if (evt?.type === "text_delta") {
          const delta = (evt.delta as string) || "";
          if (delta) {
            s.currentAssistantContent += delta;
            s.lastProgressAt = Date.now();
          }
        } else if (evt?.type === "thinking_delta") {
          const delta = (evt.delta as string) || "";
          if (delta) {
            s.currentThinking += delta;
            s.lastProgressAt = Date.now();
          }
        }
        break;
      }

      case "message_end": {
        const s = getState(instanceId);
        if (!s) break;
        const msg = data.message as Record<string, unknown> | undefined;
        if (msg?.model) {
          s.currentModel = { id: msg.model as string, provider: s.currentModel?.provider };
        }
        if (msg?.role === "assistant") {
          const content = extractTextContent(msg);
          const thinking = extractThinkingContent(msg);
          const images = extractImages(msg);
          addMessage(s, "assistant", content || s.currentAssistantContent, {
            thinking: thinking || s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
            images: images.length > 0 ? images : undefined,
          });
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
        }
        s.lastProgressAt = Date.now();
        break;
      }

      case "turn_end": {
        const s = getState(instanceId);
        if (!s) break;
        if (s.currentThinking || s.currentAssistantContent || s.toolExecutions.length > 0) {
          addMessage(s, "assistant", s.currentAssistantContent, {
            thinking: s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
          });
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
        }
        s.lastProgressAt = Date.now();
        break;
      }

      case "queue_update": {
        const s = getState(instanceId);
        if (!s) break;
        s.queue = {
          steering: Array.isArray(data.steering) ? (data.steering as string[]) : [],
        };
        break;
      }

      case "tool_execution_start": {
        const s = getState(instanceId);
        if (!s) break;
        const toolCallId = data.toolCallId as string || `tool-${Date.now()}`;
        const toolName = data.toolName as string || "Tool";
        const args = (data.args as Record<string, unknown>) || {};
        s.toolExecutions = [...s.toolExecutions, { toolCallId, toolName, args, status: "pending" }];
        s.lastProgressAt = Date.now();
        break;
      }
      case "tool_execution_update": {
        const s = getState(instanceId);
        if (!s) break;
        const toolCallId = data.toolCallId as string;
        const partialResult = data.partialResult;
        s.toolExecutions = s.toolExecutions.map((te) =>
          te.toolCallId === toolCallId
            ? { ...te, status: "streaming" as const, output: formatToolOutput(partialResult) }
            : te,
        );
        s.lastProgressAt = Date.now();
        break;
      }
      case "tool_execution_end": {
        const s = getState(instanceId);
        if (!s) break;
        const toolCallId = data.toolCallId as string;
        const result = data.result;
        const isError = data.isError as boolean || false;
        s.toolExecutions = s.toolExecutions.map((te) =>
          te.toolCallId === toolCallId
            ? { ...te, status: isError ? "error" as const : "complete" as const, output: formatToolOutput(result), isError }
            : te,
        );
        s.lastProgressAt = Date.now();
        break;
      }

      // ── 失败可见性（BUG-013）：provider 故障 / 重试 / 扩展错误不再静默 ──
      case "auto_retry_start": {
        const s = getOrCreateState(instanceId);
        const attempt = (data.attempt as number) || 0;
        const maxAttempts = (data.maxAttempts as number) || 0;
        const delayMs = (data.delayMs as number) || 0;
        const errMsg = (data.errorMessage as string) || "provider request failed";
        const suffix = delayMs > 0 ? `（${Math.round(delayMs / 1000)}s 后重试）` : "";
        addMessage(s, "system", `[Retry ${attempt}/${maxAttempts}] ${errMsg}${suffix}`);
        s.lastProgressAt = Date.now();
        break;
      }

      case "auto_retry_end": {
        const s = getOrCreateState(instanceId);
        if (data.success === true) break; // 重试成功，不打扰
        const finalError = (data.finalError as string) || "generation failed after retries";
        addMessage(s, "system", `[Error] ${finalError}`);
        s.isStreaming = false;
        s.currentAssistantContent = "";
        s.currentThinking = "";
        s.toolExecutions = [];
        s.warnedNoOutput = false;
        handleRunSettled(s);
        break;
      }

      case "extension_error": {
        const s = getOrCreateState(instanceId);
        const err = (data.error as string) || "extension error";
        const evt = (data.event as string) || "";
        addMessage(s, "system", `[Extension Error] ${err}${evt ? ` (${evt})` : ""}`);
        break;
      }

      // ── 启动期诊断（broker 从 pi stderr 解析，见 broker/process.rs）──
      case "extension_load_failed": {
        // 扩展加载失败（pi 仍可能正常启动，但缺失该扩展能力）；最常见诱因是扩展与 pi 版本不匹配
        const extPath = (data.extensionPath as string) || "";
        const err = (data.error as string) || "unknown error";
        recordSessionWarning(
          instanceId,
          `[Extension Load Failed] ${err}${extPath ? ` (${extPath})` : ""} — ${i18n.global.t("chat.startupVersionHint")}`,
        );
        break;
      }

      case "pi_startup_failed": {
        // pi 在启动宽限期内自行退出（如扩展加载失败导致无法启动）
        const s = getOrCreateState(instanceId);
        s.isStreaming = false;
        const err = (data.error as string) || "unknown error";
        recordSessionWarning(
          instanceId,
          `[Pi Failed to Start] ${err} — ${i18n.global.t("chat.startupVersionHint")}`,
        );
        pushNotify("error", `${i18n.global.t("chat.piStartupFailed")} — ${i18n.global.t("chat.startupVersionHint")}`);
        break;
      }

      // ── 交互型扩展 UI 子协议（pi docs/rpc.md）────────────────
      // 阻塞方法（select/confirm/input/editor）：以卡片形式嵌入该会话消息流，
      // pi 阻塞等待应答（用户回到该会话作答后回 extension_ui_response）；
      // 即发即弃方法（notify/setStatus/setWidget/setTitle/set_editor_text）：不阻塞。
      case "extension_ui_request": {
        const s = getOrCreateState(instanceId);
        const method = (data.method as string) || "";
        const requestId = (data.id as string) || "";

        if (method === "notify") {
          const ntype = (data.notifyType as string) || "info";
          pushNotify(
            ntype === "error" ? "error" : ntype === "warning" ? "warning" : "info",
            (data.message as string) || "",
          );
          break;
        }
        if (method === "setStatus" || method === "setWidget" || method === "setTitle" || method === "set_editor_text") {
          // 状态条 / 小组件 / 标题暂不渲染，记录日志即可（后续可扩展）
          console.log("[extension_ui]", method, data);
          break;
        }
        if (!requestId || !["select", "confirm", "input", "editor"].includes(method)) break;

        const card: ExtensionUiCard = {
          id: requestId,
          method: method as ExtensionUiCard["method"],
          title: (data.title as string) || method,
          answered: false,
          createdAt: Date.now(),
        };
        if (method === "select") card.options = Array.isArray(data.options) ? (data.options as string[]) : [];
        if (method === "confirm") card.message = (data.message as string) || undefined;
        if (method === "input") card.placeholder = (data.placeholder as string) || undefined;
        if (method === "editor") card.prefill = (data.prefill as string) || undefined;
        // timeout 单位是毫秒（pi rpc.md）；agent 到点自动以 undefined 解析，
        // 客户端定时器只为 UI 显示（到点把卡片标成已取消），并在快照恢复时用 createdAt 重算剩余时间
        const timeoutMs = typeof data.timeout === "number" ? data.timeout : undefined;
        if (timeoutMs && timeoutMs > 0) card.timeout = timeoutMs;
        // 卡片进入消息流（role="system"、content 空，由 extUi 字段渲染交互卡片）
        addMessage(s, "system", "", { extUi: card });
        persistPendingCards(s);

        if (s.dialogTimer) clearTimeout(s.dialogTimer);
        s.dialogTimer = null;
        if (card.timeout) {
          s.dialogTimer = setTimeout(() => {
            s.dialogTimer = null;
            answerCard(s, requestId, { kind: "cancelled" }, { cancelled: true });
          }, card.timeout);
        }
        break;
      }

      case "sessions_list": {
        const raw = data.projects as Array<Record<string, unknown>> || [];
        wsSessions.value = mapProjectGroups(raw);
        break;
      }

      case "session_status":
        sessionStatus.value = (data.status as "running" | "idle") || null;
        break;

      case "response": {
        const cmd = data.command as string;
        // pi 斜杠命令列表（get_commands RPC）：解析 data.commands 写入对应会话缓存。
        // 失败（success:false）静默：缓存留 null，下次输入 / 时由 fetchSlashCommands 重试。
        if (cmd === "get_commands") {
          const s = getState(instanceId);
          if (s && data.success) {
            const d = data.data as Record<string, unknown> | undefined;
            const commands = d?.commands;
            if (Array.isArray(commands)) {
              s.slashCommands = commands
                .map((c): SlashCommand => {
                  const raw = c as Record<string, unknown>;
                  const src = raw.source;
                  const source: SlashCommand["source"] = src === "prompt" || src === "skill" ? src : "extension";
                  return {
                    name: String(raw.name ?? ""),
                    description: raw.description as string | undefined,
                    source,
                    sourceInfo: (raw.sourceInfo as Record<string, unknown>) || undefined,
                  };
                })
                .filter((c) => c.name.length > 0);
            }
          }
          break;
        }
        // 模型切换失败也走现有 system 消息链路提示（失败时 prompt 仍会用旧模型继续）
        if ((cmd === "set_model" || cmd === "cycle_model") && data.success === false) {
          const s = getOrCreateState(instanceId);
          const errText = (data.error as string) || "unknown";
          addMessage(s, "system", i18n.global.t("chat.modelSwitchFailed", { msg: errText }));
          break;
        }
        if (cmd === "new_session" && data.success) {
          const iid = data.instanceId as string | undefined;
          if (iid) {
            activeInstanceId.value = iid;
            // 命令列表随会话变化：新会话缓存必须失效，触发时重新拉取
            const s = getState(iid);
            if (s) s.slashCommands = null;
          }
        }
        if (cmd === "get_state" && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          const model = d?.model as Record<string, unknown> | undefined;
          if (model?.id) {
            const s = getState(instanceId);
            if (s) s.currentModel = { id: model.id as string, provider: model.provider as string | undefined };
          }
        }
        if ((cmd === "set_model" || cmd === "cycle_model") && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          const model = (d?.model as Record<string, unknown>) || (d as Record<string, unknown> | undefined);
          if (model?.id) {
            const s = getState(instanceId);
            if (s) s.currentModel = { id: model.id as string, provider: model.provider as string | undefined };
          }
        }
        break;
      }
    }
  }

  // ─── Snapshot Loading ───────────────────────────────────────────────

  function loadMessagesIntoSession(instanceId: string | null, msgs: Array<Record<string, unknown>>) {
    const s = getOrCreateState(instanceId);
    const parsed: Message[] = [];

    // Tool results are persisted as separate role="toolResult" messages.
    // Index them by toolCallId so we can fold output/status into the matching
    // assistant toolCall block (mirroring the runtime tool_execution events).
    const toolResults = new Map<string, { output: string; isError: boolean }>();
    for (const m of msgs) {
      if (m.role === "toolResult") {
        const callId = (m.toolCallId as string) || "";
        if (callId) {
          toolResults.set(callId, {
            output: formatToolOutput(m),
            isError: (m.isError as boolean) || false,
          });
        }
      }
    }

    for (let i = 0; i < msgs.length; i++) {
      const m = msgs[i];
      const role = (m.role as string) || "assistant";
      // toolResult messages are folded into the assistant tool calls above.
      if (role === "toolResult") continue;
      const toolExecs: ToolExecution[] = [];
      if (Array.isArray(m.content)) {
        for (const block of m.content as Record<string, unknown>[]) {
          if (block.type === "toolCall") {
            const callId = (block.id as string) || `tool-${i}`;
            const res = toolResults.get(callId);
            toolExecs.push({
              toolCallId: callId,
              toolName: (block.name as string) || "Tool",
              args: (block.arguments as Record<string, unknown>) || {},
              status: res?.isError ? "error" : "complete",
              output: res?.output,
              isError: res?.isError,
            });
          }
        }
      }
      const images = extractImages(m);
      parsed.push({
        id: i,
        role: role as Message["role"],
        content: extractTextContent(m),
        thinking: extractThinkingContent(m) || undefined,
        images: images.length > 0 ? images : undefined,
        toolExecutions: toolExecs.length > 0 ? toolExecs : undefined,
        timestamp: (m.timestamp as number) || Date.now(),
      });
    }
    // ── 快照重建（切会话 / 重连 / 窗口恢复）──
    // 启动期诊断（扩展加载失败 / pi 启动失败）重新注入，避免被快照整体替换吞掉
    const warnings = sessionWarnings.get(instanceId ?? "") ?? [];
    if (warnings.length > 0) {
      for (const w of warnings) {
        parsed.push({ id: parsed.length, role: "system", content: w, timestamp: Date.now() });
      }
      sessionWarnings.delete(instanceId ?? "");
    }
    // 未应答的扩展 UI 卡片不能随快照消失：pi 仍阻塞在 extension_ui_request 上，
    // 必须把卡片 merge 回消息流（追加末尾——pi 停在卡片上，卡片就是"当前最新事件"），
    // 否则该会话将永久阻塞、只能 abort。来源：旧内存态 + localStorage（页面刷新兜底）。
    const pendingCards = collectPendingCards(s);
    for (const card of pendingCards) {
      parsed.push({
        id: parsed.length,
        role: "system",
        content: "",
        extUi: card,
        timestamp: card.createdAt ?? Date.now(),
      });
    }
    s.messages = parsed;
    s.msgId = parsed.length;
    s.isStreaming = false;
    s.currentAssistantContent = "";
    s.currentThinking = "";
    s.toolExecutions = [];
    s.queue = { steering: [] };
    s.outbox = [];
    s.abortFlushPending = false;
    // 命令列表随会话/项目配置变化：快照重建后失效，输入 / 时重新拉取
    s.slashCommands = null;
    if (s.abortTimer) clearTimeout(s.abortTimer);
    s.abortTimer = null;
    s.warnedNoOutput = false;
    if (s.dialogTimer) clearTimeout(s.dialogTimer);
    s.dialogTimer = null;
    // 重建超时定时器（按剩余时间；每会话同时至多一张 pending，取第一张带 timeout 的即可）
    for (const card of pendingCards) {
      if (card.timeout && card.createdAt) {
        const remaining = card.timeout - (Date.now() - card.createdAt);
        if (remaining > 0) {
          s.dialogTimer = setTimeout(() => {
            s.dialogTimer = null;
            answerCard(s, card.id, { kind: "cancelled" }, { cancelled: true });
          }, remaining);
        } else {
          // 超时已耗尽：agent 侧已自动解析，本地把卡片标成已取消
          markCardAnswered(s, card.id, { kind: "cancelled" });
        }
        break;
      }
    }
    persistPendingCards(s);
  }

  // ─── WebSocket ──────────────────────────────────────────────────────

  /** 注册窗口生命周期监听（幂等）：隐藏→断 WS 并暂停重连；恢复可见→重连重新订阅 */
  function registerWindowLifecycleListeners() {
    if (windowLifecycleRegistered) return;
    windowLifecycleRegistered = true;
    if (typeof window === "undefined") return;
    const isTauri = "__TAURI_INTERNALS__" in window;
    if (isTauri) {
      import("@tauri-apps/api/event")
        .then(({ listen }) => {
          listen("piter-window-hidden", () => {
            suspendReconnect = true;
            ws?.close();
          }).catch(() => {});
          // 窗口从托盘恢复可见 → 复位并重连。
          // 桌面 WebView 隐藏窗口不会触发 visibilitychange（窗口隐藏≠页面不可见），
          // 恢复侧必须由 Rust 在 Focused(true) 时显式发信号（见 init.rs）。
          listen("piter-window-shown", () => {
            if (suspendReconnect) {
              suspendReconnect = false;
              if (!ws || ws.readyState !== WebSocket.OPEN) connectWebSocket();
            }
          }).catch(() => {});
        })
        .catch(() => {});
    }
    // 浏览器/移动端兜底：页面从后台恢复时复位并重连（桌面 WebView 不触发此事件）
    document.addEventListener("visibilitychange", () => {
      if (!document.hidden && suspendReconnect) {
        suspendReconnect = false;
        if (!ws || ws.readyState !== WebSocket.OPEN) connectWebSocket();
      }
    });
  }

  function connectWebSocket() {
    registerWindowLifecycleListeners();
    const url = getWsUrl();
    statusText.value = i18n.global.t("chat.connecting");
    ws = new WebSocket(url);
    ws.onopen = () => {
      isRunning.value = true;
      statusText.value = i18n.global.t("common.connected");
      reconnectAttempts = 0;
      // Re-acknowledge current session after reconnect
      if (activeInstanceId.value) {
        ackReview(activeInstanceId.value);
      }
    };
    ws.onmessage = (e) => {
      try {
        const data = JSON.parse(e.data);
        handlePiEvent(data);
      } catch {
        // ignored
      }
    };
    ws.onclose = () => {
      isRunning.value = false;
      statusText.value = i18n.global.t("chat.disconnected");
      scheduleReconnect();
    };
    ws.onerror = () => {
      ws?.close();
    };
  }

  function scheduleReconnect() {
    if (suspendReconnect) return; // 窗口隐藏中：不自动重连（恢复可见时由 visibilitychange 接管）
    if (reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
      reconnectAttempts++;
      const delay = reconnectAttempts * 3000;
      statusText.value = i18n.global.t("chat.reconnecting", { s: reconnectAttempts });
      reconnectTimer = setTimeout(() => connectWebSocket(), delay);
    } else {
      statusText.value = i18n.global.t("chat.connectionFailed");
    }
  }

  // ─── Commands ───────────────────────────────────────────────────────

  /** 用户选择模型：写回当前会话的 per-session model 状态（切换会话后仍保留）。 */
  function setCurrentModel(model: ModelRef) {
    const s = getOrCreateState(activeInstanceId.value);
    s.currentModel = model;
  }

  /** 给 prompt 类 payload 附上 images（非空才加，保持字段最小化） */
  function withImages(
    payload: Record<string, unknown>,
    images?: ImageContent[],
  ): Record<string, unknown> {
    if (images && images.length > 0) payload.images = images;
    return payload;
  }

  function sendPrompt(
    text: string,
    desiredModel?: ModelRef | null,
    behavior?: DeliveryBehavior,
    images?: ImageContent[],
    meta?: Record<string, unknown>,
  ) {
    if (!text.trim() && (!images || images.length === 0)) return;
    const s = getOrCreateState(activeInstanceId.value);
    const iid = activeInstanceId.value;
    const streaming = s.isStreaming;
    // 未显式指定时回退到当前会话自身的 model 状态——发送永远跟随会话，而非全局残留值。
    const model = desiredModel ?? s.currentModel;
    // 附加 meta（如面板执行的 slash 命令灰显标记）；不因空对象/空数组生成多余字段
    const extras = (m?: Record<string, unknown>): Partial<Message> => ({
      ...(meta || m ? { meta: m ?? meta } : {}),
      ...(images?.length ? { images } : {}),
    });

    if (behavior === "steer") {
      // 插队：流式中立即以 steer 投递（pi 原生队列，turn 边界生效）；空闲时直接发普通 prompt
      addMessage(s, "user", text, extras());
      if (!iid) {
        addMessage(s, "system", "No active session yet — please wait for the session to be ready.");
        return;
      }
      const payload = withImages({ type: "prompt", message: text }, images);
      if (model) payload.desiredModel = model;
      if (streaming) payload.streamingBehavior = "steer";
      sendCommand(payload);
      return;
    }

    if (streaming) {
      // 流式默认发送：进入本地 outbox，agent_end 后自动以普通 prompt 投递（等价 pi 原生 followUp）。
      // 排队期间不进入消息时间线（仅队列条展示），投递时才 addMessage 进时间线。
      // 不调用 pi 的 follow_up 命令，这样投递前可以取消/升级为插队。
      const oid = s.msgId++;
      s.outbox = [...s.outbox, { id: oid, text, model: model ?? undefined, images, meta }];
      return;
    }

    addMessage(s, "user", text, extras());
    // 空闲：立即发送
    if (!iid) {
      addMessage(s, "system", "No active session yet — please wait for the session to be ready.");
      return;
    }
    const payload = withImages({ type: "prompt", message: text }, images);
    if (model) payload.desiredModel = model;
    sendCommand(payload);
  }

  // ── 本地 outbox 投递 ──────────────────────────────────────────────

  /** 会话进入空闲（agent_end / error aborted）后：abort 场景投递最新一条，否则按序投递第一条 */
  function handleRunSettled(s: SessionState) {
    if (s.abortFlushPending) {
      flushAfterAbort(s);
    } else if (s.outbox.length > 0) {
      deliverOutboxFirst(s);
    }
  }

  /** 按序投递 outbox 第一条（one-at-a-time，等价 pi followUp 默认模式） */
  function deliverOutboxFirst(s: SessionState) {
    if (!s.instanceId || s.outbox.length === 0) return;
    const [first, ...rest] = s.outbox;
    s.outbox = rest;
    // 投递时刻才进入消息时间线（排队期间仅显示在队列条）
    addMessage(s, "user", first.text, {
      ...(first.meta ? { meta: first.meta } : {}),
      ...(first.images?.length ? { images: first.images } : {}),
    });
    const payload = withImages({ type: "prompt", message: first.text }, first.images);
    if (first.model) payload.desiredModel = first.model;
    // 投递到 outbox 所属会话（s.instanceId），而非当前活动会话——
    // 防止用户在等待期间切换到其他会话时，排队消息被发到错误的会话。
    sendCommand(payload, s.instanceId);
  }

  /** 用户点击停止后：只停当前生成，投递 outbox 最新一条（当前意图），丢弃更早的排队消息 */
  function flushAfterAbort(s: SessionState) {
    if (s.abortTimer) clearTimeout(s.abortTimer);
    s.abortTimer = null;
    s.abortFlushPending = false;
    s.isStreaming = false;
    if (s.outbox.length === 0) return;
    // 断线时不清理 outbox，等重连后由下一次 settle 处理
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    const latest = s.outbox[s.outbox.length - 1];
    const dropped = s.outbox.slice(0, -1);
    s.outbox = [];
    // 投递时刻才进入消息时间线（排队期间仅显示在队列条）
    addMessage(s, "user", latest.text, {
      ...(latest.meta ? { meta: latest.meta } : {}),
      ...(latest.images?.length ? { images: latest.images } : {}),
    });
    if (dropped.length > 0) {
      addMessage(
        s,
        "system",
        `[Aborted] Dropped ${dropped.length} queued message${dropped.length > 1 ? "s" : ""} — sent only the latest.`,
      );
    }
    const payload = withImages({ type: "prompt", message: latest.text }, latest.images);
    if (latest.model) payload.desiredModel = latest.model;
    // 同样投递到 outbox 所属会话，避免切换会话后发错
    sendCommand(payload, s.instanceId);
  }

  /** 取消一条本地排队消息（纯本地，pi 不感知） */
  function cancelQueued(id: number) {
    const s = getState(activeInstanceId.value);
    if (!s) return;
    // 排队消息不在时间线中，取消只需从 outbox 移除
    s.outbox = s.outbox.filter((o) => o.id !== id);
  }

  /** 把一条本地排队消息升级为插队（立即投递，流式中走 steer，空闲时走普通 prompt） */
  function upgradeQueued(id: number) {
    const s = getState(activeInstanceId.value);
    const iid = activeInstanceId.value;
    if (!s || !iid) return;
    const item = s.outbox.find((o) => o.id === id);
    if (!item) return;
    s.outbox = s.outbox.filter((o) => o.id !== id);
    // 升级为插队即立即投递，此刻才进入消息时间线
    addMessage(s, "user", item.text, {
      ...(item.meta ? { meta: item.meta } : {}),
      ...(item.images?.length ? { images: item.images } : {}),
    });
    const payload = withImages({ type: "prompt", message: item.text }, item.images);
    if (item.model) payload.desiredModel = item.model;
    if (s.isStreaming) payload.streamingBehavior = "steer";
    sendCommand(payload);
  }

  /** 把某张扩展 UI 卡片标记为已应答（本地状态），返回是否命中 */
  function markCardAnswered(s: SessionState, id: string, result: ExtensionUiCard["result"]) {
    let found = false;
    s.messages = s.messages.map((m) => {
      if (m.extUi?.id === id && !m.extUi.answered) {
        found = true;
        return { ...m, extUi: { ...m.extUi, answered: true, result } };
      }
      return m;
    });
    if (found) persistPendingCards(s);
    return found;
  }

  /** 应答扩展 UI 卡片：标记 answered + 回传 extension_ui_response（broker_command 包裹 → 网关透传 → pi 继续执行） */
  function answerCard(s: SessionState, id: string, result: ExtensionUiCard["result"], payload: Record<string, unknown>) {
    if (!markCardAnswered(s, id, result)) return;
    if (s.dialogTimer) {
      clearTimeout(s.dialogTimer);
      s.dialogTimer = null;
    }
    if (s.instanceId) {
      sendCommand({ type: "extension_ui_response", id, ...payload }, s.instanceId);
    }
  }

  /** 清理会话的扩展 UI 卡片：respond=true 时以 cancelled 回执解除 pi 阻塞（中止/出错等兜底）。
   *  循环处理全部未应答卡片（并发扩展可能累积多张 pending）。 */
  function dismissDialog(s: SessionState, respond: boolean) {
    const pending = s.messages.filter((m) => m.extUi && !m.extUi.answered);
    if (pending.length === 0) {
      if (s.dialogTimer) {
        clearTimeout(s.dialogTimer);
        s.dialogTimer = null;
      }
      return;
    }
    for (const p of pending) {
      const id = p.extUi!.id;
      if (respond) {
        answerCard(s, id, { kind: "cancelled" }, { cancelled: true });
      } else {
        markCardAnswered(s, id, { kind: "cancelled" });
      }
    }
    if (s.dialogTimer) {
      clearTimeout(s.dialogTimer);
      s.dialogTimer = null;
    }
  }

  /** 收集会话的未应答卡片（旧内存态 + localStorage 持久化），按 id 去重 */
  function collectPendingCards(s: SessionState): ExtensionUiCard[] {
    const seen = new Set<string>();
    const out: ExtensionUiCard[] = [];
    const push = (c: ExtensionUiCard | undefined) => {
      if (!c || c.answered || seen.has(c.id)) return;
      seen.add(c.id);
      out.push(c);
    };
    for (const m of s.messages) push(m.extUi);
    if (s.instanceId) {
      for (const c of readStoredPendingCards()[s.instanceId] ?? []) push(c);
    }
    return out;
  }

  /** 用户作答消息流中的扩展 UI 卡片（卡片仅在其所属会话中可见，故从当前活动会话查找） */
  function respondExtensionDialog(id: string, answer: { value?: string; confirmed?: boolean; cancelled?: boolean }) {
    const s = getState(activeInstanceId.value);
    if (!s) return;
    let result: ExtensionUiCard["result"];
    let payload: Record<string, unknown>;
    if (answer.cancelled) {
      result = { kind: "cancelled" };
      payload = { cancelled: true };
    } else if (answer.confirmed !== undefined) {
      result = answer.confirmed ? { kind: "confirmed" } : { kind: "rejected" };
      payload = { confirmed: answer.confirmed };
    } else {
      result = { kind: "value", text: answer.value ?? "" };
      payload = { value: answer.value ?? "" };
    }
    answerCard(s, id, result, payload);
  }

  /** 终止当前生成。pi 停稳后（agent_end/error 或兜底超时）投递 outbox 最新一条。 */
  function abortGeneration() {
    sendCommand({ type: "abort" });
    const s = getState(activeInstanceId.value);
    if (s) {
      s.isStreaming = false;
      s.abortFlushPending = true;
      if (s.abortTimer) clearTimeout(s.abortTimer);
      s.abortTimer = setTimeout(() => flushAfterAbort(s), 2000);
      // 中止 agent 的同时把未应答的扩展 UI 卡片以 cancelled 回执，解除 pi 阻塞
      dismissDialog(s, true);
    }
  }

  function sendCommand(cmd: Record<string, unknown>, targetInstanceId?: string) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      const iid = targetInstanceId ?? activeInstanceId.value;
      if (iid) {
        ws.send(JSON.stringify({
          type: "broker_command",
          instanceId: iid,
          payload: cmd,
        }));
      } else {
        ws.send(JSON.stringify(cmd));
      }
      return true;
    }
    return false;
  }

  /** 懒加载当前活动会话的 pi 斜杠命令列表（仅缓存为空时；失败静默，下次触发重试） */
  function fetchSlashCommands() {
    const s = activeSessionState.value;
    if (s.slashCommands !== null || !s.instanceId) return;
    sendCommand({ type: "get_commands", id: "slash-cmds" }, s.instanceId);
  }

  function newSession(cwd: string, name: string, model?: ModelRef | null) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      const payload: Record<string, unknown> = { type: "new_session", cwd, name };
      if (model) payload.model = model;
      ws.send(JSON.stringify({
        type: "broker_command",
        payload,
      }));
    }
  }

  function ackReview(instanceId: string) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({
        type: "broker_command",
        instanceId,
        payload: { type: "ack_review", instanceId },
      }));
    }
  }

  function switchSession(instanceId: string, initialMessages?: Message[]) {
    if (initialMessages) {
      const s = getOrCreateState(instanceId);
      s.messages = initialMessages;
      s.msgId = initialMessages.length;
    }
    // 命令列表随会话变化（扩展/项目配置不同）：切会话后失效，触发时重新拉取
    const target = getOrCreateState(instanceId);
    target.slashCommands = null;
    activeInstanceId.value = instanceId;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({
        type: "broker_command",
        instanceId,
        payload: { type: "switch_session", instanceId },
      }));
      // Acknowledge review so WaitingReview → Idle
      ackReview(instanceId);
    }
  }

  function restartPi() {
    reconnectAttempts = 0;
    ws?.close();
    setTimeout(() => connectWebSocket(), 500);
  }

  function disconnect() {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    ws?.close();
    ws = null;
  }

  function loadHistory(history: Message[]) {
    const s = getOrCreateState(activeInstanceId.value);
    s.messages = history;
    s.msgId = history.length;
  }

  function clearMessages() {
    const iid = activeInstanceId.value;
    if (iid) {
      sessionStates.delete(iid);
      // 会话已删除：清理其持久化的未应答卡片，避免残留
      const map = readStoredPendingCards();
      if (map[iid]) {
        delete map[iid];
        writeStoredPendingCards(map);
      }
    }
  }

  onUnmounted(() => {
    if (watchdogTimer) {
      clearInterval(watchdogTimer);
      watchdogTimer = null;
    }
    if (budgetTimer) {
      clearInterval(budgetTimer);
      budgetTimer = null;
    }
    disconnect();
  });

  return {
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
    sendCommand,
    fetchSlashCommands,
    respondExtensionDialog,
    abortGeneration,
    cancelQueued,
    upgradeQueued,
    newSession,
    switchSession,
    ackReview,
    setActiveInstanceId: (id: string | null) => { activeInstanceId.value = id; },
    restartPi,
    disconnect,
    loadHistory,
    clearMessages,
  };
}
