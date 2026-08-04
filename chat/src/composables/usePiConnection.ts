import { ref, reactive, computed, onUnmounted } from "vue";
import type { Message, ToolExecution, ProjectGroup, ModelRef } from "../types";
import {
  extractTextContent,
  extractThinkingContent,
  formatToolOutput,
} from "../utils/message";

// ─── Per-session state ─────────────────────────────────────────────

/** A message queued locally (outbox) while the agent is streaming. */
export interface PendingItem {
  id: number;
  text: string;
  model?: ModelRef;
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
  });
}

export function usePiConnection() {
  const sessionStates = new Map<string, SessionState>();
  const activeInstanceId = ref<string | null>(null);

  // ── Global connection state ──
  const isRunning = ref(false);
  const statusText = ref("Connecting...");
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

  /** Get the active session state, creating it if needed. */
  function getOrCreateState(instanceId: string | null): SessionState {
    if (!instanceId) return createSessionState("__transient__");
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

  // ── Message helpers (write to a specific session's state) ──

  /** Append a message and return its id (deduped against the last identical message). */
  function addMessage(state: SessionState, role: Message["role"], content: string, extras?: Partial<Message>): number {
    // Defensive dedup: skip if the identical (role, content) pair is already the
    // last message. Guards against a final answer being appended twice via
    // overlapping snapshot/event paths.
    const last = state.messages[state.messages.length - 1];
    if (last && last.role === role && last.content === content) {
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
        statusText.value = "Connected";
        break;

      case "pi_exited":
      case "disconnected":
        isRunning.value = false;
        statusText.value = "Disconnected";
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
          handleRunSettled(s);
        } else {
          // 空错误文案不渲染裸 `[Error]`（如失败信息只在 message 字段或已由其他事件展示）
          if (errText) {
            addMessage(s, "system", `[Error] ${errText}`);
          }
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
        if (Array.isArray(msgs)) {
          for (const m of msgs) {
            const modelId = m.model as string | undefined;
            if (modelId) {
              s.currentModel = { id: modelId, provider: s.currentModel?.provider };
              break;
            }
          }
        }
        s.isStreaming = false;
        s.warnedNoOutput = false;
        if (s.currentThinking || s.currentAssistantContent || s.toolExecutions.length > 0) {
          addMessage(s, "assistant", s.currentAssistantContent, {
            thinking: s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
          });
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
        }
        // Agent is now idle — deliver queued outbox messages (or flush after abort).
        handleRunSettled(s);
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
          addMessage(s, "assistant", content || s.currentAssistantContent, {
            thinking: thinking || s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
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

      case "sessions_list": {
        const raw = data.projects as Array<Record<string, unknown>> || [];
        wsSessions.value = raw.map((p) => ({
          id: p.id as string | undefined,
          path: p.path as string || "",
          name: (p.name ?? p.dirName) as string || "",
          pinned: (p.pinned as number) || 0,
          archived: (p.archived as boolean) || false,
          sessions: p.sessions as any[] || [],
        }));
        break;
      }

      case "session_status":
        sessionStatus.value = (data.status as "running" | "idle") || null;
        break;

      case "response": {
        const cmd = data.command as string;
        if (cmd === "new_session" && data.success) {
          const iid = data.instanceId as string | undefined;
          if (iid) {
            activeInstanceId.value = iid;
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
      parsed.push({
        id: i,
        role: role as Message["role"],
        content: extractTextContent(m),
        thinking: extractThinkingContent(m) || undefined,
        toolExecutions: toolExecs.length > 0 ? toolExecs : undefined,
        timestamp: (m.timestamp as number) || Date.now(),
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
    if (s.abortTimer) clearTimeout(s.abortTimer);
    s.abortTimer = null;
    s.warnedNoOutput = false;
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
        })
        .catch(() => {});
    }
    // 恢复可见 → 复位并重连（纯前端感知，不依赖后端信号）
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
    statusText.value = "Connecting...";
    ws = new WebSocket(url);
    ws.onopen = () => {
      isRunning.value = true;
      statusText.value = "Connected";
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
      statusText.value = "Disconnected";
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
      statusText.value = `Reconnecting in ${reconnectAttempts}s...`;
      reconnectTimer = setTimeout(() => connectWebSocket(), delay);
    } else {
      statusText.value = "Connection failed";
    }
  }

  // ─── Commands ───────────────────────────────────────────────────────

  function sendPrompt(text: string, desiredModel?: ModelRef | null, behavior?: DeliveryBehavior) {
    if (!text.trim()) return;
    const s = getOrCreateState(activeInstanceId.value);
    const iid = activeInstanceId.value;
    const streaming = s.isStreaming;

    if (behavior === "steer") {
      // 插队：流式中立即以 steer 投递（pi 原生队列，turn 边界生效）；空闲时直接发普通 prompt
      addMessage(s, "user", text);
      if (!iid) {
        addMessage(s, "system", "No active session yet — please wait for the session to be ready.");
        return;
      }
      const payload: Record<string, unknown> = { type: "prompt", message: text };
      if (desiredModel) payload.desiredModel = desiredModel;
      if (streaming) payload.streamingBehavior = "steer";
      sendCommand(payload);
      return;
    }

    if (streaming) {
      // 流式默认发送：进入本地 outbox，agent_end 后自动以普通 prompt 投递（等价 pi 原生 followUp）。
      // 排队期间不进入消息时间线（仅队列条展示），投递时才 addMessage 进时间线。
      // 不调用 pi 的 follow_up 命令，这样投递前可以取消/升级为插队。
      const oid = s.msgId++;
      s.outbox = [...s.outbox, { id: oid, text, model: desiredModel ?? undefined }];
      return;
    }

    addMessage(s, "user", text);
    // 空闲：立即发送
    if (!iid) {
      addMessage(s, "system", "No active session yet — please wait for the session to be ready.");
      return;
    }
    const payload: Record<string, unknown> = { type: "prompt", message: text };
    if (desiredModel) payload.desiredModel = desiredModel;
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
    addMessage(s, "user", first.text);
    const payload: Record<string, unknown> = { type: "prompt", message: first.text };
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
    addMessage(s, "user", latest.text);
    if (dropped.length > 0) {
      addMessage(
        s,
        "system",
        `[Aborted] Dropped ${dropped.length} queued message${dropped.length > 1 ? "s" : ""} — sent only the latest.`,
      );
    }
    const payload: Record<string, unknown> = { type: "prompt", message: latest.text };
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
    addMessage(s, "user", item.text);
    const payload: Record<string, unknown> = { type: "prompt", message: item.text };
    if (item.model) payload.desiredModel = item.model;
    if (s.isStreaming) payload.streamingBehavior = "steer";
    sendCommand(payload);
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
    if (iid) sessionStates.delete(iid);
  }

  onUnmounted(() => {
    if (watchdogTimer) {
      clearInterval(watchdogTimer);
      watchdogTimer = null;
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
    connectWebSocket,
    sendPrompt,
    sendCommand,
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
