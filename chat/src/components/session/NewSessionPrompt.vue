<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { Paperclip, FileText, X } from "lucide-vue-next";
import type { Attachment, ModelRef } from "../../types";
import { useFileDrop } from "../../composables/useFileDrop";
import { filesToAttachments, clipboardImageFiles } from "../../utils/attachments";
import { formatBytes, imageContentToSrc } from "../../utils/image";

// ─── 新会话首条消息输入（含附件拖拽/粘贴/选择）──
// 自持 firstMessage + 附件暂存；提交时把 (message, attachments) 交给父级组装 create 载荷。

const { t } = useI18n();

const props = defineProps<{
  /** pi 是否已连接（未连接时不响应文件拖拽） */
  isRunning: boolean;
  /** 当前会话模型（用于多模态预检弱提示） */
  currentModel?: ModelRef | null;
  /** 是否可提交（未选目录时禁用发送按钮，校验在父级） */
  canCreate: boolean;
}>();

const emit = defineEmits<{
  (e: "submit", payload: { message: string; attachments: Attachment[] }): void;
}>();

const firstMessage = ref("");
const pendingAttachments = ref<Attachment[]>([]);
const hintMsg = ref("");
/** 拖拽命中测试目标（Tauri 原生拖拽需按坐标判断是否落在输入区） */
const promptAreaRef = ref<HTMLElement | null>(null);
const fileInputRef = ref<HTMLInputElement | null>(null);

let hintTimer: ReturnType<typeof setTimeout> | null = null;
function showHint(msg: string) {
  hintMsg.value = msg;
  if (hintTimer) clearTimeout(hintTimer);
  hintTimer = setTimeout(() => { hintMsg.value = ""; }, 4000);
}

function removeAttachment(id: string) {
  pendingAttachments.value = pendingAttachments.value.filter((a) => a.id !== id);
}

/** 处理拖入/选中的文件（与 Composer 共用 filesToAttachments） */
async function addFiles(files: File[]) {
  const added = await filesToAttachments(files, {
    t: (k) => t(k),
    currentModel: props.currentModel,
    onHint: showHint,
  });
  if (added.length) pendingAttachments.value.push(...added);
}

/** 按钮选择文件（移动端/浏览器没有 OS 拖拽时的入口） */
function handleFiles(e: Event) {
  const input = e.target as HTMLInputElement;
  const files = Array.from(input.files || []);
  // 清空 value 以便再次选择同一文件
  input.value = "";
  if (!files.length) return;
  addFiles(files);
}

/** 粘贴图片（如 QQ 截图 Ctrl+V）→ 进入附件链路；剪贴板无图片时走默认文本粘贴 */
function handlePaste(e: ClipboardEvent) {
  if (!props.isRunning) return;
  const files = clipboardImageFiles(e);
  if (!files.length) return;
  e.preventDefault();
  addFiles(files);
}

const { isDragging, onDragEnter, onDragOver, onDragLeave, onDrop } = useFileDrop({
  enabled: () => props.isRunning,
  onFiles: addFiles,
  target: promptAreaRef,
});

function handleSubmit() {
  emit("submit", { message: firstMessage.value, attachments: pendingAttachments.value });
}
</script>

