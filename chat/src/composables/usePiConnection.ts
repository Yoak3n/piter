import { onUnmounted, ref } from "vue";
import { i18n } from "../i18n";
import type { Message, ModelRef } from "../types";
import {
  activeInstanceId,
  activeSessionState,
  getOrCreateState,
  getState,
  sessionStates,
  messages,
  isStreaming,
  currentAssistantContent,
  currentThinking,
  toolExecutions,
  currentModel,
  steeringQueue,
  outbox,
  slashCommands,
  wsSessions,
  addMessage,
  setCurrentModel,
  sendPrompt,
  cancelQueued,
  upgradeQueued,
  flushAfterAbort,
  loadHistory,
  setActiveInstanceId,
  registerTransport,
} from "./useSessionStore";
import { notifications, startBudgetPolling, stopBudgetPolling } from "./usePiNotify";
import {
  respondExtensionDialog,
  readStoredPendingCards,
  writeStoredPendingCards,
  dismissDialog,
} from "./useExtensionCards";
import { handlePiEvent } from "./usePiEvents";

// 对外类型兼容重导出（App.vue / Composer.vue / ChatPane.vue 从本文件导入类型）
export type { PendingItem, DeliveryBehavior } from "./useSessionStore";
export type { ExtensionNotify } from "./useExtensionCards";

// ─── No-progress watchdog（BUG-013：pi 卡死时兜底提示）────────────
const WATCHDOG_INTERVAL_MS = 15_000;
const WATCHDOG_NO_PROGRESS_MS = 90_000;
let watchdogTimer: ReturnType<typeof setInterval> | null = null;

// Tauri 窗口生命周期监听防重复注册（参考 watchdogTimer 单例写法）
let windowLifecycleRegistered = false;

// ── 全局连接状态（模块级单例）──
export const isRunning = ref(false);
export const statusText = ref(i18n.global.t("chat.connecting"));
export const sessionStatus = ref<"running" | "idle" | null>(null);

let ws: WebSocket | null = null;
let reconnectAttempts = 0;
const MAX_RECONNECT_ATTEMPTS = 3;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

// ── gateway_command（WS 上的 REST 式一问一答，撤回能力探测等用）──
interface GatewayResponse {
  requestId: string;
  success: boolean;
  data?: Record<string, unknown>;
  error?: string;
}
const gatewayPending = new Map<string, (r: GatewayResponse) => void>();
let gatewayReqSeq = 0;

/** 发送 gateway_command 并等待 gateway_response（requestId 关联）。 */
export function sendGatewayCommand(
  command: string,
  data: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    const requestId = `gw-${++gatewayReqSeq}-${Date.now()}`;
    gatewayPending.set(requestId, (r) => {
      if (r.success) resolve(r.data ?? {});
      else reject(new Error(r.error ?? "gateway command failed"));
    });
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "gateway_command", requestId, command, data }));
    } else {
      gatewayPending.delete(requestId);
      reject(new Error("not connected"));
    }
  });
}

// ── Tauri 窗口隐藏：暂停 WS 重连（BUG-011 衍生）────────────
// 窗口关闭（隐藏到托盘）时主动断 WS → 后端 onclose 立即清理订阅；
// 隐藏期间暂停自动重连，恢复可见时重连（重新订阅）。
let suspendReconnect = false;

// ─── 发送命令 ─────────────────────────────────────────────────────────────

