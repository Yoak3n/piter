/** Tool execution tracking */
export interface ToolExecution {
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  status: "pending" | "streaming" | "complete" | "error";
  output?: string;
  isError?: boolean;
}

/**
 * Image content block, mirroring the pi protocol:
 * `{ type: "image", data, mimeType }`.
 * `data` is the **raw base64 payload** (no `data:` prefix) — pi re-adds the
 * `data:<mime>;base64,` prefix itself when building provider requests.
 * Sent via `Command::Prompt/Steer/FollowUp.images` and received in
 * `ContentBlock::Image` (extracted from message content blocks).
 */
export interface ImageContent {
  type: "image";
  /** Pure base64 image data (no data-URI prefix). */
  data: string;
  mimeType: string;
}

/** A pending attachment in the composer (image or text file). */
export interface Attachment {
  id: string;
  type: "image" | "text";
  fileName: string;
  mimeType: string;
  /** Base64 data URI for images (compressed). */
  data?: string;
  /** Text content for text files (may be truncated at read time). */
  content?: string;
  /** Original file size in bytes. */
  size: number;
  /** True when the text content was truncated (>200KB). */
  truncated?: boolean;
}

/** Chat message */
export interface Message {
  id: number;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
  thinking?: string;
  /** Images attached to this message (user-sent or assistant content blocks). */
  images?: ImageContent[];
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
  /** Model id this session is using (runtime or persisted in DB). */
  model?: string;
  /** Provider for `model` (persisted alongside it). */
  modelProvider?: string;
  thinkingLevel?: string;
  messageCount?: number;
  messageSeq?: number;
}

/** A project grouping sessions */
export interface ProjectGroup {
  /** Database project id; undefined for the synthetic "Other" group */
  id?: string;
  path: string;
  name: string;
  /** 1 when pinned (backend sorts pinned first) */
  pinned?: number;
  /** Whether the project is archived (hidden from the default list) */
  archived?: boolean;
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
  /** pi 模型库声明的输入模态（含 "image" 表示支持图片输入） */
  input?: string[];
}
