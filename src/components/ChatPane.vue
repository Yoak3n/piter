<script setup lang="ts">
import { ref, computed, nextTick, watch } from "vue";
import { PanelRightClose, PanelRight, Terminal } from "lucide-vue-next";
import { marked } from "marked";

marked.setOptions({ breaks: true, gfm: true });

interface Message {
  id: number;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
  meta?: Record<string, unknown>;
  timestamp: number;
}

interface Turn {
  id: number;
  user: Message | null;
  assistant: Message | null;
  tools: Message[];
  system: Message | null;
}

const props = defineProps<{
  messages: Message[];
  isRunning: boolean;
  isStreaming: boolean;
  currentAssistantContent: string;
  statusText: string;
  sidebarCollapsed: boolean;
  drawerCollapsed: boolean;
  terminalOpen: boolean;
}>();

const emit = defineEmits<{
  (e: "send", text: string): void;
  (e: "restart-pi"): void;
  (e: "toggle-sidebar"): void;
  (e: "toggle-drawer", mode?: "files" | "sessions"): void;
  (e: "toggle-terminal"): void;
}>();

const inputText = ref("");
const timelineRef = ref<HTMLDivElement | null>(null);

// ─── Turn grouping ──────────────────────────────────────────────────

const turns = computed(() => {
  const result: Turn[] = [];
  let current: Turn | null = null;
  for (const msg of props.messages) {
    if (msg.role === "user") {
      if (current) result.push(current);
      current = { id: msg.id, user: msg, assistant: null, tools: [], system: null };
    } else if (msg.role === "assistant") {
      if (!current) current = { id: msg.id, user: null, assistant: null, tools: [], system: null };
      current.assistant = msg;
    } else if (msg.role === "tool") {
      if (!current) current = { id: msg.id, user: null, assistant: null, tools: [], system: null };
      current.tools.push(msg);
    } else if (msg.role === "system") {
      if (!current) current = { id: msg.id, user: null, assistant: null, tools: [], system: null };
      current.system = msg;
    }
  }
  if (current) result.push(current);
  return result;
});

// ─── Auto-scroll ────────────────────────────────────────────────────

watch(() => props.messages.length, () => {
  nextTick(() => { if (timelineRef.value) timelineRef.value.scrollTop = timelineRef.value.scrollHeight; });
});

// ─── Actions ────────────────────────────────────────────────────────

function send() {
  const text = inputText.value.trim();
  if (!text || !props.isRunning) return;
  emit("send", text);
  inputText.value = "";
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); }
}

function renderMarkdown(content: string): string {
  if (!content) return "";
  try { return marked.parse(content, { async: false }) as string; }
  catch { return `<pre>${escapeHtml(content)}</pre>`; }
}

