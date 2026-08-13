<script setup lang="ts">
import { ref, computed, nextTick, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { Copy, Check } from "lucide-vue-next";
import { marked } from "marked";
import { formatMessageTime } from "../../utils/message";

marked.setOptions({ breaks: true, gfm: true });

// ─── 链接渲染修正（LLM 输出常见坑）─────────────────────────────────
// 1) marked gfm 裸链接会把 URL 后的闭合标点/全角括号组吞进 href（如 "v0.2.1）" →
//    %EF%BC%89、"docs（说明）" 整组），点击 404 → 渲染时修剪尾部标点与全角括号组。
// 2) 桌面端（Tauri）点击链接应在系统浏览器打开，而非在 webview 内导航；
//    web 端 target=_blank 新标签打开（见 handleBodyClick）。
// 规则：
//  - 全角括号组（如 （说明））整体剪（URL 用 ASCII，全角组必是附加文本）；
//  - 无配对语义的标点（。，、；：!？等）直接剪；
//  - 单 `)`/`）` 仅当 URL 内没有未配对的 `(`/`（` 才剪（配对括号是 URL 合法部分，如维基 Foo_(bar)）。
function trimLinkTrailing(href: string): string {
  // 1) 连续全角括号组整体剪
  let s = href.replace(/(（[^）]*）)+$/g, "");
  // 2) 从尾部扫描剪标点与单闭合括号（配对检查）
  let i = s.length;
  while (i > 0) {
    const ch = s[i - 1];
    if (/[」』】、，,。.;；:：!！?？]/.test(ch)) {
      i--;
      continue;
    }
    if (ch === ")" || ch === "）") {
      const openCh = ch === ")" ? "(" : "（";
      let open = 0;
      for (let j = 0; j < i - 1; j++) if (s[j] === openCh) open++;
      if (open > 0) break; // 配对了 URL 内的开括号 → 保留
      i--;
      continue;
    }
    break;
  }
  return s.slice(0, i);
}

/** 修剪尾部标点/括号组；仅放行安全 scheme（http/https/mailto/tel/相对/锚点），
 *  防 javascript: 等注入（沿用既有 XSS 加固思路）。返回 null 表示丢弃链接。 */
function safeLinkHref(href: string): string | null {
  const trimmed = trimLinkTrailing(href);
  if (/^(https?:|mailto:|tel:|#|\/)/i.test(trimmed)) return trimmed;
  if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(trimmed)) return trimmed; // 无 scheme（相对路径）
  return null;
}

marked.use({
  renderer: {
    link({ href, title, text }) {
      const safe = safeLinkHref(href);
      if (safe === null) return escapeHtml(text); // 危险 scheme：仅显示文本，不生成链接
      const display = text === href ? safe : text.replace(/(（[^）]*）)+$/, "");
      const titleAttr = title ? ` title="${escapeHtml(title)}"` : "";
      return `<a href="${escapeHtml(safe)}" target="_blank" rel="noopener noreferrer"${titleAttr}>${display}</a>`;
    },
    image({ href, title, text }) {
      const safe = safeLinkHref(href);
      if (safe === null) return "";
      const alt = text ? ` alt="${escapeHtml(text)}"` : "";
      const titleAttr = title ? ` title="${escapeHtml(title)}"` : "";
      return `<img src="${escapeHtml(safe)}"${alt}${titleAttr}>`;
    },
  },
});

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const { t } = useI18n();

const props = defineProps<{
  content: string;
  mode: "user" | "assistant" | "streaming";
  timestamp?: number;
  /** 弱化显示（如面板执行的 slash 命令：灰显"已执行命令"） */
  muted?: boolean;
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
    langEl.textContent = lang || t("chat.code");

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "code-copy-btn";
    btn.title = t("chat.copyCode");
    btn.setAttribute("aria-label", t("chat.copyCode"));
    btn.innerHTML = COPY_ICON;

    header.append(langEl, btn);
    wrapper.append(header);
    // 先让 wrapper 占据 pre 的位置（此时 pre 仍是 root 的子节点），
    // 再把 pre 移入 wrapper——避免先移动 pre 导致 insertBefore 抛 NotFoundError。
    pre.replaceWith(wrapper);
    wrapper.append(pre);
  });
}

function handleBodyClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  // 链接：桌面端（Tauri）在系统浏览器打开（webview 内导航会破坏应用）；
  // web 端 target=_blank 已生效，保持默认（新标签）。
  const link = target.closest<HTMLAnchorElement>("a");
  if (link && link.href) {
    if (isTauri) {
      e.preventDefault();
      import("@tauri-apps/plugin-opener")
        .then(({ openUrl }) => openUrl(link.href))
        .catch(() => window.open(link.href, "_blank"));
    }
    return;
  }
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
    <div
      class="msg-bubble"
      :class="mode === 'user' ? (muted ? 'user-bubble user-bubble-muted' : 'user-bubble') : 'assistant-bubble'"
    >
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
        :aria-label="$t('chat.copyMessage')"
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
/* 用户气泡的宽度上限由父级 .user-block（max-width:90%/95%）承担；
   若这里再套 90%，会相对 shrink-to-fit 后的父宽再压缩一次（90%×90%），
   导致短消息（如 8 个汉字）提前换行。 */
.user-msg { align-self:flex-end; align-items:flex-end; max-width:100%; }
.assistant-msg { align-self:flex-start; align-items:flex-start; }

