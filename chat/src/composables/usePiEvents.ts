import { i18n } from "../i18n";
import { mapProjectGroups } from "../utils/projects";
import type { ImageContent, SlashCommand, ExtensionUiCard } from "../types";
import {
  extractTextContent,
  extractThinkingContent,
  extractImages,
  formatToolOutput,
} from "../utils/message";
import {
  getOrCreateState,
  getState,
  addMessage,
  loadMessagesIntoSession,
  handleRunSettled,
  sessionStates,
  activeInstanceId,
  wsSessions,
} from "./useSessionStore";
import { pushNotify, recordSessionWarning, notifySessionCompleted } from "./usePiNotify";
import { dismissDialog, persistPendingCards, answerCard } from "./useExtensionCards";
import { isRunning, statusText, sessionStatus, scheduleReconnect, ensureWatchdog } from "./usePiConnection";

type EventPayload = Record<string, unknown>;
type Handler = (data: EventPayload, instanceId: string | null) => void;

// ─── Broker-level meta events（先于 envelope 解包处理）────────────────────

function handleCommandUndeliverable(raw: Record<string, unknown>) {
  const reason = raw.reason as string || "unknown";
  const command = raw.command as string || "unknown";
  const state = getOrCreateState(activeInstanceId.value);
  addMessage(state, "system", `[Delivery Error] Command "${command}" could not be delivered: ${reason}`);
  state.isStreaming = false;
}

function handleSessionSnapshot(raw: Record<string, unknown>) {
  const iid = raw.instanceId as string;
  if (iid) {
    activeInstanceId.value = iid;
  }
  const msgs = raw.messages as Array<Record<string, unknown>> | undefined;
  if (Array.isArray(msgs) && msgs.length > 0) {
    loadMessagesIntoSession(iid || activeInstanceId.value, msgs);
  }
}

// ─── Per-event handlers（由 handlePiEvent 按 data.type 分发）───────────────

function handlePiStarted() {
  isRunning.value = true;
  statusText.value = i18n.global.t("common.connected");
}

function handlePiExited() {
  isRunning.value = false;
  statusText.value = i18n.global.t("chat.disconnected");
  // pi 进程已退出：卡片已无实例可回执，本地标成已取消并从持久化中移除
  for (const s of sessionStates.values()) {
    dismissDialog(s, false);
    persistPendingCards(s);
  }
  scheduleReconnect();
}

function handleDisconnected() {
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
}

function handleError(data: EventPayload, instanceId: string | null) {
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
}

function handleAgentStart(_data: EventPayload, instanceId: string | null) {
  const s = getOrCreateState(instanceId);
  s.isStreaming = true;
  s.currentAssistantContent = "";
  s.currentThinking = "";
  s.toolExecutions = [];
  // 新一轮生成：重置无进展计时与提示标记，启动 watchdog
  s.lastProgressAt = Date.now();
  s.warnedNoOutput = false;
  ensureWatchdog();
}

function handleAgentEnd(data: EventPayload, instanceId: string | null) {
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
}

function handleMessageUpdate(data: EventPayload, instanceId: string | null) {
  const s = getState(instanceId);
  if (!s) return;
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
}

function handleMessageEnd(data: EventPayload, instanceId: string | null) {
  const s = getState(instanceId);
  if (!s) return;
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
}

function handleTurnEnd(_data: EventPayload, instanceId: string | null) {
  const s = getState(instanceId);
  if (!s) return;
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
}

function handleQueueUpdate(data: EventPayload, instanceId: string | null) {
  const s = getState(instanceId);
  if (!s) return;
  s.queue = {
    steering: Array.isArray(data.steering) ? (data.steering as string[]) : [],
  };
}

