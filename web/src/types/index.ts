/** Tool execution tracking */
export interface ToolExecution {
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  status: "pending" | "streaming" | "complete" | "error";
  output?: string;
  isError?: boolean;
}

/** Chat message */
export interface Message {
  id: number;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
  thinking?: string;
  toolExecutions?: ToolExecution[];
  meta?: Record<string, unknown>;
  timestamp: number;
}

/** Session metadata */
export interface SessionInfo {
  id: string;
  label: string;
  createdAt: string;
  filePath: string;
  updatedAt: number;
  preview: string;
  cwd: string;
  instanceId?: string;
  state?: "active" | "idle" | "unloaded";
  model?: string;
  thinkingLevel?: string;
  messageCount?: number;
  messageSeq?: number;
}

/** A project grouping sessions */
export interface ProjectGroup {
  path: string;
  name: string;
  sessions: SessionInfo[];
}

/** Model metadata from the backend */
export interface ModelInfo {
  id: string;
  provider?: string;
  contextWindow?: number;
}
