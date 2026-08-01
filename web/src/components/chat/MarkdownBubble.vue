<script setup lang="ts">
import { ref, computed } from "vue";
import { Copy, Check } from "lucide-vue-next";
import { marked } from "marked";
import { formatMessageTime } from "../../utils/message";

marked.setOptions({ breaks: true, gfm: true });

const props = defineProps<{
  content: string;
  mode: "user" | "assistant" | "streaming";
  timestamp?: number;
}>();

function renderMarkdown(content: string): string {
  if (!content) return "";
  try { return marked.parse(content, { async: false }) as string; }
  catch { return `<pre>${escapeHtml(content)}</pre>`; }
}

function escapeHtml(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// Time label: only for finalized user/assistant text messages (streaming
// bubbles and thinking/tool-only messages have no timestamp).
const showTime = computed(() => props.mode !== "streaming" && !!props.timestamp);
const timeLabel = computed(() => (props.timestamp ? formatMessageTime(props.timestamp) : ""));

// Copy to clipboard
const copied = ref(false);
function copyToClipboard() {
  const doCopy = navigator.clipboard
    ? navigator.clipboard.writeText(props.content)
    : new Promise<void>((resolve) => {
        const ta = document.createElement("textarea");
        ta.value = props.content;
        ta.style.cssText = "position:fixed;left:-9999px";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
        resolve();
      });
  doCopy.then(() => {
    copied.value = true;
    setTimeout(() => { copied.value = false; }, 1500);
  });
}
</script>

<template>
  <div class="msg" :class="mode === 'user' ? 'user-msg' : 'assistant-msg'">
    <div class="msg-bubble" :class="mode === 'user' ? 'user-bubble' : 'assistant-bubble'">
      <div class="markdown-body" v-html="renderMarkdown(content)" />
      <button
        v-if="mode === 'assistant'"
        class="copy-btn"
        :class="{ copied }"
        aria-label="Copy message"
        @click="copyToClipboard"
      >
        <Check v-if="copied" :size="12" />
        <Copy v-else :size="12" />
      </button>
      <span v-if="mode === 'streaming'" class="cursor-blink" />
    </div>
    <span v-if="showTime" class="msg-time">{{ timeLabel }}</span>
  </div>
</template>

<style scoped>
.msg { display:flex; flex-direction:column; max-width:90%; min-width:0; }
.user-msg { align-self:flex-end; align-items:flex-end; }
.assistant-msg { align-self:flex-start; align-items:flex-start; }

.msg-bubble { border-radius:12px; padding:8px 12px; line-height:1.5; font-size:13px; position:relative; min-width:0; }
.user-bubble { background:var(--color-accent-soft); border:1px solid color-mix(in srgb, var(--color-accent) 15%, transparent); }
.assistant-bubble { background:var(--color-bg-panel); border:1px solid var(--color-border-subtle); }

.msg-time { font-size:10px; color:var(--color-text-tertiary); margin-top:2px; padding:0 4px; user-select:none; }

.cursor-blink { display:inline-block; width:6px; height:14px; background:var(--color-accent); animation:blink 1s step-end infinite; vertical-align:text-bottom; }
@keyframes blink { 50% { opacity:0; } }

/* Copy button */
.copy-btn { position:absolute; top:6px; right:6px; opacity:0; display:flex; align-items:center; justify-content:center; width:24px; height:24px; border:none; background:var(--color-bg-muted); border-radius:var(--radius-sm); color:var(--color-text-tertiary); cursor:pointer; transition:opacity 0.15s, color 0.15s; }
.msg-bubble:hover .copy-btn { opacity:0.6; }
.copy-btn:hover { opacity:1 !important; }
.copy-btn.copied { opacity:1 !important; color:var(--success); }

.markdown-body { min-width:0; overflow-wrap:anywhere; word-break:break-word; }
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
  .msg { max-width:95%; }
}
</style>
