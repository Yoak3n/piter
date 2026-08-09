import { i18n } from "../../i18n";
import type { ExtensionUiCard } from "../../types";
import { getOrCreateState, addMessage } from "../useSessionStore";
import { pushNotify, recordSessionWarning } from "../usePiNotify";
import { answerCard, persistPendingCards } from "../useExtensionCards";
import type { EventPayload, Handler } from "../usePiEvents";

// ─── 扩展与启动诊断域 handler（extension_error / load_failed /
// pi_startup_failed / extension_ui_request）───────────────────────────

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
  if (!requestId || !["select", "confirm", "input", "editor"].includes(method)) {
    // 未知/缺 id 的阻塞型请求：pi 会阻塞等待应答，静默丢弃会永久卡住该会话。
    // 打印原始载荷便于排查（如 pi 新 method 未接入）。
    console.warn("[extension_ui] unhandled blocking request:", method, data);
    return;
  }

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

export const extensionHandlers: Record<string, Handler> = {
  extension_error: handleExtensionError,
  extension_load_failed: handleExtensionLoadFailed,
  pi_startup_failed: handlePiStartupFailed,
  extension_ui_request: handleExtensionUiRequest,
};