function escapeHtml(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function formatTool(c: string): string {
  const t = c.trim();
  if (t.startsWith("{") || t.startsWith("[")) try { return JSON.stringify(JSON.parse(t), null, 2); } catch {}
  return c;
}
</script>

<template>
  <div class="chat">
    <!-- Header: minimal, just status + actions -->
    <header class="chat-header">
      <div class="flex-row gap-2">
        <button class="btn btn-ghost btn-icon btn-sm" @click="$emit('toggle-sidebar')" :title="sidebarCollapsed ? 'Show' : 'Hide'">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 18 9 12 15 6"/></svg>
        </button>
      </div>
      <div class="flex-row gap-2">
        <span v-if="!isRunning" class="status-label" style="color:var(--color-danger);">{{ statusText }}</span>
        <span class="status-dot" :class="{ connected: isRunning, disconnected: !isRunning }" :title="isRunning ? 'Connected' : 'Disconnected'"></span>
        <button class="btn btn-ghost btn-icon btn-sm" @click="$emit('toggle-terminal')" :title="terminalOpen ? 'Hide terminal' : 'Show terminal'">
          <Terminal :size="13" />
        </button>
        <button class="btn btn-ghost btn-icon btn-sm" @click="$emit('toggle-drawer')" :title="drawerCollapsed ? 'Show drawer' : 'Hide drawer'">
          <PanelRight v-if="drawerCollapsed" :size="13" />
          <PanelRightClose v-else :size="13" />
        </button>
      </div>
    </header>

    <!-- Timeline -->
    <div ref="timelineRef" class="timeline">
      <div v-if="turns.length === 0" class="empty-state">
        <div class="empty-icon">💬</div>
        <p>Chat with Pi, your coding agent.</p>
        <p class="empty-hint">Type a message below and press Enter.</p>
      </div>

      <div v-for="turn in turns" :key="turn.id" class="turn">
        <div v-if="turn.system" class="msg system-msg" v-html="escapeHtml(turn.system.content)"></div>

        <div v-if="turn.user" class="msg user-msg">
          <div class="msg-bubble user-bubble">
            <div class="markdown-body" v-html="renderMarkdown(turn.user.content)"></div>
          </div>
        </div>

        <div v-if="turn.assistant" class="msg assistant-msg">
          <div class="msg-bubble assistant-bubble">
            <div class="markdown-body" v-html="renderMarkdown(turn.assistant.content)"></div>
          </div>
        </div>

        <div v-if="turn.tools.length" class="tool-group">
          <details v-for="tool in turn.tools" :key="tool.id" class="tool-details">
            <summary class="tool-summary">
              {{ tool.content.split('\n')[0].substring(0, 80) }}
            </summary>
            <pre class="tool-body"><code>{{ formatTool(tool.content) }}</code></pre>
          </details>
        </div>
      </div>

      <!-- Streaming -->
      <div v-if="isStreaming" class="msg assistant-msg streaming">
        <div class="msg-bubble assistant-bubble" :class="{ 'thinking-bubble': !currentAssistantContent }">
          <template v-if="currentAssistantContent">
            <div class="markdown-body" v-html="renderMarkdown(currentAssistantContent)"></div>
            <span class="cursor-blink">▊</span>
          </template>
          <template v-else>
            <div class="thinking-dots">
              <span class="thinking-dot"></span>
              <span class="thinking-dot"></span>
              <span class="thinking-dot"></span>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- Composer -->
    <div class="composer">
      <div class="composer-box">
        <textarea
          v-model="inputText"
          class="composer-input"
          :placeholder="isRunning ? 'Type a message... (Enter to send)' : 'Pi is disconnected'"
          :disabled="!isRunning"
          @keydown="handleKeydown"
          rows="2"
        />
        <div class="composer-actions">
          <span class="composer-hint">
            <template v-if="isRunning">Enter to send · Shift+Enter newline</template>
            <template v-else><a href="#" @click.prevent="$emit('restart-pi')" style="color:var(--color-accent);">Reconnect</a> to pi agent</template>
          </span>
          <button class="btn btn-primary btn-sm" :disabled="!isRunning || !inputText.trim() || isStreaming" @click="send">
            <span v-if="isStreaming" class="spinner"></span>
            <span v-else>Send</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

/* ─── Header ─── */
.chat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--color-bg-panel);
  flex-shrink: 0;
  height: 40px;
}
.status-dot {
  display: inline-block;
  width: 8px; height: 8px;
  border-radius: 50%;
  background: #999;
  transition: background 0.3s;
  flex-shrink: 0;
}
.status-dot.connected { background: var(--color-accent); box-shadow: 0 0 4px var(--color-accent); }
.status-dot.disconnected { background: var(--color-danger); }
.status-label {
  font-size: var(--font-size-micro);
  color: var(--color-text-tertiary);
  min-width: 0;
}

/* ─── Timeline ─── */
.timeline {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--color-text-tertiary);
  text-align: center;
  gap: var(--space-2);
}
.empty-icon { font-size: 2.5rem; }
.empty-hint { font-size: var(--font-size-caption); }

.turn { display: flex; flex-direction: column; gap: var(--space-2); }

.msg {
  display: flex;
  max-width: 88%;
}
.user-msg { align-self: flex-end; }
.assistant-msg { align-self: flex-start; }
.system-msg {
  align-self: center;
  font-size: var(--font-size-caption);
  color: var(--color-text-tertiary);
  background: var(--color-bg-muted);
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-pill);
}

.msg-bubble {
  border-radius: var(--radius-lg);
  padding: var(--space-2) var(--space-3);
  line-height: var(--line-height-body);
  font-size: var(--font-size-body);
}
.user-bubble {
  background: var(--color-accent-soft);
  border: 1px solid color-mix(in srgb, var(--color-accent) 15%, transparent);
}
.assistant-bubble {
  background: var(--color-bg-panel);
  border: 1px solid var(--color-border-subtle);
}

