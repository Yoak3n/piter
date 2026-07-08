<script setup lang="ts">
import { ref, computed, nextTick, watch } from "vue";
import { Menu, ChevronRight, Brain, Copy, Check } from "lucide-vue-next";
import { marked } from "marked";

marked.setOptions({ breaks: true, gfm: true });

interface ToolExecution {
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  status: "pending" | "streaming" | "complete" | "error";
  output?: string;
  isError?: boolean;
}

interface Message {
  id: number;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
  thinking?: string;
  toolExecutions?: ToolExecution[];
  meta?: Record<string, unknown>;
  timestamp: number;
}

interface Turn {
  id: number;
  user: Message | null;
  assistants: Message[];
  tools: Message[];
  system: Message | null;
}

const props = defineProps<{
  messages: Message[];
  isRunning: boolean;
  isStreaming: boolean;
  currentAssistantContent: string;
  currentThinking?: string;
  toolExecutions?: ToolExecution[];
  statusText: string;
  sessionName?: string;
  modelName?: string;
  sidebarCollapsed: boolean;
}>();

const emit = defineEmits<{
  (e: "send", text: string): void;
  (e: "restart-pi"): void;
  (e: "toggle-sidebar"): void;
}>();

const inputText = ref("");
const timelineRef = ref<HTMLDivElement | null>(null);

// Track expanded/collapsed state for thinking blocks and tool cards
const expandedThinking = ref<Set<number>>(new Set());
const expandedTools = ref<Set<string>>(new Set());

function toggleThinking(id: number) {
  if (expandedThinking.value.has(id)) {
    expandedThinking.value.delete(id);
  } else {
    expandedThinking.value.add(id);
  }
}

function toggleTool(toolCallId: string) {
  if (expandedTools.value.has(toolCallId)) {
    expandedTools.value.delete(toolCallId);
  } else {
    expandedTools.value.add(toolCallId);
  }
}

const turns = computed(() => {
  const result: Turn[] = [];
  let current: Turn | null = null;
  for (const msg of props.messages) {
    if (msg.role === "user") {
      if (current) result.push(current);
      current = { id: msg.id, user: msg, assistants: [], tools: [], system: null };
    } else if (msg.role === "assistant") {
      if (!current) current = { id: msg.id, user: null, assistants: [], tools: [], system: null };
      current.assistants.push(msg);
    } else if (msg.role === "tool") {
      if (!current) current = { id: msg.id, user: null, assistants: [], tools: [], system: null };
      current.tools.push(msg);
    } else if (msg.role === "system") {
      if (!current) current = { id: msg.id, user: null, assistants: [], tools: [], system: null };
      current.system = msg;
    }
  }
  if (current) result.push(current);
  return result;
});

watch(() => props.messages.length, () => {
  nextTick(() => { if (timelineRef.value) timelineRef.value.scrollTop = timelineRef.value.scrollHeight; });
});

// Auto-scroll during streaming
watch(() => props.currentAssistantContent, () => {
  nextTick(() => { if (timelineRef.value) timelineRef.value.scrollTop = timelineRef.value.scrollHeight; });
});
watch(() => props.currentThinking, () => {
  nextTick(() => { if (timelineRef.value) timelineRef.value.scrollTop = timelineRef.value.scrollHeight; });
});

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

function getArgsPreview(toolName: string, args: Record<string, unknown>): string {
  if (!args || Object.keys(args).length === 0) return "";
  if (args.path) return String(args.path).substring(0, 80);
  if (args.command) return String(args.command).substring(0, 80);
  if (args.query) return String(args.query).substring(0, 60);
  if (args.url) return String(args.url);
  for (const val of Object.values(args)) {
    if (typeof val === "string" && val.length > 0) return val.substring(0, 60);
  }
  return "";
}

function formatArgs(args: Record<string, unknown>): string {
  try {
    if (Object.keys(args).length === 0) return "";
    return JSON.stringify(args, null, 2);
  } catch { return String(args); }
}

// Copy to clipboard
const copiedId = ref<string | null>(null);
function copyToClipboard(text: string, id: string) {
  const doCopy = navigator.clipboard
    ? navigator.clipboard.writeText(text)
    : new Promise<void>((resolve) => {
        const ta = document.createElement("textarea");
        ta.value = text;
        ta.style.cssText = "position:fixed;left:-9999px";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
        resolve();
      });
  doCopy.then(() => {
    copiedId.value = id;
    setTimeout(() => { if (copiedId.value === id) copiedId.value = null; }, 1500);
  });
}
</script>

