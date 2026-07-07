import { ref, onUnmounted } from "vue";

export interface Message {
  id: number;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
  meta?: Record<string, unknown>;
  timestamp: number;
}

export interface SessionSummary {
  id: string;
  name?: string;
  preview?: string;
  filePath: string;
  updatedAt: number;
  createdAt: number;
}

export function usePiConnection() {
  const messages = ref<Message[]>([]);
  const isRunning = ref(false);
  const isStreaming = ref(false);
  const statusText = ref("Connecting...");
  const currentAssistantContent = ref("");
  const activeSessionPath = ref<string | null>(null);

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

  function handlePiEvent(data: Record<string, unknown>) {
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
    connectWebSocket,
    sendPrompt,
    restartPi,
    disconnect,
  };
}
