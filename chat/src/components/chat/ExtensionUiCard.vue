<script setup lang="ts">
import { ref, watch, computed } from "vue";
import { useI18n } from "vue-i18n";
import { Check, X } from "lucide-vue-next";
import type { ExtensionUiCard } from "../../types";

const props = defineProps<{
  request: ExtensionUiCard;
}>();

const emit = defineEmits<{
  (e: "respond", payload: { id: string; answer: { value?: string; confirmed?: boolean; cancelled?: boolean } }): void;
}>();

const { t } = useI18n();

const inputText = ref("");
// D5：editor 初始化为 prefill（快照恢复的卡片同样带入 prefill）
const editorText = ref(props.request.prefill ?? "");

// 换卡（组件按消息 id 复用实例）时复位输入
watch(
  () => props.request.id,
  () => {
    inputText.value = "";
    editorText.value = props.request.prefill ?? "";
  },
);

// ── 结果查看器识别 ──
// piolium 等扩展把 select 当"结果查看器"用：把命令输出逐行塞进 options，
// 末尾附 "Press Esc to return to chat." 关闭提示、标题带 "(… Esc …)" 提示。
// 这类不应渲染成可点选的编号菜单（会把文档/空行变成"选项"），而是只读文档 + 关闭按钮。
const DISMISS_HINT_RE = /press\s+.*?\besc\b.*?(?:return|close|dismiss|chat|back)/i;
const TITLE_ESC_HINT_RE = /\([^)]*\besc\b[^)]*\)/i;

const isResultViewer = computed(() => {
  const opts = props.request.options ?? [];
  if (opts.length === 0) return false;
  const last = opts[opts.length - 1] ?? "";
  return DISMISS_HINT_RE.test(last) || TITLE_ESC_HINT_RE.test(props.request.title);
});

/** 查看器内容：去掉末尾关闭提示行及其前的空行 */
const viewerLines = computed(() => {
  const lines = (props.request.options ?? []).slice(0, -1);
  while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines.join("\n");
});

// D7：rpiv 的 RPC walker 选项常自带 "1. label — desc" 编号，去掉前置编号避免与序号徽标重复
function displayOption(opt: string): string {
  return opt.replace(/^\d+[.)]\s+/, "");
}

function choose(value: string) {
  emit("respond", { id: props.request.id, answer: { value } });
}
function confirm(ok: boolean) {
  emit("respond", { id: props.request.id, answer: { confirmed: ok } });
}
function cancel() {
  emit("respond", { id: props.request.id, answer: { cancelled: true } });
}
function submit(value: string) {
  emit("respond", { id: props.request.id, answer: { value } });
}
</script>

