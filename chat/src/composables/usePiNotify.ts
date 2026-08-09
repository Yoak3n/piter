import { ref } from "vue";
import { i18n } from "../i18n";
import type { ExtensionNotify } from "./useExtensionCards";
import { getState, addMessage, wsSessions } from "./useSessionStore";

// ── 扩展通知（extension_ui_request notify，即发即弃）──
export const notifications = ref<ExtensionNotify[]>([]);
let notifySeq = 0;
// 通知节流：同内容去重 + 全局限频 + 可见条数上限。
// piolium 等扩展会在 session_start / 阶段进度 / 重试时高频 notify，
// 若全部弹 toast 会形成轰炸；去重窗口内同一条只出现一次（加载时的首次提示仍会显示）。
const TOAST_DEDUP_MS = 30_000; // 同一 (type+message) 在 30s 内不再重复出现
const TOAST_MIN_INTERVAL_MS = 1_000; // 全局至少间隔 1s 才允许新增一条
const TOAST_MAX_VISIBLE = 3;
const recentToasts = new Map<string, number>(); // `${type}:${message}` → lastShownAt
let lastToastAt = 0;

export function pushNotify(
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
export function getSessionLabel(instanceId: string): string {
  for (const g of wsSessions.value) {
    for (const s of g.sessions) {
      if (s.instanceId === instanceId && s.label) return s.label;
    }
  }
  return instanceId.slice(0, 8);
}

/** 会话完成时推送顶部 toast。受 localStorage 开关控制（默认开）。 */
export function notifySessionCompleted(instanceId: string) {
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

// ── 启动期会话诊断（扩展加载失败 / pi 启动失败）──
// 事件可能在会话状态创建之前到达，且会话快照重建会整体替换 messages；
// 先用 Map 暂存，快照重建时重新注入，保证用户打开该会话时仍能看到失败原因。
export const sessionWarnings = new Map<string, string[]>();

export function recordSessionWarning(iid: string | null, text: string) {
  if (!iid) return;
  const arr = sessionWarnings.get(iid) ?? [];
  if (arr.includes(text)) return; // 同一条不重复记录
  sessionWarnings.set(iid, [...arr, text]);
  const s = getState(iid);
  if (s && !s.messages.some((m) => m.role === "system" && m.content === text)) {
    addMessage(s, "system", text);
  }
}

// ── 月度预算提醒（0.2.0 P3）────────────────────────────
// 拉取对比：定时轮询 GET /api/budget/status，与 localStorage 记录的"上次档位"
// 比较，只有跨档上升（如 49%→52%）才 toast（50=info / 80=warning / 100=error）。
// 月初重置后回落（100%→5%）不提醒；budget 未设置 / 拉取失败一律静默（percent 0）。
const BUDGET_TIER_KEY = "piter:budgetTier";
const BUDGET_POLL_MS = 5 * 60 * 1000; // 预算低频，5 分钟轮询足够
let budgetTimer: ReturnType<typeof setInterval> | null = null;
let budgetPollStarted = false;

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

/** 启动预算轮询（usePiConnection 初始化时调用）：立即检查一次，随后按固定间隔轮询 */
export function startBudgetPolling() {
  if (budgetPollStarted) return;
  budgetPollStarted = true;
  void checkBudget();
  budgetTimer = setInterval(checkBudget, BUDGET_POLL_MS);
}

export function stopBudgetPolling() {
  if (budgetTimer) {
    clearInterval(budgetTimer);
    budgetTimer = null;
  }
}