function handleToolExecutionStart(data: EventPayload, instanceId: string | null) {
  const s = getState(instanceId);
  if (!s) return;
  const toolCallId = data.toolCallId as string || `tool-${Date.now()}`;
  const toolName = data.toolName as string || "Tool";
  const args = (data.args as Record<string, unknown>) || {};
  s.toolExecutions = [...s.toolExecutions, { toolCallId, toolName, args, status: "pending" }];
  s.lastProgressAt = Date.now();
}

function handleToolExecutionUpdate(data: EventPayload, instanceId: string | null) {
  const s = getState(instanceId);
  if (!s) return;
  const toolCallId = data.toolCallId as string;
  const partialResult = data.partialResult;
  s.toolExecutions = s.toolExecutions.map((te) =>
    te.toolCallId === toolCallId
      ? { ...te, status: "streaming" as const, output: formatToolOutput(partialResult) }
      : te,
  );
  s.lastProgressAt = Date.now();
}

function handleToolExecutionEnd(data: EventPayload, instanceId: string | null) {
  const s = getState(instanceId);
  if (!s) return;
  const toolCallId = data.toolCallId as string;
  const result = data.result;
  const isError = data.isError as boolean || false;
  s.toolExecutions = s.toolExecutions.map((te) =>
    te.toolCallId === toolCallId
      ? { ...te, status: isError ? "error" as const : "complete" as const, output: formatToolOutput(result), isError }
      : te,
  );
  s.lastProgressAt = Date.now();
}

// ── 失败可见性（BUG-013）：provider 故障 / 重试 / 扩展错误不再静默 ──
function handleAutoRetryStart(data: EventPayload, instanceId: string | null) {
  const s = getOrCreateState(instanceId);
  const attempt = (data.attempt as number) || 0;
  const maxAttempts = (data.maxAttempts as number) || 0;
  const delayMs = (data.delayMs as number) || 0;
  const errMsg = (data.errorMessage as string) || "provider request failed";
  const suffix = delayMs > 0 ? `（${Math.round(delayMs / 1000)}s 后重试）` : "";
  addMessage(s, "system", `[Retry ${attempt}/${maxAttempts}] ${errMsg}${suffix}`);
  s.lastProgressAt = Date.now();
}

function handleAutoRetryEnd(data: EventPayload, instanceId: string | null) {
  const s = getOrCreateState(instanceId);
  if (data.success === true) return; // 重试成功，不打扰
  const finalError = (data.finalError as string) || "generation failed after retries";
  addMessage(s, "system", `[Error] ${finalError}`);
  s.isStreaming = false;
  s.currentAssistantContent = "";
  s.currentThinking = "";
  s.toolExecutions = [];
  s.warnedNoOutput = false;
  handleRunSettled(s);
}

function handleExtensionError(data: EventPayload, instanceId: string | null) {
  const s = getOrCreateState(instanceId);
  const err = (data.error as string) || "extension error";
  const evt = (data.event as string) || "";
  addMessage(s, "system", `[Extension Error] ${err}${evt ? ` (${evt})` : ""}`);
}

// ── 启动期诊断（broker 从 pi stderr 解析，见 broker/process.rs）──
function handleExtensionLoadFailed(data: EventPayload, instanceId: string | null) {
  // 扩展加载失败（pi 仍可能正常启动，但缺失该扩展能力）；最常见诱因是扩展与 pi 版本不匹配
  const extPath = (data.extensionPath as string) || "";
  const err = (data.error as string) || "unknown error";
  recordSessionWarning(
    instanceId,
    `[Extension Load Failed] ${err}${extPath ? ` (${extPath})` : ""} — ${i18n.global.t("chat.startupVersionHint")}`,
  );
}

function handlePiStartupFailed(data: EventPayload, instanceId: string | null) {
  // pi 在启动宽限期内自行退出（如扩展加载失败导致无法启动）
  const s = getOrCreateState(instanceId);
  s.isStreaming = false;
  const err = (data.error as string) || "unknown error";
  recordSessionWarning(
    instanceId,
    `[Pi Failed to Start] ${err} — ${i18n.global.t("chat.startupVersionHint")}`,
  );
  pushNotify("error", `${i18n.global.t("chat.piStartupFailed")} — ${i18n.global.t("chat.startupVersionHint")}`);
}

