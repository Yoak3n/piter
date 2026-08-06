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

/** Extension UI request card embedded in the message stream (pi extension_ui_request).
 *  阻塞方法（select/confirm/input/editor）以卡片形式进入会话消息流；
 *  未应答前 pi 阻塞等待，用户回到该会话点选/输入后回传 extension_ui_response。
 *  应答后卡片保留为只读历史。 */
export interface ExtensionUiCard {
  id: string;
  method: "select" | "confirm" | "input" | "editor";
  title: string;
  options?: string[];
  message?: string;
  placeholder?: string;
  prefill?: string;
  /** True once answered / cancelled — the card becomes read-only history. */
  answered: boolean;
  result?: { kind: "value" | "confirmed" | "rejected" | "cancelled"; text?: string };
  /** 协议 timeout（毫秒，pi rpc.md：agent 到点自动以 undefined 解析；客户端定时器仅为 UI 显示） */
  timeout?: number;
  /** 卡片创建时间（ms），跨快照恢复时用于计算剩余超时 */
  createdAt?: number;
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
  /** 内嵌的扩展 UI 请求卡片（仅 role="system" 且 content 为空时携带） */
  extUi?: ExtensionUiCard;
  meta?: Record<string, unknown>;
  timestamp: number;
}

/** A logical group of messages sharing a user turn */
export interface ChatTurn {
  id: number;
  user: Message | null;
  assistants: Message[];
  tools: Message[];
  /** 多条 system 消息并存（[Error] 提示、扩展 UI 卡片等可能连续出现） */
  system: Message[];
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
