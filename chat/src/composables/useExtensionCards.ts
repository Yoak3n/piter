import type { ExtensionUiCard } from "../types";
import { getState, activeInstanceId } from "./useSessionStore";
import type { SessionState } from "./useSessionStore";
import { sendCommand } from "./usePiConnection";

// ─── 扩展通知类型 ────────────────────────────────────────────────────────

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

// ── 未应答扩展 UI 卡片持久化（P0：切会话/刷新/重连后卡片不丢失，pi 不再永久阻塞）──
// 切会话触发 session_snapshot → messages 整体重建；页面刷新则连 sessionStates 都重置。
// 把未应答卡片按 instanceId 存 localStorage，快照重建时 merge 回消息流，回来仍可作答。
const EXT_UI_STORAGE_KEY = "piter:extUiPending";

export function readStoredPendingCards(): Record<string, ExtensionUiCard[]> {
  try {
    const raw = localStorage.getItem(EXT_UI_STORAGE_KEY);
    return raw ? (JSON.parse(raw) as Record<string, ExtensionUiCard[]>) : {};
  } catch {
    return {};
  }
}

export function writeStoredPendingCards(map: Record<string, ExtensionUiCard[]>) {
  try {
    localStorage.setItem(EXT_UI_STORAGE_KEY, JSON.stringify(map));
  } catch {
    // localStorage 不可用（隐私模式等）时静默降级：卡片仅存于内存
  }
}

/** 把某会话未应答卡片写入 localStorage（已应答/取消的不再持久化） */
export function persistPendingCards(s: SessionState) {
  if (!s.instanceId) return;
  const pending = s.messages
    .filter((m) => m.extUi && !m.extUi.answered)
    .map((m) => m.extUi!);
  const map = readStoredPendingCards();
  if (pending.length > 0) map[s.instanceId] = pending;
  else delete map[s.instanceId];
  writeStoredPendingCards(map);
}

/** 收集会话的未应答卡片（旧内存态 + localStorage 持久化），按 id 去重 */
export function collectPendingCards(s: SessionState): ExtensionUiCard[] {
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

/** 把某张扩展 UI 卡片标记为已应答（本地状态），返回是否命中 */
export function markCardAnswered(s: SessionState, id: string, result: ExtensionUiCard["result"]) {
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
export function answerCard(s: SessionState, id: string, result: ExtensionUiCard["result"], payload: Record<string, unknown>) {
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
export function dismissDialog(s: SessionState, respond: boolean) {
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

/** 用户作答消息流中的扩展 UI 卡片（卡片仅在其所属会话中可见，故从当前活动会话查找） */
export function respondExtensionDialog(id: string, answer: { value?: string; confirmed?: boolean; cancelled?: boolean }) {
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