<template>
  <div class="ext-card" :class="{ answered: request.answered }">
    <div class="ext-header">
      <span class="ext-title">{{ request.title }}</span>
      <span v-if="request.answered" class="ext-badge ext-badge--done">{{ t("chat.extensionDone") }}</span>
      <span v-else class="ext-badge">{{ t("chat.extensionPending") }}</span>
    </div>

    <!-- 已应答：只读历史 -->
    <div v-if="request.answered" class="ext-result">
      <Check v-if="request.result?.kind === 'value' || request.result?.kind === 'confirmed'" :size="14" class="ext-result-icon" />
      <X v-else :size="14" class="ext-result-icon ext-result-icon--x" />
      <span class="ext-result-text">
        {{
          request.result?.kind === "value"
            ? t("chat.extensionAnswerShown", { v: request.result.text ?? "" })
            : request.result?.kind === "confirmed"
              ? t("chat.extensionResultConfirmed")
              : request.result?.kind === "rejected"
                ? t("chat.extensionResultRejected")
                : t("chat.extensionResultCancelled")
        }}
      </span>
    </div>

    <!-- select：结果查看器（piolium 类）→ 只读文档 + 关闭；否则 → 点选选项 -->
    <div v-else-if="request.method === 'select'" class="ext-body">
      <template v-if="isResultViewer">
        <div class="ext-doc">{{ viewerLines }}</div>
        <div class="ext-actions ext-actions--right">
          <button class="btn btn-primary" @click="cancel">{{ t("common.close") }}</button>
        </div>
      </template>
      <template v-else>
        <div v-if="request.options?.length" class="ext-options">
          <button
            v-for="(opt, i) in request.options"
            :key="i"
            class="ext-option"
            @click="choose(opt)"
          >
            <span class="ext-opt-idx">{{ i + 1 }}</span>
            <span class="ext-opt-text">{{ displayOption(opt) }}</span>
          </button>
        </div>
        <p v-else class="ext-empty">{{ t("chat.extensionNoOptions") }}</p>
      </template>
    </div>

    <!-- confirm：接受 / 拒绝 -->
    <div v-else-if="request.method === 'confirm'" class="ext-body">
      <p v-if="request.message" class="ext-message">{{ request.message }}</p>
      <div class="ext-actions">
        <button class="btn btn-ghost ext-reject" @click="confirm(false)">
          <X :size="14" /> {{ t("chat.extensionReject") }}
        </button>
        <button class="btn btn-primary ext-accept" @click="confirm(true)">
          <Check :size="14" /> {{ t("chat.extensionAccept") }}
        </button>
      </div>
    </div>

    <!-- input：单行输入 -->
    <div v-else-if="request.method === 'input'" class="ext-body">
      <input
        v-model="inputText"
        class="input ext-input"
        :placeholder="request.placeholder || t('chat.extensionAnswer')"
        @keydown.enter.prevent="submit(inputText)"
      />
      <div class="ext-actions ext-actions--right">
        <button class="btn btn-primary" @click="submit(inputText)">{{ t("chat.extensionSubmit") }}</button>
      </div>
    </div>

    <!-- editor：多行编辑 -->
    <div v-else-if="request.method === 'editor'" class="ext-body">
      <textarea
        v-model="editorText"
        class="ext-editor"
        :placeholder="t('chat.extensionAnswer')"
        spellcheck="false"
      />
      <div class="ext-actions ext-actions--right">
        <button class="btn btn-primary" @click="submit(editorText)">{{ t("chat.extensionSubmit") }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ext-card {
  align-self: flex-start;
  width: min(460px, 100%);
  max-height: min(560px, calc(100vh - 48px));
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
  box-shadow: var(--shadow-sm);
  overflow: hidden;
}
.ext-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
}
.ext-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ext-badge {
  flex-shrink: 0;
  padding: 1px 8px;
  border-radius: var(--radius-pill);
  background: var(--accent-soft);
  color: var(--accent-strong);
  font-size: 9px;
  font-weight: 600;
}
.ext-badge--done {
  background: var(--bg-muted);
  color: var(--text-tertiary);
}
.ext-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
/* 结果查看器：只读文档（保留换行与空行，不被当成可选项） */
.ext-doc {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.6;
  color: var(--text);
  white-space: pre-wrap;
  word-break: break-word;
  min-width: 0;
}
.ext-message {
  margin: 0;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text);
  white-space: pre-wrap;
  word-break: break-word;
}
.ext-options {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ext-option {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg);
  color: var(--text);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease), border-color var(--duration-fast) var(--ease);
}
.ext-option:hover {
  background: var(--bg-hover);
  border-color: var(--border-hover);
}
.ext-option:active {
  background: var(--accent-soft);
  border-color: var(--accent);
}
.ext-opt-idx {
  flex-shrink: 0;
  min-width: 20px;
  height: 20px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  background: var(--bg-muted);
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 600;
}
.ext-opt-text {
  /* BUG-020：选项需基于完整文本选择，溢出省略（nowrap+ellipsis）破坏可读性 →
     改为自动换行完整显示；overflow-wrap:anywhere 让长单词/路径也换行不溢出。
     标题（.ext-title）与已应答摘要（.ext-result-text）保留单行省略是合理场景。 */
  white-space: normal;
  overflow-wrap: anywhere;
  min-width: 0;
}
.ext-empty {
  margin: 0;
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 8px 0;
}
.ext-input { width: 100%; }
.ext-editor {
  width: 100%;
  min-height: 120px;
  max-height: 280px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg);
  color: var(--text);
  font-size: 13px;
  line-height: 1.6;
  resize: vertical;
  outline: none;
  font-family: var(--font-mono);
  transition: border-color var(--duration) var(--ease), box-shadow var(--duration) var(--ease);
}
.ext-editor:hover { border-color: var(--border-hover); }
.ext-editor:focus { border-color: var(--accent); box-shadow: var(--focus-ring); }
.ext-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
.ext-accept { color: var(--success); }
.ext-reject { color: var(--danger); }
.ext-result {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  font-size: 12px;
  color: var(--text-secondary);
}
.ext-result-icon { color: var(--success); flex-shrink: 0; }
.ext-result-icon--x { color: var(--text-tertiary); }
.ext-result-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
