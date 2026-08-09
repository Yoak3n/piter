import { getOrCreateState, getState, addMessage, handleRunSettled } from "../useSessionStore";
import { formatToolOutput } from "../../utils/message";
import type { EventPayload, Handler } from "../usePiEvents";

// ─── 工具执行与自动重试域 handler（tool_execution_*、auto_retry_*）───────

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

// ── 失败可见性（BUG-013）：provider 故障 / 重试不再静默 ──
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

export const toolHandlers: Record<string, Handler> = {
  tool_execution_start: handleToolExecutionStart,
  tool_execution_update: handleToolExecutionUpdate,
  tool_execution_end: handleToolExecutionEnd,
  auto_retry_start: handleAutoRetryStart,
  auto_retry_end: handleAutoRetryEnd,
};
