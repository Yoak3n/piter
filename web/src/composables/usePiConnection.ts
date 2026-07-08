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

export function usePiConnection() {
  const messages = ref<Message[]>([]);
  const isRunning = ref(false);
  const isStreaming = ref(false);
  const statusText = ref("Connecting...");
  const currentAssistantContent = ref("");
  const currentThinking = ref("");
  const toolExecutions = ref<ToolExecution[]>([]);
  const activeSessionPath = ref<string | null>(null);
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

  function setActiveSessionPath(path: string | null) {
    activeSessionPath.value = path;
  }

  function handlePiEvent(data: Record<string, unknown>) {
    // Route events to the correct session when sessionPath metadata is present
    if (data.sessionPath) {
      // Always extract the inner payload first — the real event data is inside
      const inner = data.payload as Record<string, unknown>;
      if (activeSessionPath.value) {
        if (data.sessionPath !== activeSessionPath.value) return;
      }
      // Use the unwrapped payload for event processing
      if (inner) data = inner;
    }
    switch (data.type) {
      case "pi_started":
      case "connected":
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
      case "agent_end":
        // Extract model from embedded messages (the assistant message carries model info)
        {
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
      case "message_update": {
        const evt = data.assistantMessageEvent as
          | Record<string, unknown>
          | undefined;
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
        const te: ToolExecution = {
          toolCallId,
          toolName,
          args,
          status: "pending",
        };
        toolExecutions.value = [...toolExecutions.value, te];
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
      case "raw":
        addMessage("system", `[raw] ${data.data}`);
        break;
      case "sessions_list":
        wsSessions.value = (data.projects as ProjectGroup[]) || [];
        break;
      case "session_status":
        sessionStatus.value = (data.status as "running" | "idle") || null;
        break;
      case "response": {
        const cmd = data.command as string;
        if (cmd === "new_session" && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          if (d?.sessionFile) {
            activeSessionPath.value = d.sessionFile as string;
            // Session created — fetch current model info via WS
            setTimeout(() => sendCommand({ type: "get_state" }), 300);
          }
        }
        if (cmd === "switch_session" && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          if (d?.sessionFile) {
            activeSessionPath.value = d.sessionFile as string;
            // Re-fetch model info for the new session's pi process
            setTimeout(() => sendCommand({ type: "get_state" }), 300);
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
      case "mirror_sync": {
        const entries = data.entries as Array<Record<string, unknown>> | undefined;
        if (entries && Array.isArray(entries)) {
          const msgs: Message[] = [];
          for (let i = 0; i < entries.length; i++) {
            const e = entries[i];
            if (e.type === "message" && e.message) {
              const m = e.message as Record<string, unknown>;
              const role = (m.role as Message["role"]) || "assistant";
              const thinking = extractThinkingContent(m);
              // Extract tool executions from message content blocks
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
                  } else if (block.type === "tool_result") {
                    // Find matching tool exec and set output
                    const matchId = block.tool_use_id as string;
                    const match = toolExecs.find((t) => t.toolCallId === matchId);
                    if (match) {
                      const isErr = block.is_error as boolean || false;
                      match.output = formatToolOutput(block.content);
                      match.isError = isErr;
                      match.status = isErr ? "error" : "complete";
                    }
                  }
                }
              }
              msgs.push({
                id: i,
                role,
                content: extractTextContent(m),
                thinking: thinking || undefined,
                toolExecutions: toolExecs.length > 0 ? toolExecs : undefined,
                timestamp: Date.now(),
              });
            }
          }
          messages.value = msgs;
          msgId = msgs.length;
          currentAssistantContent.value = "";
          currentThinking.value = "";
          toolExecutions.value = [];
          isStreaming.value = false;
        }
        if (data.sessionFile)
          activeSessionPath.value = data.sessionFile as string;
        break;
      }
    }
  }

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
      addMessage(
        "system",
        `WebSocket disconnected after ${MAX_RECONNECT_ATTEMPTS} retries. Reload to reconnect.`,
      );
    }
  }

  function sendPrompt(text: string) {
    if (!text.trim()) return;
    addMessage("user", text);
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "prompt", message: text }));
    }
  }

  function sendCommand(cmd: Record<string, unknown>) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(cmd));
      return true;
    }
    return false;
  }

  function switchSession(sessionFile: string) {
    clearMessages();
    sendCommand({ type: "switch_session", sessionFile });
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
    activeSessionPath,
    wsSessions,
    sessionStatus,
    currentModel,
    connectWebSocket,
    sendPrompt,
    sendCommand,
    switchSession,
    setActiveSessionPath,
    restartPi,
    disconnect,
    loadHistory,
    clearMessages,
  };
}
