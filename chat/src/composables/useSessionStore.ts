import { ref, reactive, computed } from "vue";
import type {
  Message,
  ToolExecution,
  ProjectGroup,
  ModelRef,
  ImageContent,
  SlashCommand,
} from "../types";
import {
  extractTextContent,
  extractThinkingContent,
  extractImages,
  formatToolOutput,
} from "../utils/message";
import { sessionWarnings } from "./usePiNotify";
import {
  collectPendingCards,
  answerCard,
  markCardAnswered,
  persistPendingCards,
} from "./useExtensionCards";

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

/** 流式发送的显式行为：目前仅插队（steer）会走 pi 原生队列 */
export type DeliveryBehavior = "steer";

export interface SessionState {
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

// ─── Module-level singleton state ──────────────────────────────────────────

const sessionStates = new Map<string, SessionState>();
export const activeInstanceId = ref<string | null>(null);
/** 会话列表（gateway 推送的 sessions_list），供侧栏/跳转/通知 label 使用 */
export const wsSessions = ref<ProjectGroup[]>([]);

// 未选会话时的哨兵 state（activeInstanceId === null）。必须缓存同一个对象：
// 否则 setCurrentModel 写入的 transient 与 computed 读取的不是同一实例，
// 启动时 seed 的默认模型永远无法反映到 ModelSelector。
let transientState: SessionState | null = null;

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

/** Get the active session state, creating it if needed. */
export function getOrCreateState(instanceId: string | null): SessionState {
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
export function getState(instanceId: string | null): SessionState | undefined {
  if (!instanceId) return undefined;
  return sessionStates.get(instanceId);
}

// ── Derived refs (computed from active session) ──

export const activeSessionState = computed(() => getOrCreateState(activeInstanceId.value));
export const messages = computed(() => activeSessionState.value.messages);
export const isStreaming = computed(() => activeSessionState.value.isStreaming);
export const currentAssistantContent = computed(() => activeSessionState.value.currentAssistantContent);
export const currentThinking = computed(() => activeSessionState.value.currentThinking);
export const toolExecutions = computed(() => activeSessionState.value.toolExecutions);
export const currentModel = computed(() => activeSessionState.value.currentModel);
export const steeringQueue = computed(() => activeSessionState.value.queue.steering);
export const outbox = computed(() => activeSessionState.value.outbox);
/** 当前活动会话的 pi 斜杠命令列表（null = 未加载/失败，输入 / 时懒加载） */
export const slashCommands = computed(() => activeSessionState.value.slashCommands);

// ── 发送能力注入（解耦：本模块不持有 WS，由 usePiConnection 注册）──
// 在 usePiConnection() 初始化时 registerTransport；注册前调用等价于 WS 未连接（返回 false）。

let sendCommandImpl: ((cmd: Record<string, unknown>, targetInstanceId?: string) => boolean) | null = null;
let isWsOpenImpl: (() => boolean) | null = null;

export function registerTransport(
  send: (cmd: Record<string, unknown>, targetInstanceId?: string) => boolean,
  isOpen: () => boolean,
) {
  sendCommandImpl = send;
  isWsOpenImpl = isOpen;
}

function sendCommand(cmd: Record<string, unknown>, targetInstanceId?: string): boolean {
  return sendCommandImpl ? sendCommandImpl(cmd, targetInstanceId) : false;
}

function isWsOpen(): boolean {
  return isWsOpenImpl ? isWsOpenImpl() : false;
}

// ── Message helpers (write to a specific session's state) ──

/** Append a message and return its id (deduped against the last identical message). */
export function addMessage(state: SessionState, role: Message["role"], content: string, extras?: Partial<Message>): number {
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

/** 给 prompt 类 payload 附上 images（非空才加，保持字段最小化） */
export function withImages(
  payload: Record<string, unknown>,
  images?: ImageContent[],
): Record<string, unknown> {
  if (images && images.length > 0) payload.images = images;
  return payload;
}

/** 用户选择模型：写回当前会话的 per-session model 状态（切换会话后仍保留）。 */
export function setCurrentModel(model: ModelRef) {
  const s = getOrCreateState(activeInstanceId.value);
  s.currentModel = model;
}

export function sendPrompt(
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

// ── 本地 outbox 投递 ──────────────────────────────────────────────────────

/** 会话进入空闲（agent_end / error aborted）后：abort 场景投递最新一条，否则按序投递第一条 */
export function handleRunSettled(s: SessionState) {
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
export function flushAfterAbort(s: SessionState) {
  if (s.abortTimer) clearTimeout(s.abortTimer);
  s.abortTimer = null;
  s.abortFlushPending = false;
  s.isStreaming = false;
  if (s.outbox.length === 0) return;
  // 断线时不清理 outbox，等重连后由下一次 settle 处理
  if (!isWsOpen()) return;
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
export function cancelQueued(id: number) {
  const s = getState(activeInstanceId.value);
  if (!s) return;
  // 排队消息不在时间线中，取消只需从 outbox 移除
  s.outbox = s.outbox.filter((o) => o.id !== id);
}

/** 把一条本地排队消息升级为插队（立即投递，流式中走 steer，空闲时走普通 prompt） */
export function upgradeQueued(id: number) {
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

// ─── Snapshot Loading ──────────────────────────────────────────────────────

export function loadMessagesIntoSession(instanceId: string | null, msgs: Array<Record<string, unknown>>) {
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

/** 载入完整历史（loadHistory：切换会话时整体替换消息流） */
export function loadHistory(history: Message[]) {
  const s = getOrCreateState(activeInstanceId.value);
  s.messages = history;
  s.msgId = history.length;
}

export function setActiveInstanceId(id: string | null) {
  activeInstanceId.value = id;
}

// 供事件层 / 连接层访问全部会话（watchdog 轮询、断线清理、清除会话）
export { sessionStates };
