<script setup lang="ts">
import { ref, computed, nextTick, watch, onMounted } from "vue";
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

// ─── Copy helpers ─────────────────────────────────────────────────────────

function copyText(text: string): Promise<void> {
  return navigator.clipboard
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
}

// Copy the whole message
const copied = ref(false);
function copyMessage() {
  copyText(props.content).then(() => {
    copied.value = true;
    setTimeout(() => { copied.value = false; }, 1500);
  });
}

// ─── Per-code-block copy ──────────────────────────────────────────────────
// The markdown is injected via v-html, so we decorate the rendered `<pre>`
// elements after each render: wrap them in a `.code-block` header carrying a
// copy button. Clicks are handled by event delegation; the code text is read
// from the DOM (`pre.innerText`) so no HTML-escaping is involved.

const COPY_ICON =
  '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>';
const CHECK_ICON =
  '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>';

const bodyRef = ref<HTMLElement | null>(null);

function decorateCodeBlocks(root: HTMLElement) {
  root.querySelectorAll<HTMLPreElement>("pre").forEach((pre) => {
    if (pre.parentElement?.classList.contains("code-block")) return;
    const lang =
      pre.querySelector("code")?.className.match(/language-([\w-]+)/)?.[1] || "";

    const wrapper = document.createElement("div");
    wrapper.className = "code-block";

    const header = document.createElement("div");
    header.className = "code-block-header";

    const langEl = document.createElement("span");
    langEl.className = "code-lang";
    langEl.textContent = lang || "code";

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "code-copy-btn";
    btn.title = "Copy code";
    btn.setAttribute("aria-label", "Copy code");
    btn.innerHTML = COPY_ICON;

    header.append(langEl, btn);
    wrapper.append(header, pre);
    root.insertBefore(wrapper, pre);
  });
}

function handleBodyClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  const btn = target.closest<HTMLButtonElement>(".code-copy-btn");
  if (!btn) return;
  const pre = btn.closest<HTMLElement>(".code-block")?.querySelector("pre");
  if (!pre) return;
  copyText(pre.innerText).then(() => {
    btn.classList.add("copied");
    btn.innerHTML = CHECK_ICON;
    setTimeout(() => {
      btn.classList.remove("copied");
      btn.innerHTML = COPY_ICON;
    }, 1500);
  });
}

watch(
  () => [props.content, props.mode] as const,
  () => {
    nextTick(() => bodyRef.value && decorateCodeBlocks(bodyRef.value));
  },
  { flush: "post" },
);

onMounted(() => {
  bodyRef.value && decorateCodeBlocks(bodyRef.value);
});
</script>

<template>
  <div class="msg" :class="mode === 'user' ? 'user-msg' : 'assistant-msg'">
    <div class="msg-bubble" :class="mode === 'user' ? 'user-bubble' : 'assistant-bubble'">
      <div
        class="markdown-body"
        ref="bodyRef"
        v-html="renderMarkdown(content)"
        @click="handleBodyClick"
      />
      <button
        v-if="mode === 'assistant'"
        class="copy-btn"
        :class="{ copied }"
        aria-label="Copy message"
        @click="copyMessage"
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
.markdown-body :deep(pre){ margin:0; padding:10px 12px; background:var(--color-code-bg); color:var(--color-code-text); border-radius:0 0 8px 8px; overflow-x:auto; font-family:var(--font-family-mono); font-size:13px; line-height:1.6; }
.markdown-body :deep(pre code){ background:none; padding:0; font-size:inherit; }
.markdown-body :deep(blockquote){ margin:0.3em 0; padding-left:10px; border-left:2px solid var(--color-border-strong); color:var(--color-text-secondary); }

/* Code block wrapper (decorated at runtime: header + pre) */
.markdown-body :deep(.code-block){ margin:0.4em 0; border-radius:8px; overflow:hidden; background:var(--color-code-bg); }
.markdown-body :deep(.code-block-header){ display:flex; align-items:center; justify-content:space-between; padding:3px 6px 3px 12px; background:color-mix(in srgb, var(--color-code-bg) 70%, #000 30%); border-bottom:1px solid color-mix(in srgb, var(--color-code-bg) 80%, #fff 10%); }
.markdown-body :deep(.code-lang){ font-family:var(--font-family-mono); font-size:11px; color:var(--color-text-tertiary); user-select:none; text-transform:lowercase; }
.markdown-body :deep(.code-copy-btn){ display:flex; align-items:center; justify-content:center; width:22px; height:22px; border:none; border-radius:4px; background:transparent; color:var(--color-text-tertiary); cursor:pointer; opacity:0; transition:opacity 0.15s, background 0.15s, color 0.15s; }
.markdown-body :deep(.code-block:hover .code-copy-btn){ opacity:0.7; }
.markdown-body :deep(.code-copy-btn:hover){ opacity:1 !important; background:color-mix(in srgb, var(--color-code-text) 12%, transparent); }
.markdown-body :deep(.code-copy-btn.copied){ opacity:1 !important; color:#34d399; }

@media (max-width: 640px) {
  .msg { max-width:95%; }
}
</style>