<template>
  <div class="prompt-wrap">
    <div v-if="pendingAttachments.length" class="pending-attachments">
      <div
        v-for="att in pendingAttachments"
        :key="att.id"
        class="attachment-chip"
        :title="att.fileName"
      >
        <img v-if="att.type === 'image' && att.data" :src="imageContentToSrc(att)" class="attachment-thumb" alt="" />
        <span v-else class="attachment-file-icon"><FileText :size="14" /></span>
        <span class="attachment-name">{{ att.fileName }}</span>
        <span class="attachment-size">{{ formatBytes(att.size) }}</span>
        <button
          class="attachment-remove"
          :title="t('chat.removeAttachment')"
          :aria-label="t('chat.removeAttachment')"
          @click="removeAttachment(att.id)"
        >
          <X :size="12" />
        </button>
      </div>
    </div>
    <div
      ref="promptAreaRef"
      class="prompt-area"
      :class="{ 'is-dragging': isDragging }"
      @dragenter="onDragEnter"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <div v-if="isDragging" class="prompt-drop-overlay" aria-hidden="true">
        <Paperclip :size="18" />
        <span>{{ $t("chat.dropFilesHint") }}</span>
      </div>
      <input
        v-model="firstMessage"
        type="text"
        class="prompt-input"
        :placeholder="$t('chat.promptPlaceholder')"
        @keydown.enter="handleSubmit"
        @paste="handlePaste"
      />
      <input
        ref="fileInputRef"
        type="file"
        class="file-input"
        accept="image/*,.txt,.md,.json,.csv,.log"
        multiple
        @change="handleFiles"
      />
      <button
        v-if="isRunning"
        class="attach-btn"
        :title="t('chat.attachFiles')"
        :aria-label="t('chat.attachFiles')"
        @click="fileInputRef?.click()"
      >
        <Paperclip :size="16" />
      </button>
      <button class="send-btn" @click="handleSubmit" :disabled="!canCreate">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/>
        </svg>
      </button>
    </div>
    <p v-if="hintMsg" class="prompt-hint" role="status">{{ hintMsg }}</p>
    <p class="hint">{{ $t("chat.paneHint") }}</p>
  </div>
</template>

<style scoped>
.prompt-wrap {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.prompt-area {
  position: relative;
  display: flex;
  gap: 0.75rem;
}

.prompt-drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  border: 1.5px dashed var(--accent);
  border-radius: var(--radius-lg);
  background: color-mix(in srgb, var(--accent) 8%, var(--bg-panel));
  color: var(--accent);
  font-size: 0.95rem;
  font-weight: 500;
  pointer-events: none;
}

.prompt-area.is-dragging .prompt-input {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.pending-attachments {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.attachment-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 220px;
  padding: 3px 6px 3px 3px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-muted);
}

.attachment-thumb {
  width: 28px;
  height: 28px;
  border-radius: 4px;
  object-fit: cover;
  border: 1px solid var(--border);
  background: var(--bg-panel);
  flex-shrink: 0;
}

.attachment-file-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 4px;
  flex-shrink: 0;
  background: var(--bg-panel);
  color: var(--text-tertiary);
  border: 1px solid var(--border);
}

.attachment-name {
  font-size: 0.75rem;
  color: var(--text);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.attachment-size {
  font-size: 0.7rem;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.attachment-remove {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  padding: 0;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.attachment-remove:hover {
  background: var(--danger-soft);
  color: var(--danger);
}

.prompt-hint {
  color: var(--warning);
  font-size: 0.85rem;
  margin: 0;
}

.prompt-input {
  flex: 1;
  padding: 1rem 1.25rem;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-panel);
  color: var(--text);
  font-size: 1.1rem;
  outline: none;
  box-shadow: var(--shadow-sm);
}

.file-input { display: none; }

.attach-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  background: var(--bg-panel);
  color: var(--text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease), border-color var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.attach-btn:hover {
  color: var(--text);
  border-color: var(--border-strong);
  background: var(--bg-hover);
}

.prompt-input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.send-btn {
  width: 52px;
  height: 52px;
  border-radius: var(--radius-lg);
  border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  background: var(--accent-soft);
  color: var(--accent-strong);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease), border-color var(--duration-fast) var(--ease), transform 0.1s var(--ease);
}

.send-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.send-btn:not(:disabled):hover {
  background: var(--accent-glow);
  border-color: var(--accent);
}

.send-btn:not(:disabled):active {
  transform: scale(0.96);
}

.hint {
  font-size: 0.85rem;
  color: var(--text-tertiary);
  margin: 0;
  text-align: center;
}
</style>
