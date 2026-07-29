import { ref, reactive, computed, onUnmounted } from "vue";
import type { Message, ToolExecution, ProjectGroup } from "../types";
import {
  extractTextContent,
  extractThinkingContent,
  formatToolOutput,
} from "../utils/message";

// ─── Per-session state ─────────────────────────────────────────────

interface SessionState {
  sessionId: string;
  messages: Message[];
  msgId: number;
  isStreaming: boolean;
  currentAssistantContent: string;
  currentThinking: string;
  toolExecutions: ToolExecution[];
  currentModel: string;
}

function createSessionState(sessionId: string): SessionState {
  return reactive({
    sessionId,
    messages: [],
    msgId: 0,
    isStreaming: false,
    currentAssistantContent: "",
    currentThinking: "",
    toolExecutions: [],
    currentModel: "",
  });
}

export function usePiConnection() {
  const sessionStates = new Map<string, SessionState>();
  const activeInstanceId = ref<string | null>(null);

  // ── Global connection state ──
  const isRunning = ref(false);
  const statusText = ref("Connecting...");
  const wsSessions = ref<ProjectGroup[]>([]);
  const sessionStatus = ref<"running" | "idle" | null>(null);

  let ws: WebSocket | null = null;
  let reconnectAttempts = 0;
  const MAX_RECONNECT_ATTEMPTS = 3;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Active session helpers ──

  /** Get the active session state, creating it if needed. */
  function getOrCreateState(instanceId: string | null): SessionState {
    if (!instanceId) return createSessionState("__transient__");
    let s = sessionStates.get(instanceId);
    if (!s) {
      s = createSessionState(instanceId);
      sessionStates.set(instanceId, s);
    }
    return s;
  }

  /** Get the active session state only if it already exists (for events). */
  function getState(instanceId: string | null): SessionState | undefined {
    if (!instanceId) return undefined;
    return sessionStates.get(instanceId);
  }

  // ── Derived refs (computed from active session) ──

  const activeSessionState = computed(() => getOrCreateState(activeInstanceId.value));

  const messages = computed(() => activeSessionState.value.messages);
  const isStreaming = computed(() => activeSessionState.value.isStreaming);
  const currentAssistantContent = computed(() => activeSessionState.value.currentAssistantContent);
  const currentThinking = computed(() => activeSessionState.value.currentThinking);
  const toolExecutions = computed(() => activeSessionState.value.toolExecutions);
  const currentModel = computed(() => activeSessionState.value.currentModel);

  // ── Message helpers (write to a specific session's state) ──

  function addMessage(state: SessionState, role: Message["role"], content: string, extras?: Partial<Message>) {
    state.messages = [
      ...state.messages,
      { id: state.msgId++, role, content, timestamp: Date.now(), ...extras },
    ];
  }

  function getWsUrl(): string {
    const params = new URLSearchParams(window.location.search);
    const brokerWs = params.get("brokerWs");
    if (brokerWs) return brokerWs;
    const port = window.location.port;
    return `ws://${window.location.hostname}:${port}/ws`;
  }

  // ─── Event Handler ─────────────────────────────────────────────────

  function handlePiEvent(raw: Record<string, unknown>) {
    // ── Broker-level meta events ──
    if (raw.type === "capabilities") return;
    if (raw.type === "control_response") return;
    if (raw.type === "command_undeliverable") {
      const reason = raw.reason as string || "unknown";
      const command = raw.command as string || "unknown";
      const state = getOrCreateState(activeInstanceId.value);
      addMessage(state, "system", `[Delivery Error] Command "${command}" could not be delivered: ${reason}`);
      state.isStreaming = false;
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
        loadMessagesIntoSession(iid || activeInstanceId.value, msgs);
      }
      return;
    }

    // ── Unwrap the event envelope ──
    const eventInstanceId = raw.instanceId as string | undefined;
    let data: Record<string, unknown>;
    if (raw.type === "event" && raw.event) {
      data = raw.event as Record<string, unknown>;
    } else if (raw.payload && typeof raw.payload === "object") {
      data = raw.payload as Record<string, unknown>;
    } else {
      data = raw;
    }

    const instanceId = eventInstanceId || activeInstanceId.value;

    switch (data.type) {
      case "pi_started":
        isRunning.value = true;
        statusText.value = "Connected";
        break;

      case "pi_exited":
      case "disconnected":
        isRunning.value = false;
        statusText.value = "Disconnected";
        scheduleReconnect();
        break;

      case "error": {
        const s = getOrCreateState(instanceId);
        addMessage(s, "system", `[Error] ${data.error}`);
        break;
      }

      case "agent_start": {
        const s = getOrCreateState(instanceId);
        s.isStreaming = true;
        s.currentAssistantContent = "";
        s.currentThinking = "";
        s.toolExecutions = [];
        break;
      }

      case "agent_end": {
        const s = getOrCreateState(instanceId);
        const msgs = data.messages as Array<Record<string, unknown>> | undefined;
        if (Array.isArray(msgs)) {
          for (const m of msgs) {
            const modelId = m.model as string | undefined;
            if (modelId) {
              s.currentModel = modelId;
              break;
            }
          }
        }
        s.isStreaming = false;
        if (s.currentThinking || s.currentAssistantContent || s.toolExecutions.length > 0) {
          addMessage(s, "assistant", s.currentAssistantContent, {
            thinking: s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
          });
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
        }
        break;
      }

      case "message_update": {
        const s = getState(instanceId);
        if (!s) break;
        const evt = data.assistantMessageEvent as Record<string, unknown> | undefined;
        if (evt?.type === "text_delta") {
          s.currentAssistantContent += (evt.delta as string) || "";
        } else if (evt?.type === "thinking_delta") {
          s.currentThinking += (evt.delta as string) || "";
        }
        break;
      }

      case "message_end": {
        const s = getState(instanceId);
        if (!s) break;
        const msg = data.message as Record<string, unknown> | undefined;
        if (msg?.model) {
          s.currentModel = msg.model as string;
        }
        if (msg?.role === "assistant") {
          const content = extractTextContent(msg);
          const thinking = extractThinkingContent(msg);
          addMessage(s, "assistant", content || s.currentAssistantContent, {
            thinking: thinking || s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
          });
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
        }
        break;
      }

      case "turn_end": {
        const s = getState(instanceId);
        if (!s) break;
        if (s.currentThinking || s.currentAssistantContent || s.toolExecutions.length > 0) {
          addMessage(s, "assistant", s.currentAssistantContent, {
            thinking: s.currentThinking || undefined,
            toolExecutions: s.toolExecutions.length > 0 ? [...s.toolExecutions] : undefined,
          });
          s.currentAssistantContent = "";
          s.currentThinking = "";
          s.toolExecutions = [];
        }
        break;
      }

      case "tool_execution_start": {
        const s = getState(instanceId);
        if (!s) break;
        const toolCallId = data.toolCallId as string || `tool-${Date.now()}`;
        const toolName = data.toolName as string || "Tool";
        const args = (data.args as Record<string, unknown>) || {};
        s.toolExecutions = [...s.toolExecutions, { toolCallId, toolName, args, status: "pending" }];
        break;
      }
      case "tool_execution_update": {
        const s = getState(instanceId);
        if (!s) break;
        const toolCallId = data.toolCallId as string;
        const partialResult = data.partialResult;
        s.toolExecutions = s.toolExecutions.map((te) =>
          te.toolCallId === toolCallId
            ? { ...te, status: "streaming" as const, output: formatToolOutput(partialResult) }
            : te,
        );
        break;
      }
      case "tool_execution_end": {
        const s = getState(instanceId);
        if (!s) break;
        const toolCallId = data.toolCallId as string;
        const result = data.result;
        const isError = data.isError as boolean || false;
        s.toolExecutions = s.toolExecutions.map((te) =>
          te.toolCallId === toolCallId
            ? { ...te, status: isError ? "error" as const : "complete" as const, output: formatToolOutput(result), isError }
            : te,
        );
        break;
      }

      case "sessions_list": {
        const raw = data.projects as Array<Record<string, unknown>> || [];
        wsSessions.value = raw.map((p) => ({
          path: p.path as string || "",
          name: (p.name ?? p.dirName) as string || "",
          sessions: p.sessions as any[] || [],
        }));
        break;
      }

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
            const s = getState(instanceId);
            if (s) s.currentModel = model.id as string;
          }
        }
        if ((cmd === "set_model" || cmd === "cycle_model") && data.success) {
          const d = data.data as Record<string, unknown> | undefined;
          const model = (d?.model as Record<string, unknown>) || (d as Record<string, unknown> | undefined);
          if (model?.id) {
            const s = getState(instanceId);
            if (s) s.currentModel = model.id as string;
          }
        }
        break;
      }
    }
  }

  // ─── Snapshot Loading ───────────────────────────────────────────────

  function loadMessagesIntoSession(instanceId: string | null, msgs: Array<Record<string, unknown>>) {
    const s = getOrCreateState(instanceId);
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
    s.messages = parsed;
    s.msgId = parsed.length;
    s.isStreaming = false;
    s.currentAssistantContent = "";
    s.currentThinking = "";
    s.toolExecutions = [];
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
        // ignored
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
    }
  }

  // ─── Commands ───────────────────────────────────────────────────────

  function sendPrompt(text: string) {
    if (!text.trim()) return;
    const s = getOrCreateState(activeInstanceId.value);
    addMessage(s, "user", text);
    if (ws && ws.readyState === WebSocket.OPEN) {
      const iid = activeInstanceId.value;
      if (!iid) {
        addMessage(s, "system", "No active session yet — please wait for the session to be ready.");
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

  function switchSession(instanceId: string, initialMessages?: Message[]) {
    // Save current session state (already saved in the Map)
    // Restore (or init) target session state
    if (initialMessages) {
      const s = getOrCreateState(instanceId);
      s.messages = initialMessages;
      s.msgId = initialMessages.length;
    }
    activeInstanceId.value = instanceId;
    // Send directly — NOT via sendCommand, which wraps in broker_command
    // with the OLD activeInstanceId, overwriting the target
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "switch_session", instanceId }));
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

  function loadHistory(history: Message[]) {
    const s = getOrCreateState(activeInstanceId.value);
    s.messages = history;
    s.msgId = history.length;
  }

  function clearMessages() {
    const iid = activeInstanceId.value;
    if (iid) sessionStates.delete(iid);
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
    setActiveInstanceId: (id: string | null) => { activeInstanceId.value = id; },
    restartPi,
    disconnect,
    loadHistory,
    clearMessages,
  };
}