export function sendCommand(cmd: Record<string, unknown>, targetInstanceId?: string): boolean {
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

// ─── WebSocket ──────────────────────────────────────────────────────────────

function getWsUrl(): string {
  const params = new URLSearchParams(window.location.search);
  const brokerWs = params.get("brokerWs");
  // brokerWs 历史格式是 ws://IP:PORT/ws（0.3.0 前）且可能携带旧端口（如 1421）。
  // ① 与当前页面同源（桌面端/App WebView 打开的都是当前服务端）→ 一律以当前
  //    页面 hostname+port 为准，忽略残留的旧 brokerWs 端口/路径；
  // ② 跨源（App 用不同 IP 打开时）→ 用 brokerWs，但路径统一归一化为 /chat-ws。
  if (brokerWs) {
    try {
      const u = new URL(brokerWs);
      if (u.hostname === window.location.hostname && window.location.port) {
        return `ws://${window.location.hostname}:${window.location.port}/chat-ws`;
      }
      u.pathname = "/chat-ws";
      return u.toString().replace(/^https?:/, "ws:");
    } catch {
      /* 非法 URL 走默认 */
    }
  }
  const port = window.location.port;
  // /chat-ws = chat 前端（work 用 /work-ws；/ui-ws 是 admin 管理端路径——
  // path 定前端，gateway 注册表据此分类，chat 不共用 /ui-ws）。
  return `ws://${window.location.hostname}:${port}/chat-ws`;
}

/** 懒启动 watchdog：轮询所有 streaming session，超 90s 无进展提示一次 */
export function ensureWatchdog() {
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

// ── LAN 未授权（WS upgrade 401）提示（0.2.0 P3 审查项）────────
// WS 握手失败页面拿不到 HTTP 状态码；onerror 时轻量探测一个 REST 端点：
// 若返回 401 lan_auth_required → 设备 cookie 已失效（被撤销/过期），
// 提示"需要 PIN"并停止自动重连循环（否则只会无限"已断开"）。
async function checkLanAuthRequired() {
  if (suspendReconnect) return;
  try {
    const res = await fetch("/api/sessions");
    if (res.status === 401) {
      const data = await res.json();
      if (data?.error === "lan_auth_required") {
        statusText.value = i18n.global.t("chat.lanAuthRequired");
        if (reconnectTimer) clearTimeout(reconnectTimer);
        suspendReconnect = true;
      }
    }
  } catch {
    // 网关整体不可达时忽略（不是鉴权问题）
  }
}

export function connectWebSocket() {
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
      // gateway_command 的应答单独结算（带 requestId，不走事件分发）
      if (data.type === "gateway_response") {
        const cb = gatewayPending.get(data.requestId as string);
        if (cb) {
          gatewayPending.delete(data.requestId as string);
          cb(data as GatewayResponse);
        }
        return;
      }
      handlePiEvent(data);
    } catch (err) {
      // 事件处理异常绝不能被静默吞掉：卡片丢失/状态卡死往往源自这里，
      // 打印原始消息便于排查（事件本身独立，不影响后续消息）。
      console.error("[ws] error processing message:", err, e.data);
    }
  };
  ws.onclose = () => {
    isRunning.value = false;
    statusText.value = i18n.global.t("chat.disconnected");
    scheduleReconnect();
  };
  ws.onerror = () => {
    ws?.close();
    // WS upgrade 可能因 LAN 鉴权 401 失败 → 探测并提示（见上）
    void checkLanAuthRequired();
  };
}

export function scheduleReconnect() {
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

// ─── Commands ───────────────────────────────────────────────────────────────

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

// ─── Composables ────────────────────────────────────────────────────────────

export function usePiConnection() {
  // 注册底层发送能力（store 的 outbox/sendPrompt 等经此发送，等价原实现直接访问 ws）
  registerTransport(sendCommand, () => !!ws && ws.readyState === WebSocket.OPEN);
  // 预算提醒轮询：启动即查一次，随后按固定间隔
  startBudgetPolling();

  onUnmounted(() => {
    if (watchdogTimer) {
      clearInterval(watchdogTimer);
      watchdogTimer = null;
    }
    stopBudgetPolling();
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
    sendGatewayCommand,
    fetchSlashCommands,
    respondExtensionDialog,
    abortGeneration,
    cancelQueued,
    upgradeQueued,
    newSession,
    switchSession,
    ackReview,
    setActiveInstanceId,
    restartPi,
    disconnect,
    loadHistory,
    clearMessages,
  };
}
