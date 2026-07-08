import { ref } from "vue";
import type { Message, ToolExecution } from "./usePiConnection";

export interface SessionInfo {
  id: string;
  label: string;
  created_at: string;
  file_path: string;
  updated_at: number;
  preview: string;
  cwd: string;
}

export interface ProjectGroup {
  path: string;
  dir_name: string;
  sessions: SessionInfo[];
}

export function useSessions() {
  const sessions = ref<ProjectGroup[]>([]);
  const loading = ref(false);

  async function fetchSessions() {
    loading.value = true;
    try {
      const res = await fetch("/api/sessions");
      const data = await res.json();
      sessions.value = data.projects || [];
    } catch (e) {
      console.error("Failed to load sessions:", e);
    } finally {
      loading.value = false;
    }
  }

  async function loadMessages(filePath: string): Promise<Message[]> {
    try {
      const res = await fetch(
        `/api/load-session?path=${encodeURIComponent(filePath)}`,
      );
      const msgs = await res.json();
      if (Array.isArray(msgs)) {
        return msgs
          .map((m: any, i: number) => {
            const thinking = extractMsgThinking(m);
            const toolExecs = extractToolExecutions(m);
            return {
              id: i,
              role: m.role || "assistant",
              content: extractMsgText(m),
              thinking: thinking || undefined,
              toolExecutions: toolExecs.length > 0 ? toolExecs : undefined,
              timestamp: Date.now(),
            };
          })
          .filter(
            (m: any) => m.role === "user" || m.role === "assistant",
          );
      }
    } catch (e) {
      console.error("Failed to load messages:", e);
    }
    return [];
  }

  async function deleteSession(filePath: string): Promise<boolean> {
    try {
      const res = await fetch(
        `/api/delete-session?path=${encodeURIComponent(filePath)}`,
      );
      const data = await res.json();
      return data.success === true;
    } catch (e) {
      console.error("Failed to delete session:", e);
      return false;
    }
  }

  async function createSession(
    cwd?: string,
    name?: string,
  ): Promise<{ id: string; file_path: string } | null> {
    try {
      const res = await fetch("/api/sessions/create", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          cwd: cwd || ".",
          name: name || "New Session",
        }),
      });
      const data = await res.json();
      if (data.success) {
        return { id: data.id, file_path: data.file_path };
      }
    } catch (e) {
      console.error("Failed to create session:", e);
    }
    return null;
  }

  return {
    sessions,
    loading,
    fetchSessions,
    loadMessages,
    deleteSession,
    createSession,
  };
}

function extractMsgText(msg: any): string {
  if (typeof msg.content === "string") return msg.content;
  if (Array.isArray(msg.content)) {
    return msg.content
      .filter((b: any) => b.type === "text")
      .map((b: any) => b.text)
      .join("\n");
  }
  return "";
}

function extractMsgThinking(msg: any): string {
  if (Array.isArray(msg.content)) {
    return msg.content
      .filter((b: any) => b.type === "thinking")
      .map((b: any) => b.thinking)
      .join("\n");
  }
  return "";
}

function extractToolExecutions(msg: any): ToolExecution[] {
  const execs: ToolExecution[] = [];
  if (!Array.isArray(msg.content)) return execs;
  for (const block of msg.content) {
    if (block.type === "tool_use") {
      execs.push({
        toolCallId: block.id || `tool-${execs.length}`,
        toolName: block.name || "Tool",
        args: block.input || {},
        status: "complete",
      });
    } else if (block.type === "tool_result") {
      const match = execs.find((t) => t.toolCallId === block.tool_use_id);
      if (match) {
        const isErr = block.is_error || false;
        match.output = formatToolOutput(block.content);
        match.isError = isErr;
        match.status = isErr ? "error" : "complete";
      }
    }
  }
  return execs;
}

function formatToolOutput(result: any): string {
  if (!result) return "";
  if (typeof result === "string") return result;
  if (result.content && Array.isArray(result.content)) {
    return result.content
      .map((b: any) => (b.type === "text" ? b.text : JSON.stringify(b)))
      .join("\n");
  }
  try { return JSON.stringify(result, null, 2); } catch { return String(result); }
}
