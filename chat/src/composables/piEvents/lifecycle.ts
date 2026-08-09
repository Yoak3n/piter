import { i18n } from "../../i18n";
import type { ImageContent } from "../../types";
import { extractTextContent, extractThinkingContent, extractImages } from "../../utils/message";
import {
  getOrCreateState,
  getState,
  addMessage,
  handleRunSettled,
  sessionStates,
  activeInstanceId,
} from "../useSessionStore";
import { notifySessionCompleted } from "../usePiNotify";
import { dismissDialog, persistPendingCards } from "../useExtensionCards";
import { isRunning, statusText, scheduleReconnect, ensureWatchdog } from "../usePiConnection";
import type { EventPayload, Handler } from "../usePiEvents";

// ─── 会话生命周期与消息域 handler（pi_started/exited/disconnected/error、
// agent_start/end、message_update/end、turn_end、queue_update）────────────

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

export const lifecycleHandlers: Record<string, Handler> = {
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
};
