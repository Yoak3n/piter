import { activeInstanceId } from "./useSessionStore";
import { lifecycleHandlers } from "./piEvents/lifecycle";
import { toolHandlers } from "./piEvents/tools";
import { extensionHandlers } from "./piEvents/extensions";
import { metaHandlers, handleMetaBeforeDispatch } from "./piEvents/meta";

// ─── 事件分发表 + 主分发器 ─────────────────────────────────────────────
// handler 按事件域分组在 piEvents/ 子模块（lifecycle/tools/extensions/meta），
// 每个域导出 Record<type, Handler>；新增事件类型只需在对应域注册，不改主函数。
// 共享类型在此导出，各域模块 import type 引用（避免类型循环定义）。

export type EventPayload = Record<string, unknown>;
export type Handler = (data: EventPayload, instanceId: string | null) => void;

const handlers: Record<string, Handler> = {
  ...lifecycleHandlers,
  ...toolHandlers,
  ...extensionHandlers,
  ...metaHandlers,
};

/** 主分发器：先处理网关 meta 事件，再解包事件信封并按 data.type 分发。 */
export function handlePiEvent(raw: Record<string, unknown>) {
  // ── Broker-level meta events（capabilities/control_response/
  //     command_undeliverable/session_snapshot）──
  if (handleMetaBeforeDispatch(raw)) return;

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