/* ─── Tools ─── */
.tool-group {
  align-self: flex-start;
  max-width: 92%;
  margin-left: 8px;
}
.tool-details { margin-top: 2px; }
.tool-summary {
  font-size: var(--font-size-caption);
  color: var(--color-text-secondary);
  cursor: pointer;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--color-bg-muted);
}
.tool-summary:hover { background: var(--color-bg-hover); }
.tool-body {
  margin: 0;
  padding: var(--space-2);
  font-size: var(--font-size-caption);
  font-family: var(--font-family-mono);
  background: var(--color-bg-muted);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-sm);
  overflow-x: auto;
  max-height: 300px;
  white-space: pre-wrap;
  word-break: break-word;
}

/* ─── Streaming ─── */
.cursor-blink { animation: blink 1s step-end infinite; color: var(--color-accent); }
@keyframes blink { 50% { opacity: 0; } }

.thinking-bubble {
  min-height: 36px;
  display: flex;
  align-items: center;
  border-color: var(--color-border-subtle);
}
.thinking-dots {
  display: flex;
  gap: 5px;
  padding: 2px 0;
}
.thinking-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-text-tertiary);
  animation: thinkBounce 1.4s ease-in-out infinite;
}
.thinking-dot:nth-child(1) { animation-delay: 0s; }
.thinking-dot:nth-child(2) { animation-delay: 0.2s; }
.thinking-dot:nth-child(3) { animation-delay: 0.4s; }
@keyframes thinkBounce {
  0%, 80%, 100% { transform: scale(0.6); opacity: 0.4; }
  40% { transform: scale(1); opacity: 1; }
}

/* ─── Markdown ─── */
.markdown-body :deep(h1), .markdown-body :deep(h2), .markdown-body :deep(h3), .markdown-body :deep(h4) { margin: 0.6em 0 0.3em; line-height: 1.3; }
.markdown-body :deep(h1) { font-size: 1.2em; }
.markdown-body :deep(h2) { font-size: 1.1em; }
.markdown-body :deep(h3) { font-size: 1.05em; }
.markdown-body :deep(p) { margin: 0.3em 0; }
.markdown-body :deep(ul), .markdown-body :deep(ol) { margin: 0.3em 0; padding-left: 1.5em; }
.markdown-body :deep(code) { font-family: var(--font-family-mono); font-size: 0.9em; background: var(--color-bg-muted); padding: 1px 4px; border-radius: var(--radius-xs); }
.markdown-body :deep(pre) { margin: 0.5em 0; padding: var(--space-3); background: var(--color-code-bg); color: var(--color-code-text); border-radius: var(--radius-md); overflow-x: auto; font-family: var(--font-family-mono); font-size: var(--font-size-caption); }
.markdown-body :deep(pre code) { background: none; padding: 0; }
.markdown-body :deep(blockquote) { margin: 0.4em 0; padding-left: var(--space-3); border-left: 3px solid var(--color-border-strong); color: var(--color-text-secondary); }
.markdown-body :deep(table) { border-collapse: collapse; margin: 0.4em 0; font-size: var(--font-size-control); }
.markdown-body :deep(th), .markdown-body :deep(td) { border: 1px solid var(--color-border-subtle); padding: var(--space-1) var(--space-2); text-align: left; }
.markdown-body :deep(th) { background: var(--color-bg-muted); font-weight: 600; }

/* ─── Composer ─── */
.composer {
  flex-shrink: 0;
  border-top: 1px solid var(--color-border-subtle);
  background: var(--color-bg-panel);
}
.composer-box {
  display: flex;
  flex-direction: column;
  padding: var(--space-3);
  gap: var(--space-2);
}
.composer-input {
  width: 100%;
  min-height: 56px;
  max-height: 160px;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-lg);
  background: var(--color-bg-app);
  color: var(--color-text-primary);
  font-size: var(--font-size-body);
  line-height: var(--line-height-body);
  resize: vertical;
  outline: none;
  font-family: var(--font-family-base);
  transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
}
.composer-input:focus { border-color: var(--color-accent); box-shadow: var(--focus-ring); }
.composer-input:hover { border-color: var(--color-border-strong); }
.composer-input:disabled { opacity: 0.5; }
.composer-input::placeholder { color: var(--color-text-tertiary); }
.composer-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.composer-hint { font-size: var(--font-size-caption); color: var(--color-text-tertiary); }
</style>
