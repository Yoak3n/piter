<script setup lang="ts">
import { ref, computed, nextTick, watch } from "vue";
import { Menu } from "lucide-vue-next";
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

watch(() => props.messages.length, () => {
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
        <div v-if="turn.assistant" class="msg assistant-msg">
          <div class="msg-bubble assistant-bubble">
            <div class="markdown-body" v-html="renderMarkdown(turn.assistant.content)" />
          </div>
        </div>
      </div>

      <div v-if="isStreaming" class="msg assistant-msg streaming">
        <div class="msg-bubble assistant-bubble" :class="{ 'thinking-bubble': !currentAssistantContent }">
          <template v-if="currentAssistantContent">
            <div class="markdown-body" v-html="renderMarkdown(currentAssistantContent)" />
            <span class="cursor-blink" />
          </template>
          <template v-else>
            <div class="thinking-dots">
              <span class="thinking-dot" />
              <span class="thinking-dot" />
              <span class="thinking-dot" />
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

.msg-bubble { border-radius:12px; padding:8px 12px; line-height:1.5; font-size:13px; }
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
}
</style>
