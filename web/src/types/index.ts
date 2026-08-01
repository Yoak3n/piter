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

/** A logical group of messages sharing a user turn */
export interface ChatTurn {
  id: number;
  user: Message | null;
  assistants: Message[];
  tools: Message[];
  system: Message | null;
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
  state?: "idle" | "busy" | "waiting_review" | "unloaded";
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

/** Model reference (id + provider pair) used for model switching */
export interface ModelRef {
  id: string;
  provider?: string;
}

/** Model metadata from the backend */
export interface ModelInfo {
  id: string;
  provider?: string;
  contextWindow?: number;
}
