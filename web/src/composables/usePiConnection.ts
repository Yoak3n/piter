import { ref, onUnmounted } from "vue";

export interface ToolExecution {
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  status: "pending" | "streaming" | "complete" | "error";
  output?: string;
  isError?: boolean;
}

export interface Message {
  id: number;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
  thinking?: string;
  toolExecutions?: ToolExecution[];
  meta?: Record<string, unknown>;
  timestamp: number;
}

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

export interface ProjectGroup {
  path: string;
  dirName: string;
  sessions: SessionInfo[];
}

export function usePiConnection() {
  const messages = ref<Message[]>([]);
  const isRunning = ref(false);
  const isStreaming = ref(false);
  const statusText = ref("Connecting...");
  const currentAssistantContent = ref("");
  const currentThinking = ref("");
  const toolExecutions = ref<ToolExecution[]>([]);
  /** Active pi instance ID (our UUID, not pi's sessionFile). */
  const activeInstanceId = ref<string | null>(null);
  const wsSessions = ref<ProjectGroup[]>([]);
  const sessionStatus = ref<"running" | "idle" | null>(null);
  const currentModel = ref<string>("");

  let msgId = 0;
  let ws: WebSocket | null = null;
  let reconnectAttempts = 0;
  const MAX_RECONNECT_ATTEMPTS = 3;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  function getWsUrl(): string {
    const params = new URLSearchParams(window.location.search);
    const brokerWs = params.get("brokerWs");
    if (brokerWs) return brokerWs;
    const port = window.location.port;
    return `ws://${window.location.hostname}:${port}/ws`;
  }

  function addMessage(role: Message["role"], content: string, extras?: Partial<Message>) {
    messages.value = [
      ...messages.value,
      { id: msgId++, role, content, timestamp: Date.now(), ...extras },
    ];
  }

  function extractTextContent(msg: Record<string, unknown>): string {
    if (typeof msg.content === "string") return msg.content;
    if (Array.isArray(msg.content)) {
      return (msg.content as Record<string, unknown>[])
        .filter((b) => b.type === "text")
        .map((b) => b.text as string)
        .join("\n");
    }
    return "";
  }

  function extractThinkingContent(msg: Record<string, unknown>): string {
    if (Array.isArray(msg.content)) {
      return (msg.content as Record<string, unknown>[])
        .filter((b) => b.type === "thinking")
        .map((b) => b.thinking as string)
        .join("\n");
    }
    return "";
  }

  function formatToolOutput(result: unknown): string {
    if (!result) return "";
    if (typeof result === "string") return result;
    const r = result as Record<string, unknown>;
    if (r.content && Array.isArray(r.content)) {
      return (r.content as Record<string, unknown>[])
        .map((b) => (b.type === "text" ? (b.text as string) : JSON.stringify(b)))
        .join("\n");
    }
    try { return JSON.stringify(result, null, 2); } catch { return String(result); }
  }

  function setActiveInstanceId(id: string | null) {
    activeInstanceId.value = id;
  }

  // ─── Event Handler ─────────────────────────────────────────────────

  function handlePiEvent(raw: Record<string, unknown>) {
    // ── Broker-level meta events ──
    if (raw.type === "capabilities") return;
    if (raw.type === "control_response") return;
    if (raw.type === "command_undeliverable") {
      const reason = raw.reason as string || "unknown";
      const command = raw.command as string || "unknown";
      addMessage("system", `[Delivery Error] Command "${command}" could not be delivered: ${reason}`);
      isStreaming.value = false;
      return;
    }

    // ── Session snapshot (from gateway, not pi) ──
    if (raw.type === "session_snapshot") {
      const iid = raw.instanceId as string;
      if (iid) {
        activeInstanceId.value = iid;
      }
      const msgs = raw.messages as Array<Record<string, unknown>> | undefined;
      if (Array.isArray(msgs) && msgs.length > 0) {
        loadMessagesFromSnapshot(msgs);
      }
      return;
    }

    // ── Filter by active instance ──
    const eventInstanceId = raw.instanceId as string | undefined;
    if (eventInstanceId && activeInstanceId.value && eventInstanceId !== activeInstanceId.value) {
      return;
    }

    // ── Unwrap the event envelope ──
    let data: Record<string, unknown>;
    if (raw.type === "event" && raw.event) {
      data = raw.event as Record<string, unknown>;
    } else if (raw.payload && typeof raw.payload === "object") {
      data = raw.payload as Record<string, unknown>;
    } else {
      data = raw;
    }

    switch (data.type) {
      case "pi_started":
        isRunning.value = true;
        statusText.value = "Connected";
        break;

      case "pi_exited":
      case "disconnected":
        isRunning.value = false;
        isStreaming.value = false;
        statusText.value = "Disconnected";
        scheduleReconnect();
        break;

      case "error":
        addMessage("system", `[Error] ${data.error}`);
        break;

      case "agent_start":
        isStreaming.value = true;
        currentAssistantContent.value = "";
        currentThinking.value = "";
        toolExecutions.value = [];
        break;

      case "agent_end": {
        const msgs = data.messages as Array<Record<string, unknown>> | undefined;
        if (Array.isArray(msgs)) {
          for (const m of msgs) {
            const modelId = m.model as string | undefined;
            if (modelId) {
              currentModel.value = modelId;
              break;
            }
          }
        }
        isStreaming.value = false;
        if (currentThinking.value || currentAssistantContent.value || toolExecutions.value.length > 0) {
          addMessage("assistant", currentAssistantContent.value, {
            thinking: currentThinking.value || undefined,
            toolExecutions: toolExecutions.value.length > 0 ? [...toolExecutions.value] : undefined,
          });
          currentAssistantContent.value = "";
          currentThinking.value = "";
          toolExecutions.value = [];
        }
        break;
      }

      case "message_update": {
        const evt = data.assistantMessageEvent as Record<string, unknown> | undefined;
        if (evt?.type === "text_delta") {
          currentAssistantContent.value += (evt.delta as string) || "";
        } else if (evt?.type === "thinking_delta") {
          currentThinking.value += (evt.delta as string) || "";
        }
        break;
      }

      case "message_end": {
        const msg = data.message as Record<string, unknown> | undefined;
        if (msg?.model) {
          currentModel.value = msg.model as string;
        }
        if (msg?.role === "assistant") {
          const content = extractTextContent(msg);
          const thinking = extractThinkingContent(msg);
          addMessage("assistant", content || currentAssistantContent.value, {
            thinking: thinking || currentThinking.value || undefined,
            toolExecutions: toolExecutions.value.length > 0 ? [...toolExecutions.value] : undefined,
          });
          currentAssistantContent.value = "";
          currentThinking.value = "";
          toolExecutions.value = [];
        }
        break;
      }

      case "turn_end": {
        if (currentThinking.value || currentAssistantContent.value || toolExecutions.value.length > 0) {
          addMessage("assistant", currentAssistantContent.value, {
            thinking: currentThinking.value || undefined,
            toolExecutions: toolExecutions.value.length > 0 ? [...toolExecutions.value] : undefined,
          });
          currentAssistantContent.value = "";
          currentThinking.value = "";
          toolExecutions.value = [];
        }
        break;
      }

      case "tool_execution_start": {
        const toolCallId = data.toolCallId as string || `tool-${Date.now()}`;
        const toolName = data.toolName as string || "Tool";
        const args = (data.args as Record<string, unknown>) || {};
        toolExecutions.value = [...toolExecutions.value, { toolCallId, toolName, args, status: "pending" }];
        break;
      }
      case "tool_execution_update": {
        const toolCallId = data.toolCallId as string;
        const partialResult = data.partialResult;
        toolExecutions.value = toolExecutions.value.map((te) =>
          te.toolCallId === toolCallId
            ? { ...te, status: "streaming" as const, output: formatToolOutput(partialResult) }
            : te,
        );
        break;
      }
      case "tool_execution_end": {
        const toolCallId = data.toolCallId as string;
        const result = data.result;
        const isError = data.isError as boolean || false;
        toolExecutions.value = toolExecutions.value.map((te) =>
          te.toolCallId === toolCallId
            ? { ...te, status: isError ? "error" as const : "complete" as const, output: formatToolOutput(result), isError }
            : te,
        );
        break;
      }

      case "sessions_list":
        wsSessions.value = (data.projects as ProjectGroup[]) || [];
        break;

      case "session_status":
        sessionStatus.value = (data.status as "running" | "idle") || null;
        break;

      case "response": {
        const cmd = data.command as string;
        if (cmd === "new_session" && data.success) {
          const iid = data.instanceId as string | undefined;
          if (iid) {
            activeInstanceId.value = iid;
          }
        }
        if (cmd === "get_state" && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          const model = d?.model as Record<string, unknown> | undefined;
          if (model?.id) {
            currentModel.value = model.id as string;
          }
        }
        if ((cmd === "set_model" || cmd === "cycle_model") && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          const model = (d?.model as Record<string, unknown>) || (d as Record<string, unknown> | undefined);
          if (model?.id) {
            currentModel.value = model.id as string;
          }
        }
        break;
      }
    }
  }

  // ─── Snapshot Loading ───────────────────────────────────────────────

  function loadMessagesFromSnapshot(msgs: Array<Record<string, unknown>>) {
    const parsed: Message[] = [];
    for (let i = 0; i < msgs.length; i++) {
      const m = msgs[i];
      const role = (m.role as Message["role"]) || "assistant";
      const toolExecs: ToolExecution[] = [];
      if (Array.isArray(m.content)) {
        for (const block of m.content as Record<string, unknown>[]) {
          if (block.type === "tool_use") {
            toolExecs.push({
              toolCallId: block.id as string || `tool-${i}`,
              toolName: block.name as string || "Tool",
              args: (block.input as Record<string, unknown>) || {},
              status: "complete",
            });
          }
        }
      }
      parsed.push({
        id: i,
        role,
        content: extractTextContent(m),
        thinking: extractThinkingContent(m) || undefined,
        toolExecutions: toolExecs.length > 0 ? toolExecs : undefined,
        timestamp: (m.timestamp as number) || Date.now(),
      });
    }
    messages.value = parsed;
    msgId = parsed.length;
    currentAssistantContent.value = "";
    currentThinking.value = "";
    toolExecutions.value = [];
  }

  // ─── WebSocket ──────────────────────────────────────────────────────

  function connectWebSocket() {
    const url = getWsUrl();
    statusText.value = "Connecting...";
    ws = new WebSocket(url);
    ws.onopen = () => {
      isRunning.value = true;
      statusText.value = "Connected";
      reconnectAttempts = 0;
    };
    ws.onmessage = (e) => {
      try {
        const data = JSON.parse(e.data);
        handlePiEvent(data);
      } catch {
        addMessage("system", `[raw] ${e.data}`);
      }
    };
    ws.onclose = () => {
      isRunning.value = false;
      statusText.value = "Disconnected";
      scheduleReconnect();
    };
    ws.onerror = () => {
      ws?.close();
    };
  }

  function scheduleReconnect() {
    if (reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
      reconnectAttempts++;
      const delay = reconnectAttempts * 3000;
      statusText.value = `Reconnecting in ${reconnectAttempts}s...`;
      reconnectTimer = setTimeout(() => connectWebSocket(), delay);
    } else {
      statusText.value = "Connection failed";
      addMessage("system", `WebSocket disconnected after ${MAX_RECONNECT_ATTEMPTS} retries. Reload to reconnect.`);
    }
  }

  // ─── Commands ───────────────────────────────────────────────────────

  function sendPrompt(text: string) {
    if (!text.trim()) return;
    addMessage("user", text);
    if (ws && ws.readyState === WebSocket.OPEN) {
      const iid = activeInstanceId.value;
      if (!iid) {
        addMessage("system", "No active session yet — please wait for the session to be ready.");
        return;
      }
      ws.send(JSON.stringify({
        type: "broker_command",
        instanceId: iid,
        payload: { type: "prompt", message: text },
      }));
    }
  }

  function sendCommand(cmd: Record<string, unknown>) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      const iid = activeInstanceId.value;
      if (iid) {
        ws.send(JSON.stringify({
          type: "broker_command",
          instanceId: iid,
          payload: cmd,
        }));
      } else {
        ws.send(JSON.stringify(cmd));
      }
      return true;
    }
    return false;
  }

  function switchSession(instanceId: string) {
    clearMessages();
    activeInstanceId.value = instanceId;
    sendCommand({ type: "switch_session", instanceId });
  }

  function restartPi() {
    reconnectAttempts = 0;
    ws?.close();
    setTimeout(() => connectWebSocket(), 500);
  }

  function disconnect() {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    ws?.close();
    ws = null;
  }

  function loadHistory(history: Message[]) {
    messages.value = history;
    msgId = history.length;
  }

  function clearMessages() {
    messages.value = [];
    msgId = 0;
    currentAssistantContent.value = "";
    currentThinking.value = "";
    toolExecutions.value = [];
    currentModel.value = "";
  }

  onUnmounted(() => {
    disconnect();
  });

  return {
    messages,
    isRunning,
    isStreaming,
    statusText,
    currentAssistantContent,
    currentThinking,
    toolExecutions,
    activeInstanceId,
    wsSessions,
    sessionStatus,
    currentModel,
    connectWebSocket,
    sendPrompt,
    sendCommand,
    switchSession,
    setActiveInstanceId,
    restartPi,
    disconnect,
    loadHistory,
    clearMessages,
  };
}
