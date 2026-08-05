import type { ImageContent, ToolExecution } from "../types";

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

/**
 * Extract image content blocks (`{ type: "image", data, mimeType }`) from a
 * message's content array. `extractTextContent` intentionally drops these,
 * so rendering image blocks needs this separate pass.
 */
export function extractImages(
  msg: Record<string, unknown> | { content: unknown },
): ImageContent[] {
  if (!Array.isArray(msg.content)) return [];
  const images: ImageContent[] = [];
  for (const b of msg.content as Record<string, unknown>[]) {
    if (b.type === "image" && typeof b.data === "string" && b.data) {
      images.push({
        type: "image",
        data: b.data,
        mimeType: typeof b.mimeType === "string" ? b.mimeType : "",
      });
    }
  }
  return images;
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

function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/** Format an epoch-ms timestamp for display (QQ-style: today → HH:MM,
 * yesterday → Yesterday HH:MM, same year → M/D HH:MM, older → YYYY/M/D HH:MM). */
export function formatMessageTime(ts: number): string {
  const d = new Date(ts);
  const now = new Date();
  const hm = `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  const startOfDay = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dayDiff = Math.round((startOfDay(now) - startOfDay(d)) / 86400000);
  if (dayDiff <= 0) return hm;
  if (dayDiff === 1) return `Yesterday ${hm}`;
  const md = `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
  if (d.getFullYear() === now.getFullYear()) return md;
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${hm}`;
}