.msg-bubble { border-radius:var(--radius-md); padding:8px 12px; line-height:1.5; font-size:13px; position:relative; min-width:0; max-width:100%; }
.user-bubble { background:var(--accent-soft); border:1px solid color-mix(in srgb, var(--accent) 15%, transparent); }
/* 面板执行的 slash 命令：灰显（纯动作类命令不产 agent turn，弱化其存在感） */
.user-bubble-muted { background:var(--bg-muted); border:1px solid var(--border); color:var(--text-tertiary); }
.user-bubble-muted .markdown-body { opacity:0.75; }
.assistant-bubble { background:var(--bg-panel); border:1px solid var(--border); }

.msg-time { font-size:10px; color:var(--text-tertiary); margin-top:2px; padding:0 4px; user-select:none; }

.cursor-blink { display:inline-block; width:6px; height:14px; background:var(--accent); animation:blink 1s step-end infinite; vertical-align:text-bottom; }
@keyframes blink { 50% { opacity:0; } }

/* Copy button */
.copy-btn { position:absolute; top:6px; right:6px; opacity:0; display:flex; align-items:center; justify-content:center; width:24px; height:24px; border:none; background:var(--bg-muted); border-radius:var(--radius-sm); color:var(--text-tertiary); cursor:pointer; transition:opacity 0.15s, color 0.15s; }
.msg-bubble:hover .copy-btn { opacity:0.6; }
.copy-btn:hover { opacity:1 !important; }
.copy-btn.copied { opacity:1 !important; color:var(--success); }

.markdown-body { min-width:0; overflow-wrap:anywhere; word-break:break-word; }
.markdown-body :deep(a){ color:var(--accent); text-decoration:underline; text-decoration-thickness:1px; text-underline-offset:2px; word-break:break-all; }
.markdown-body :deep(a:hover){ opacity:0.85; }
.markdown-body :deep(h1),.markdown-body :deep(h2),.markdown-body :deep(h3){ margin:0.4em 0 0.2em; line-height:1.3; }
.markdown-body :deep(h1){ font-size:1.15em; }
.markdown-body :deep(h2){ font-size:1.05em; }
.markdown-body :deep(h3){ font-size:1em; }
.markdown-body :deep(p){ margin:0.2em 0; }
.markdown-body :deep(ul),.markdown-body :deep(ol){ margin:0.2em 0; padding-left:1.4em; }
.markdown-body :deep(code){ font-family:var(--font-mono); font-size:0.85em; background:var(--bg-muted); padding:1px 4px; border-radius:3px; }
.markdown-body :deep(pre){ margin:0; padding:10px 12px; background:var(--code-bg); color:var(--code-text); border-radius:0 0 var(--radius-sm) var(--radius-sm); overflow-x:auto; scrollbar-width:thin; scrollbar-color:color-mix(in srgb, var(--code-text) 25%, transparent) transparent; font-family:var(--font-mono); font-size:13px; line-height:1.6; }
/* 代码块内横向滚动条：细滚动条，内容超宽时可见可横滚（PC/移动端一致） */
.markdown-body :deep(pre::-webkit-scrollbar){ height:6px; }
.markdown-body :deep(pre::-webkit-scrollbar-track){ background:transparent; }
.markdown-body :deep(pre::-webkit-scrollbar-thumb){ background:color-mix(in srgb, var(--code-text) 25%, transparent); border-radius:3px; }
.markdown-body :deep(pre::-webkit-scrollbar-thumb:hover){ background:color-mix(in srgb, var(--code-text) 45%, transparent); }
.markdown-body :deep(pre code){ background:none; padding:0; font-size:inherit; }
.markdown-body :deep(blockquote){ margin:0.3em 0; padding-left:10px; border-left:2px solid var(--border-strong); color:var(--text-secondary); }

/* 表格：块内横向滚动，列宽自适应容器，不撑破文档 */
.markdown-body :deep(table){ display:block; max-width:100%; overflow-x:auto; border-collapse:collapse; }
.markdown-body :deep(th), .markdown-body :deep(td){ padding:4px 8px; border:1px solid var(--border); }

/* 图片：限制最大宽度，避免大图/带宽度属性图片溢出被裁剪 */
.markdown-body :deep(img){ max-width:100%; height:auto; }

/* 兜底：所有子元素不超出容器（防御内联 HTML / 固定宽度元素） */
.markdown-body :deep(*){ max-width:100%; }

/* Code block wrapper (decorated at runtime: header + pre) */
.markdown-body :deep(.code-block){ margin:0.4em 0; border-radius:var(--radius-sm); overflow:hidden; max-width:100%; min-width:0; background:var(--code-bg); }
.markdown-body :deep(.code-block-header){ display:flex; align-items:center; justify-content:space-between; padding:3px 6px 3px 12px; background:color-mix(in srgb, var(--code-bg) 70%, #000 30%); border-bottom:1px solid color-mix(in srgb, var(--code-bg) 80%, #fff 10%); }
.markdown-body :deep(.code-lang){ font-family:var(--font-mono); font-size:11px; color:var(--text-tertiary); user-select:none; text-transform:lowercase; }
.markdown-body :deep(.code-copy-btn){ display:flex; align-items:center; justify-content:center; width:22px; height:22px; border:none; border-radius:4px; background:transparent; color:var(--text-tertiary); cursor:pointer; opacity:0; transition:opacity 0.15s, background 0.15s, color 0.15s; }
.markdown-body :deep(.code-block:hover .code-copy-btn){ opacity:0.7; }
.markdown-body :deep(.code-copy-btn:hover){ opacity:1 !important; background:color-mix(in srgb, var(--code-text) 12%, transparent); }
.markdown-body :deep(.code-copy-btn.copied){ opacity:1 !important; color:var(--state-idle); }

@media (max-width: 640px) {
  .msg { max-width:95%; }
  /* 移动端同样避免二次压缩（见 .user-msg 注释） */
  .user-msg { max-width:100%; }
}
</style>
