import { ref, onUnmounted } from "vue";

export interface Message {
  id: number;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
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
  const activeSessionPath = ref<string | null>(null);
  const wsSessions = ref<ProjectGroup[]>([]);
  const sessionStatus = ref<"running" | "idle" | null>(null);

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

  function addMessage(role: Message["role"], content: string) {
    messages.value = [
      ...messages.value,
      { id: msgId++, role, content, timestamp: Date.now() },
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

  function setActiveSessionPath(path: string | null) {
    activeSessionPath.value = path;
  }

  function handlePiEvent(data: Record<string, unknown>) {
    // Route events to the correct session when sessionPath metadata is present
    if (data.sessionPath && activeSessionPath.value) {
      if (data.sessionPath !== activeSessionPath.value) return;
      data = data.payload as Record<string, unknown>;
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
        break;
      case "agent_end":
        isStreaming.value = false;
        if (currentAssistantContent.value) {
          addMessage("assistant", currentAssistantContent.value);
          currentAssistantContent.value = "";
        }
        break;
      case "message_update": {
        const evt = data.assistantMessageEvent as
          | Record<string, unknown>
          | undefined;
        if (evt?.type === "text_delta")
          currentAssistantContent.value += (evt.delta as string) || "";
        break;
      }
      case "message_end": {
        const msg = data.message as Record<string, unknown> | undefined;
        if (msg?.role === "assistant") {
          const content = extractTextContent(msg);
          if (content) addMessage("assistant", content);
          currentAssistantContent.value = "";
        }
        break;
      }
      case "turn_end": {
        if (currentAssistantContent.value) {
          addMessage("assistant", currentAssistantContent.value);
          currentAssistantContent.value = "";
        }
        break;
      }
      case "tool_execution_start": {
        const tc = data as {
          toolName?: string;
          args?: Record<string, unknown>;
        };
        addMessage(
          "tool",
          `🔧 ${tc.toolName || "Tool"}(${JSON.stringify(tc.args || {})})`,
        );
        break;
      }
      case "tool_execution_end": {
        const tc = data as { toolName?: string; isError?: boolean };
        if (tc.isError) addMessage("tool", `❌ ${tc.toolName || "Tool"} failed`);
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
          if (d?.sessionFile)
            activeSessionPath.value = d.sessionFile as string;
        }
        if (cmd === "switch_session" && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          if (d?.sessionFile)
            activeSessionPath.value = d.sessionFile as string;
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
              msgs.push({
                id: i,
                role,
                content: extractTextContent(m),
                timestamp: Date.now(),
              });
            }
          }
          messages.value = msgs;
          msgId = msgs.length;
          currentAssistantContent.value = "";
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
    activeSessionPath,
    wsSessions,
    sessionStatus,
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
