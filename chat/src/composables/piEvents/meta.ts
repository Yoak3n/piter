import { i18n } from "../../i18n";
import { mapProjectGroups } from "../../utils/projects";
import type { SlashCommand } from "../../types";
import {
  getOrCreateState,
  getState,
  addMessage,
  loadMessagesIntoSession,
  activeInstanceId,
  wsSessions,
} from "../useSessionStore";
import { sessionStatus } from "../usePiConnection";
import type { EventPayload, Handler } from "../usePiEvents";

// ─── 网关 meta 与响应域（command_undeliverable / session_snapshot /
// sessions_list / session_status / response）───────────────────────────

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

// ─── Broker-level meta events（先于 envelope 解包处理）────────────────────
// 返回 true 表示已处理（主分发器应提前返回）。

export function handleMetaBeforeDispatch(raw: Record<string, unknown>): boolean {
  if (raw.type === "capabilities") return true;
  if (raw.type === "control_response") return true;
  if (raw.type === "command_undeliverable") {
    handleCommandUndeliverable(raw);
    return true;
  }
  // Session snapshot (from gateway, not pi)
  if (raw.type === "session_snapshot") {
    handleSessionSnapshot(raw);
    return true;
  }
  return false;
}

export const metaHandlers: Record<string, Handler> = {
  sessions_list: handleSessionsList,
  session_status: handleSessionStatus,
  response: handleResponse,
};