// ── 交互型扩展 UI 子协议（pi docs/rpc.md）────────────────
// 阻塞方法（select/confirm/input/editor）：以卡片形式嵌入该会话消息流，
// pi 阻塞等待应答（用户回到该会话作答后回 extension_ui_response）；
// 即发即弃方法（notify/setStatus/setWidget/setTitle/set_editor_text）：不阻塞。
function handleExtensionUiRequest(data: EventPayload, instanceId: string | null) {
  const s = getOrCreateState(instanceId);
  const method = (data.method as string) || "";
  const requestId = (data.id as string) || "";

  if (method === "notify") {
    const ntype = (data.notifyType as string) || "info";
    pushNotify(
      ntype === "error" ? "error" : ntype === "warning" ? "warning" : "info",
      (data.message as string) || "",
    );
    return;
  }
  if (method === "setStatus" || method === "setWidget" || method === "setTitle" || method === "set_editor_text") {
    // 状态条 / 小组件 / 标题暂不渲染，记录日志即可（后续可扩展）
    console.log("[extension_ui]", method, data);
    return;
  }
  if (!requestId || !["select", "confirm", "input", "editor"].includes(method)) return;

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
}

function handleSessionsList(data: EventPayload) {
  const raw = data.projects as Array<Record<string, unknown>> || [];
  wsSessions.value = mapProjectGroups(raw);
}

function handleSessionStatus(data: EventPayload) {
  sessionStatus.value = (data.status as "running" | "idle") || null;
}

function handleResponse(data: EventPayload, instanceId: string | null) {
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
    return;
  }
  // 模型切换失败也走现有 system 消息链路提示（失败时 prompt 仍会用旧模型继续）
  if ((cmd === "set_model" || cmd === "cycle_model") && data.success === false) {
    const s = getOrCreateState(instanceId);
    const errText = (data.error as string) || "unknown";
    addMessage(s, "system", i18n.global.t("chat.modelSwitchFailed", { msg: errText }));
    return;
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
}

// ─── 分发表（新增事件类型只需注册 handler，不再改主函数）────────────────

const handlers: Record<string, Handler> = {
  pi_started: handlePiStarted,
  pi_exited: handlePiExited,
  disconnected: handleDisconnected,
  error: handleError,
  agent_start: handleAgentStart,
  agent_end: handleAgentEnd,
  message_update: handleMessageUpdate,
  message_end: handleMessageEnd,
  turn_end: handleTurnEnd,
  queue_update: handleQueueUpdate,
  tool_execution_start: handleToolExecutionStart,
  tool_execution_update: handleToolExecutionUpdate,
  tool_execution_end: handleToolExecutionEnd,
  auto_retry_start: handleAutoRetryStart,
  auto_retry_end: handleAutoRetryEnd,
  extension_error: handleExtensionError,
  extension_load_failed: handleExtensionLoadFailed,
  pi_startup_failed: handlePiStartupFailed,
  extension_ui_request: handleExtensionUiRequest,
  sessions_list: handleSessionsList,
  session_status: handleSessionStatus,
  response: handleResponse,
};

// ─── 主分发器 ─────────────────────────────────────────────────────────────

export function handlePiEvent(raw: Record<string, unknown>) {
  // ── Broker-level meta events ──
  if (raw.type === "capabilities") return;
  if (raw.type === "control_response") return;
  if (raw.type === "command_undeliverable") {
    handleCommandUndeliverable(raw);
    return;
  }

  // ── Session snapshot (from gateway, not pi) ──
  if (raw.type === "session_snapshot") {
    handleSessionSnapshot(raw);
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
  handlers[data.type as string]?.(data, instanceId);
}