<template>
  <div class="chat">
    <!-- Header -->
    <header class="chat-header">
      <button class="btn btn-ghost btn-icon btn-sm hamburger" @click="$emit('toggle-sidebar')" :title="sidebarCollapsed ? 'Show sessions' : 'Hide sessions'">
        <Menu :size="16" />
      </button>
      <div class="header-info">
        <span v-if="sessionName" class="session-label">{{ sessionName }}</span>
      </div>
      <div class="header-right">
        <slot name="header-extra" />
        <span class="status-dot" :class="{ connected: isRunning, disconnected: !isRunning }" :title="isRunning ? 'Connected' : 'Disconnected'" />
        <span v-if="!isRunning" class="status-label disconnected-label">{{ statusText }}</span>
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
        <div v-if="turn.system" class="msg system-msg" v-html="escapeHtml(turn.system.content)" />
        <div v-if="turn.user" class="msg user-msg">
          <div class="msg-bubble user-bubble">
            <div class="markdown-body" v-html="renderMarkdown(turn.user.content)" />
          </div>
        </div>
        <template v-if="turn.assistants.length">
          <template v-for="assistant in turn.assistants" :key="assistant.id">
            <!-- Thinking block (collapsible) -->
            <div v-if="assistant.thinking" class="thinking-block">
              <div class="thinking-header" @click="toggleThinking(assistant.id)">
                <ChevronRight :size="12" class="thinking-chevron" :class="{ expanded: expandedThinking.has(assistant.id) }" />
                <Brain :size="12" />
                <span class="thinking-label">Thinking</span>
              </div>
              <div v-if="expandedThinking.has(assistant.id)" class="thinking-content">
                {{ assistant.thinking }}
              </div>
            </div>
            <!-- Tool execution cards -->
            <div v-if="assistant.toolExecutions?.length" class="tool-executions">
              <div v-for="te in assistant.toolExecutions" :key="te.toolCallId" class="tool-card">
                <div class="tool-card-header" @click="toggleTool(te.toolCallId)">
                  <div class="tool-header-left">
                    <ChevronRight :size="12" class="tool-chevron" :class="{ expanded: expandedTools.has(te.toolCallId) }" />
                    <span class="tool-name">{{ te.toolName }}</span>
                    <span v-if="getArgsPreview(te.toolName, te.args)" class="tool-args-preview">{{ getArgsPreview(te.toolName, te.args) }}</span>
                  </div>
                  <div class="tool-header-right">
                    <span class="tool-status" :class="te.status">{{ te.status }}</span>
                  </div>
                </div>
                <div v-if="expandedTools.has(te.toolCallId)" class="tool-card-body">
                  <div v-if="formatArgs(te.args)" class="tool-args">{{ formatArgs(te.args) }}</div>
                  <div v-if="te.output" class="tool-output">{{ te.output }}</div>
                </div>
              </div>
            </div>
            <!-- Assistant text bubble -->
            <div v-if="assistant.content" class="msg assistant-msg">
              <div class="msg-bubble assistant-bubble">
                <div class="markdown-body" v-html="renderMarkdown(assistant.content)" />
                <button class="copy-btn" :class="{ copied: copiedId === `msg-${assistant.id}` }" @click="copyToClipboard(assistant.content, `msg-${assistant.id}`)">
                  <Check v-if="copiedId === `msg-${assistant.id}`" :size="12" />
                  <Copy v-else :size="12" />
                </button>
              </div>
            </div>
          </template>
        </template>
      </div>

      <!-- Streaming state -->
      <div v-if="isStreaming" class="turn streaming-turn">
        <!-- Streaming thinking -->
        <div v-if="currentThinking" class="thinking-block streaming">
          <div class="thinking-header expanded">
            <ChevronRight :size="12" class="thinking-chevron expanded" />
            <Brain :size="12" />
            <span class="thinking-label">Thinking</span>
            <span class="thinking-dots-inline">
              <span class="thinking-dot" />
              <span class="thinking-dot" />
              <span class="thinking-dot" />
            </span>
          </div>
          <div class="thinking-content expanded">
            {{ currentThinking }}
          </div>
        </div>
        <!-- Streaming tool executions -->
        <div v-if="toolExecutions?.length" class="tool-executions">
          <div v-for="te in toolExecutions" :key="te.toolCallId" class="tool-card" :class="te.status">
            <div class="tool-card-header" @click="toggleTool(`stream-${te.toolCallId}`)">
              <div class="tool-header-left">
                <ChevronRight :size="12" class="tool-chevron" :class="{ expanded: expandedTools.has(`stream-${te.toolCallId}`) }" />
                <span class="tool-name">{{ te.toolName }}</span>
                <span v-if="getArgsPreview(te.toolName, te.args)" class="tool-args-preview">{{ getArgsPreview(te.toolName, te.args) }}</span>
              </div>
              <div class="tool-header-right">
                <span class="tool-status" :class="te.status">{{ te.status }}</span>
              </div>
            </div>
            <div v-if="expandedTools.has(`stream-${te.toolCallId}`)" class="tool-card-body">
              <div v-if="formatArgs(te.args)" class="tool-args">{{ formatArgs(te.args) }}</div>
              <div v-if="te.output" class="tool-output">{{ te.output }}</div>
            </div>
          </div>
        </div>
        <!-- Streaming assistant text -->
        <div v-if="currentAssistantContent" class="msg assistant-msg">
          <div class="msg-bubble assistant-bubble">
            <div class="markdown-body" v-html="renderMarkdown(currentAssistantContent)" />
            <span class="cursor-blink" />
          </div>
        </div>
        <!-- Thinking dots when nothing else to show -->
        <div v-if="!currentThinking && !currentAssistantContent && (!toolExecutions?.length)" class="msg assistant-msg">
          <div class="msg-bubble assistant-bubble thinking-bubble">
            <div class="thinking-dots">
              <span class="thinking-dot" />
              <span class="thinking-dot" />
              <span class="thinking-dot" />
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Composer -->
    <div class="composer">
      <div class="composer-box">
        <textarea
          v-model="inputText"
          class="composer-input"
          :placeholder="isRunning ? 'Message Pi...' : 'Disconnected'"
          :disabled="!isRunning"
          @keydown="handleKeydown"
          rows="1"
        />
        <div class="composer-actions">
          <span class="composer-hint">
            <template v-if="isRunning">Enter to send, Shift+Enter for newline</template>
            <template v-else>
              <button class="btn-ghost-sm" @click="$emit('restart-pi')">Reconnect</button>
            </template>
          </span>
          <button class="btn btn-primary btn-sm" :disabled="!isRunning || !inputText.trim() || isStreaming" @click="send">
            <span v-if="isStreaming" class="spinner" />
            <span v-else>Send</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat { display:flex; flex-direction:column; height:100%; overflow:hidden; }

