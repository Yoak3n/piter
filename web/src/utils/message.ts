import type { ToolExecution } from "../types";

/** Extract plain text from a message's content field (string or content blocks). */
export function extractTextContent(
  msg: Record<string, unknown> | { content: unknown },
): string {
  if (typeof msg.content === "string") return msg.content;
  if (Array.isArray(msg.content)) {
    return (msg.content as Record<string, unknown>[])
      .filter((b) => b.type === "text")
      .map((b) => b.text as string)
      .join("\n");
  }
  return "";
}

/** Extract thinking text from a message's content blocks. */
export function extractThinkingContent(
  msg: Record<string, unknown> | { content: unknown },
): string {
  if (Array.isArray(msg.content)) {
    return (msg.content as Record<string, unknown>[])
      .filter((b) => b.type === "thinking")
      .map((b) => b.thinking as string)
      .join("\n");
  }
  return "";
}

/** Format a tool result into a displayable string. */
export function formatToolOutput(result: unknown): string {
  if (!result) return "";
  if (typeof result === "string") return result;
  const r = result as Record<string, unknown>;
  if (r.content && Array.isArray(r.content)) {
    return (r.content as Record<string, unknown>[])
      .map((b) =>
        b.type === "text" ? (b.text as string) : JSON.stringify(b),
      )
      .join("\n");
  }
  try {
    return JSON.stringify(result, null, 2);
  } catch {
    return String(result);
  }
}

/** Parse tool_use / tool_result blocks from a message's content array. */
export function extractToolExecutions(
  msg: Record<string, unknown>,
): ToolExecution[] {
  const execs: ToolExecution[] = [];
  if (!Array.isArray(msg.content)) return execs;
  for (const block of msg.content as Record<string, unknown>[]) {
    if (block.type === "tool_use") {
      execs.push({
        toolCallId: (block.id as string) || `tool-${execs.length}`,
        toolName: (block.name as string) || "Tool",
        args: (block.input as Record<string, unknown>) || {},
        status: "complete",
      });
    } else if (block.type === "tool_result") {
      const match = execs.find((t) => t.toolCallId === block.tool_use_id);
      if (match) {
        const isErr = (block.is_error as boolean) || false;
        match.output = formatToolOutput(block.content);
        match.isError = isErr;
        match.status = isErr ? "error" : "complete";
      }
    }
  }
  return execs;
}