.chat-header {
  display:flex; align-items:center; justify-content:space-between;
  padding:0 12px; height:44px; flex-shrink:0;
  border-bottom:1px solid var(--color-border-subtle);
  background:var(--color-bg-panel);
}
.hamburger { display:none; }
.header-info { display:flex; align-items:center; gap:8px; min-width:0; flex:1; }
.session-label { font-size:12px; font-weight:500; color:var(--color-text-primary); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:200px; }
.header-right { display:flex; align-items:center; gap:6px; }
.status-dot { width:7px; height:7px; border-radius:50%; background:#999; flex-shrink:0; }
.status-dot.connected { background:var(--color-accent); box-shadow:0 0 4px var(--color-accent); }
.status-dot.disconnected { background:var(--color-danger); }
.status-label { font-size:10px; color:var(--color-text-tertiary); }
.disconnected-label { color:var(--color-danger); }
.btn-ghost-sm { font-size:11px; color:var(--color-accent); cursor:pointer; background:none; border:none; padding:2px 6px; }

.timeline { flex:1; overflow-y:auto; padding:16px 12px; display:flex; flex-direction:column; gap:12px; }
.empty-state { display:flex; flex-direction:column; align-items:center; justify-content:center; height:100%; color:var(--color-text-tertiary); text-align:center; gap:4px; }
.empty-icon { font-size:2.5rem; }
.empty-hint { font-size:11px; }

.turn { display:flex; flex-direction:column; gap:6px; }
.msg { display:flex; max-width:90%; }
.user-msg { align-self:flex-end; }
.assistant-msg { align-self:flex-start; }
.system-msg { align-self:center; font-size:10px; color:var(--color-text-tertiary); background:var(--color-bg-muted); padding:2px 10px; border-radius:10px; }

.msg-bubble { border-radius:12px; padding:8px 12px; line-height:1.5; font-size:13px; position:relative; }
.user-bubble { background:var(--color-accent-soft); border:1px solid color-mix(in srgb, var(--color-accent) 15%, transparent); }
.assistant-bubble { background:var(--color-bg-panel); border:1px solid var(--color-border-subtle); }

.cursor-blink { display:inline-block; width:6px; height:14px; background:var(--color-accent); animation:blink 1s step-end infinite; vertical-align:text-bottom; }
@keyframes blink { 50% { opacity:0; } }
.thinking-bubble { min-height:32px; display:flex; align-items:center; }
.thinking-dots { display:flex; gap:4px; padding:4px 0; }
.thinking-dot { width:6px; height:6px; border-radius:50%; background:var(--color-text-tertiary); animation:thinkBounce 1.4s ease-in-out infinite; }
.thinking-dot:nth-child(1) { animation-delay:0s; }
.thinking-dot:nth-child(2) { animation-delay:0.2s; }
.thinking-dot:nth-child(3) { animation-delay:0.4s; }
@keyframes thinkBounce { 0%,80%,100%{ transform:scale(0.6); opacity:0.4; } 40%{ transform:scale(1); opacity:1; } }

/* Copy button */
.copy-btn { position:absolute; top:6px; right:6px; opacity:0; display:flex; align-items:center; justify-content:center; width:24px; height:24px; border:none; background:var(--color-bg-muted); border-radius:var(--radius-sm); color:var(--color-text-tertiary); cursor:pointer; transition:opacity 0.15s, color 0.15s; }
.msg-bubble:hover .copy-btn { opacity:0.6; }
.copy-btn:hover { opacity:1 !important; }
.copy-btn.copied { opacity:1 !important; color:var(--success); }

/* Thinking block */
.thinking-block {
  background:var(--color-bg-muted);
  border:1px solid var(--color-border-subtle);
  border-radius:10px;
  overflow:hidden;
  align-self:flex-start;
  max-width:90%;
  font-size:13px;
  transition:border-color 0.2s var(--ease);
}
.thinking-block:hover { border-color:var(--color-border-strong); }
.thinking-header {
  display:flex; align-items:center; gap:8px;
  padding:8px 12px; cursor:pointer; user-select:none;
  font-size:12px; color:var(--color-text-tertiary);
  transition:background 0.15s var(--ease);
}
.thinking-header:hover { background:var(--color-bg-hover); }
.thinking-label { font-family:var(--font-family-mono); font-size:11px; }
.thinking-chevron { transition:transform 0.2s var(--ease); opacity:0.4; flex-shrink:0; }
.thinking-chevron.expanded { transform:rotate(90deg); }
.thinking-content {
  padding:0 12px 12px; white-space:pre-wrap;
  font-style:italic; border-top:1px solid var(--color-border-subtle);
  max-height:260px; overflow-y:auto; overscroll-behavior:contain;
  font-size:12px; line-height:1.5; color:var(--color-text-secondary);
}
.thinking-content.expanded { display:block; }
.thinking-dots-inline { display:flex; gap:3px; margin-left:4px; }
.thinking-dots-inline .thinking-dot { width:4px; height:4px; }

/* Tool execution cards */
.tool-executions { display:flex; flex-direction:column; gap:4px; align-self:flex-start; max-width:90%; }
.tool-card {
  background:var(--color-bg-muted);
  border:1px solid var(--color-border-subtle);
  border-radius:10px;
  overflow:hidden;
  font-size:13px;
  transition:border-color 0.2s var(--ease);
}
.tool-card:hover { border-color:var(--color-border-strong); }
.tool-card-header {
  display:flex; justify-content:space-between; align-items:center;
  padding:8px 12px; cursor:pointer; user-select:none;
  transition:background 0.15s var(--ease);
}
.tool-card-header:hover { background:var(--color-bg-hover); }
.tool-header-left { display:flex; align-items:center; gap:8px; min-width:0; }
.tool-header-right { display:flex; align-items:center; gap:6px; flex-shrink:0; }
.tool-chevron { transition:transform 0.2s var(--ease); opacity:0.4; flex-shrink:0; }
.tool-chevron.expanded { transform:rotate(90deg); }
.tool-name { color:var(--color-accent); font-family:var(--font-family-mono); font-size:11px; }
.tool-args-preview { color:var(--color-text-tertiary); font-family:var(--font-family-mono); font-size:11px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:300px; }
.tool-status {
  font-size:10px; padding:2px 7px; border-radius:var(--radius-pill);
  text-transform:uppercase; letter-spacing:0.04em; flex-shrink:0;
  display:flex; align-items:center; gap:4px;
}
.tool-status.pending { background:var(--color-bg-panel); color:var(--color-text-tertiary); border:1px solid var(--color-border-subtle); }
.tool-status.pending::before { content:"○"; font-size:8px; }
.tool-status.streaming { background:var(--color-accent); color:#fff; animation:pulse 1.5s infinite; }
.tool-status.streaming::before { content:"●"; font-size:7px; }
.tool-status.complete { background:var(--success-soft, rgba(74,154,106,0.1)); color:var(--success); border:1px solid rgba(74,154,106,0.2); }
.tool-status.complete::before { content:"✓"; font-size:9px; }
.tool-status.error { background:var(--danger-soft, rgba(217,92,92,0.1)); color:var(--danger); border:1px solid rgba(217,92,92,0.2); }
.tool-status.error::before { content:"!"; font-size:9px; }
@keyframes pulse { 0%,100%{ opacity:1; } 50%{ opacity:0.7; } }
.tool-card-body { border-top:1px solid var(--color-border-subtle); }
.tool-args {
  background:rgba(0,0,0,0.06); padding:10px 12px;
  font-family:var(--font-family-mono); font-size:11px;
  overflow-x:auto; white-space:pre-wrap;
  border-bottom:1px solid var(--color-border-subtle);
}
[data-theme="dark"] .tool-args { background:rgba(0,0,0,0.2); }
.tool-output {
  padding:10px 12px; font-family:var(--font-family-mono); font-size:11px;
  white-space:pre-wrap; overflow-x:auto; max-height:300px; overflow-y:auto;
}

.composer { flex-shrink:0; border-top:1px solid var(--color-border-subtle); background:var(--color-bg-panel); }
.composer-box { display:flex; flex-direction:column; padding:10px 12px; gap:6px; }
.composer-input { width:100%; min-height:40px; max-height:120px; padding:8px 10px; border:1px solid var(--color-border-subtle); border-radius:10px; background:var(--color-bg-app); color:var(--color-text-primary); font-size:13px; line-height:1.4; resize:none; outline:none; font-family:var(--font-family-base); }
.composer-input:focus { border-color:var(--color-accent); }
.composer-input:disabled { opacity:0.4; }
.composer-actions { display:flex; align-items:center; justify-content:space-between; }
.composer-hint { font-size:10px; color:var(--color-text-tertiary); }

.markdown-body :deep(h1),.markdown-body :deep(h2),.markdown-body :deep(h3){ margin:0.4em 0 0.2em; line-height:1.3; }
.markdown-body :deep(h1){ font-size:1.15em; }
.markdown-body :deep(h2){ font-size:1.05em; }
.markdown-body :deep(h3){ font-size:1em; }
.markdown-body :deep(p){ margin:0.2em 0; }
.markdown-body :deep(ul),.markdown-body :deep(ol){ margin:0.2em 0; padding-left:1.4em; }
.markdown-body :deep(code){ font-family:var(--font-family-mono); font-size:0.85em; background:var(--color-bg-muted); padding:1px 4px; border-radius:3px; }
.markdown-body :deep(pre){ margin:0.4em 0; padding:10px; background:var(--color-code-bg); color:var(--color-code-text); border-radius:8px; overflow-x:auto; font-family:var(--font-family-mono); font-size:11px; }
.markdown-body :deep(pre code){ background:none; padding:0; }
.markdown-body :deep(blockquote){ margin:0.3em 0; padding-left:10px; border-left:2px solid var(--color-border-strong); color:var(--color-text-secondary); }

@media (max-width: 640px) {
  .hamburger { display:flex; }
  .msg { max-width:95%; }
  .session-label { max-width:120px; }
  .composer-input { font-size:16px; }  /* prevent iOS zoom */
  .tool-args-preview { max-width:140px; }
}
</style>
