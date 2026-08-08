<script setup lang="ts">
import { ref, watch, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { Send, Maximize, Square, Zap, Clock, X, Paperclip, FileText, Info } from "lucide-vue-next";
import type { PendingItem } from "../../composables/usePiConnection";
import type { Attachment, ModelRef, SlashCommand } from "../../types";
import { useFileDrop } from "../../composables/useFileDrop";
import { useRecentCommands } from "../../composables/useRecentCommands";
import { filesToAttachments, clipboardImageFiles } from "../../utils/attachments";
import { formatBytes, imageContentToSrc } from "../../utils/image";
import SlashMenu, { type SlashMenuPosition } from "./SlashMenu.vue";

const { t } = useI18n();

const props = defineProps<{
  modelValue: string;
  isRunning: boolean;
  isStreaming: boolean;
  /** 本地待投递队列（可取消 / 升级为插队） */
  outbox?: PendingItem[];
  /** pi 原生插队队列（只读展示） */
  steeringQueue?: string[];
  /** 待发送附件（图片/文本），状态由父级按 session 持有 */
  attachments?: Attachment[];
  /** 当前会话模型（用于多模态预检弱提示） */
  currentModel?: ModelRef | null;
  /** 外部触发的提示（如切换模型后仍有附图）：key 递增时重新显示 */
  visionHint?: { text: string; key: number } | null;
  /** 当前会话的 pi 斜杠命令列表（null = 未加载/失败，输入 / 时懒加载拉取） */
  slashCommands?: SlashCommand[] | null;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "send"): void;
  (e: "steer"): void;
  (e: "abort"): void;
  (e: "cancel-queued", id: number): void;
  (e: "upgrade-queued", id: number): void;
  (e: "expand"): void;
  (e: "restart-pi"): void;
  (e: "update:attachments", attachments: Attachment[]): void;
  /** 首字符 / 触发补全但命令缓存为空时请求拉取（懒加载） */
  (e: "fetch-slash-commands"): void;
}>();

const inputRef = ref<HTMLTextAreaElement | null>(null);
const fileInputRef = ref<HTMLInputElement | null>(null);
/** 拖拽命中测试目标（Tauri 原生拖拽需按坐标判断是否落在输入区） */
const composerBoxRef = ref<HTMLElement | null>(null);

function autoGrow() {
  const el = inputRef.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = `${el.scrollHeight}px`;
}

function onInput(e: Event) {
  emit("update:modelValue", (e.target as HTMLTextAreaElement).value);
  nextTick(autoGrow);
  refreshSlashMenu();
}

watch(() => props.modelValue, () => nextTick(autoGrow));

// ── 斜杠命令补全（/ → pi 命令列表，触发语义对齐 pi TUI）─────────────────

const slashMenuOpen = ref(false);
const slashQuery = ref("");
const slashHighlight = ref(0);
const slashFiltered = ref<SlashCommand[]>([]);
/** 浮层定位（精确到 caret 字符列） */
const slashMenuPos = ref<SlashMenuPosition | null>(null);
/** 中文输入法选词中：不刷新补全、不拦截 Enter */
const composing = ref(false);

/** 最近使用命令（本地记录，模块级单例：与命令面板共享） */
const { recent, record: recordRecentCommand, reorderByRecent } = useRecentCommands();

/** caret 所在行从行首到 caret 的文本（触发检测用） */
function caretLinePrefix(): string {
  const el = inputRef.value;
  if (!el) return "";
  const before = el.value.slice(0, el.selectionStart);
  return before.slice(before.lastIndexOf("\n") + 1);
}

/**
 * 精确测量 caret 相对输入区（.composer-main）的字符列坐标（mirror div 法）：
 * 复制 textarea 的排版样式到一个隐藏 div，在 caret 处插入 marker span 测量其位置，
 * 减去滚动偏移并校正 border/padding，得到 caret 的实际像素坐标。
 */
function measureCaret(): { x: number; y: number } | null {
  const el = inputRef.value;
  if (!el) return null;
  const cs = window.getComputedStyle(el);
  const mirror = document.createElement("div");
  mirror.setAttribute("aria-hidden", "true");
  mirror.style.cssText = [
    "position:absolute;top:0;left:0;visibility:hidden;pointer-events:none;",
    "white-space:pre-wrap;overflow-wrap:break-word;word-break:break-word;",
    `font-family:${cs.fontFamily};font-size:${cs.fontSize};font-weight:${cs.fontWeight};`,
    `font-style:${cs.fontStyle};line-height:${cs.lineHeight};letter-spacing:${cs.letterSpacing};`,
    `word-spacing:${cs.wordSpacing};text-indent:${cs.textIndent};text-transform:${cs.textTransform};`,
    `tab-size:${cs.tabSize};`,
    `padding:${cs.paddingTop} ${cs.paddingRight} ${cs.paddingBottom} ${cs.paddingLeft};`,
    `border-top:${cs.borderTopWidth} ${cs.borderTopStyle} ${cs.borderTopColor};`,
    `border-right:${cs.borderRightWidth} ${cs.borderRightStyle} ${cs.borderRightColor};`,
    `border-bottom:${cs.borderBottomWidth} ${cs.borderBottomStyle} ${cs.borderBottomColor};`,
    `border-left:${cs.borderLeftWidth} ${cs.borderLeftStyle} ${cs.borderLeftColor};`,
    `width:${el.clientWidth}px;box-sizing:${cs.boxSizing};`,
  ].join("");
  document.body.appendChild(mirror);
  const caret = el.selectionStart;
  const before = document.createTextNode(el.value.slice(0, caret));
  const after = document.createTextNode(el.value.slice(caret));
  const marker = document.createElement("span");
  mirror.appendChild(before);
  mirror.appendChild(marker);
  mirror.appendChild(after);
  const x = marker.offsetLeft - el.scrollLeft;
  const y = marker.offsetTop - el.scrollTop;
  document.body.removeChild(mirror);
  const borderLeft = parseFloat(cs.borderLeftWidth) || 0;
  const borderTop = parseFloat(cs.borderTopWidth) || 0;
  // offsetLeft/offsetTop 是 textarea 相对 .composer-main（position:relative）的 border box 位置
  return { x: el.offsetLeft + x + borderLeft, y: el.offsetTop + y + borderTop };
}

/** 菜单定位：默认显示在 caret 行的上方（不遮挡输入行），left 对齐 caret 字符列；
 *  但 caret 贴近容器顶部时向上弹会溢出（首行场景）→ 翻转为向下弹出 */
const MENU_EST_HEIGHT = 260;
function updateSlashMenuPos() {
  const parent = inputRef.value?.parentElement;
  const pos = measureCaret();
  if (!parent || !pos) return;
  if (pos.y < MENU_EST_HEIGHT) {
    slashMenuPos.value = { left: pos.x, top: pos.y + 8 };
  } else {
    slashMenuPos.value = { left: pos.x, bottom: parent.clientHeight - pos.y + 8 };
  }
}

/** 触发语义：caret 所在行以 / 开头即触发（对齐 pi TUI 0.51.6+），
 *  前缀词为 ^/([\w:.-]*)；无匹配（如 / 后是普通文本/路径）→ 菜单隐藏，不打扰 */
function refreshSlashMenu() {
  if (composing.value) return; // IME 选词期间不刷新补全
  const el = inputRef.value;
  if (!el) return;
  const m = caretLinePrefix().match(/^\/([\w:.-]*)$/);
  if (!m) {
    slashMenuOpen.value = false;
    return;
  }
  const cmds = props.slashCommands ?? null;
  if (cmds === null) {
    // 缓存为空（首次 / 或上次拉取失败）：请求拉取，数据到达后由 watcher 重新刷新
    emit("fetch-slash-commands");
    return;
  }
  const query = m[1].toLowerCase();
  // 最近使用命令置顶（recent 中且命中当前过滤词的提前，保持最近使用顺序）
  const filtered = reorderByRecent(
    cmds.filter((c) => {
      const n = c.name.toLowerCase();
      return n.startsWith(query) || n.includes(query);
    }),
  );
  // 过滤词变化才重置高亮（↑↓ 只移动 caret、不改文本，保留高亮）
  if (query !== slashQuery.value) slashHighlight.value = 0;
  slashQuery.value = query;
  slashFiltered.value = filtered;
  slashMenuOpen.value = filtered.length > 0;
  if (slashMenuOpen.value) updateSlashMenuPos();
}

// 命令列表到达（懒加载成功后）：caret 行仍匹配 / 前缀则立即弹出菜单；
// 切会话/缓存失效（置 null）时立即关闭，防止残留上一个会话的过期项
watch(() => props.slashCommands, (cmds) => {
  if (cmds === null || cmds === undefined) {
    slashMenuOpen.value = false;
    slashFiltered.value = [];
    return;
  }
  refreshSlashMenu();
});

// 最近使用记录变化：菜单开着时重新排序置顶项（查询词不变，不重置高亮）
watch(recent, () => {
  if (slashMenuOpen.value) refreshSlashMenu();
});

/** 选中命令：把行首 /前缀词 替换为 "/name "，caret 置于末尾；行内后续内容原样保留 */
function acceptSlash(cmd: SlashCommand) {
  const el = inputRef.value;
  if (!el) return;
  const start = el.selectionStart;
  const before = el.value.slice(0, start);
  const lineStart = before.lastIndexOf("\n") + 1;
  const inserted = `/${cmd.name} `;
  const newValue = el.value.slice(0, lineStart) + inserted + el.value.slice(start);
  emit("update:modelValue", newValue);
  slashMenuOpen.value = false;
  slashFiltered.value = [];
  recordRecentCommand(cmd.name); // 本地记录，用于下次补全/面板置顶
  nextTick(() => {
    const caret = lineStart + inserted.length;
    el.focus();
    el.setSelectionRange(caret, caret);
    autoGrow();
  });
}

/** ↑↓←→ 等导航键只移动 caret、不触发 input：单独刷新补全状态 */
function onKeyup(e: KeyboardEvent) {
  if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown"].includes(e.key)) {
    refreshSlashMenu();
  }
}

function onCompositionStart() {
  composing.value = true;
}

function onCompositionEnd() {
  composing.value = false;
  refreshSlashMenu();
}

function handleKeydown(e: KeyboardEvent) {
  if (composing.value) return; // IME 选词期间不拦截 Enter、不做补全导航
  // 菜单打开时键盘导航优先于现有 Enter 发送逻辑
  if (slashMenuOpen.value && slashFiltered.value.length > 0) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      slashHighlight.value = (slashHighlight.value + 1) % slashFiltered.value.length;
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      slashHighlight.value = (slashHighlight.value - 1 + slashFiltered.value.length) % slashFiltered.value.length;
      return;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      acceptSlash(slashFiltered.value[slashHighlight.value]);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      slashMenuOpen.value = false;
      return;
    }
  }
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    emit("send");
  }
}

/** 粘贴图片（如 QQ 截图 Ctrl+V）→ 进入附件链路；剪贴板无图片时走默认文本粘贴 */
function handlePaste(e: ClipboardEvent) {
  const files = clipboardImageFiles(e);
  if (!files.length) return;
  e.preventDefault();
  addFiles(files);
}

// ── 附件 ───────────────────────────────────────────────────────────────

function removeAttachment(id: string) {
  emit("update:attachments", (props.attachments || []).filter((a) => a.id !== id));
}

/** 处理一组文件（图片压缩 / 文本读取），按钮选择与拖拽共用。 */
async function addFiles(files: File[]) {
  const added = await filesToAttachments(files, {
    t: (k) => t(k),
    currentModel: props.currentModel,
    onHint: showHint,
  });
  if (added.length) {
    emit("update:attachments", [...(props.attachments || []), ...added]);
  }
}

function handleFiles(e: Event) {
  const input = e.target as HTMLInputElement;
  const files = Array.from(input.files || []);
  // 清空 value 以便再次选择同一文件
  input.value = "";
  if (!files.length) return;
  addFiles(files);
}

// ── 拖拽附件（行为与准备页共用 useFileDrop） ─────────────────────────────

const { isDragging, onDragEnter, onDragOver, onDragLeave, onDrop } = useFileDrop({
  enabled: () => props.isRunning,
  onFiles: addFiles,
  target: composerBoxRef,
});

// ── 提示条（选图不支持 / 文件过大等，数秒后自动消失） ──────────────────

const hintMsg = ref("");
let hintTimer: ReturnType<typeof setTimeout> | null = null;
function showHint(msg: string) {
  hintMsg.value = msg;
  if (hintTimer) clearTimeout(hintTimer);
  hintTimer = setTimeout(() => {
    hintMsg.value = "";
  }, 4000);
}

// 外部提示（如切换模型后仍带附图）——key 变化时重新触发显示
watch(
  () => props.visionHint,
  (hint, prev) => {
    if (hint?.text && (!prev || prev.key !== hint.key)) showHint(hint.text);
  },
);
</script>

<template>
  <div class="composer">
    <div
      v-if="outbox?.length || steeringQueue?.length"
      class="composer-queue"
    >
      <span class="queue-label">{{ $t("chat.queue") }}</span>
      <span
        v-for="(m, i) in steeringQueue"
        :key="`s${i}`"
        class="queue-chip queue-chip-steer"
        :title="m"
      >
        <Zap :size="10" />{{ m }}
      </span>
      <span
        v-for="item in outbox"
        :key="`o${item.id}`"
        class="queue-chip queue-chip-followup"
        :title="item.text"
      >
        <Clock :size="10" /><span class="queue-chip-text">{{ item.text }}</span>
        <button
          class="chip-btn chip-upgrade"
          :title="$t('chat.upgradeQueued')"
          :aria-label="$t('chat.upgradeQueued')"
          @click.stop="emit('upgrade-queued', item.id)"
        >
          <Zap :size="10" />
        </button>
        <button
          class="chip-btn chip-cancel"
          :title="$t('chat.cancelQueued')"
          :aria-label="$t('chat.cancelQueued')"
          @click.stop="emit('cancel-queued', item.id)"
        >
          <X :size="10" />
        </button>
      </span>
    </div>
    <div
      ref="composerBoxRef"
      class="composer-box"
      :class="{ 'is-dragging': isDragging }"
      @dragenter="onDragEnter"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <div v-if="isDragging" class="composer-drop-overlay" aria-hidden="true">
        <Paperclip :size="18" />
        <span>{{ $t("chat.dropFilesHint") }}</span>
      </div>
      <div v-if="hintMsg" class="composer-hint-bar" role="status">
        <Info :size="12" />{{ hintMsg }}
      </div>
      <div v-if="attachments?.length" class="composer-attachments">
        <div
          v-for="att in attachments"
          :key="att.id"
          class="attachment-chip"
          :title="att.fileName"
        >
          <img v-if="att.type === 'image' && att.data" :src="imageContentToSrc(att)" class="attachment-thumb" alt="" />
          <span v-else class="attachment-file-icon"><FileText :size="12" /></span>
          <span class="attachment-name">{{ att.fileName }}</span>
          <span class="attachment-size">{{ formatBytes(att.size) }}</span>
          <button
            class="attachment-remove"
            :title="$t('chat.removeAttachment')"
            :aria-label="$t('chat.removeAttachment')"
            @click="removeAttachment(att.id)"
          >
            <X :size="10" />
          </button>
        </div>
      </div>
      <div class="composer-main">
        <input
          ref="fileInputRef"
          type="file"
          class="file-input"
          accept="image/*,.txt,.md,.json,.csv,.log"
          multiple
          @change="handleFiles"
        />
        <textarea
          ref="inputRef"
          :value="modelValue"
          class="composer-input"
          :placeholder="isRunning ? $t('chat.composerPlaceholder') : $t('chat.disconnected')"
          :disabled="!isRunning"
          rows="2"
          @input="onInput"
          @keydown="handleKeydown"
          @keyup="onKeyup"
          @paste="handlePaste"
          @click="refreshSlashMenu"
          @scroll="refreshSlashMenu"
          @blur="slashMenuOpen = false"
          @compositionstart="onCompositionStart"
          @compositionend="onCompositionEnd"
        />
        <SlashMenu
          v-if="slashMenuOpen && slashFiltered.length"
          :commands="slashFiltered"
          :highlight="slashHighlight"
          :position="slashMenuPos ?? undefined"
          @select="acceptSlash"
        />
        <div class="composer-btns">
          <button
            v-if="isRunning"
            class="composer-tool-btn composer-attach-btn"
            :title="$t('chat.attachFiles')"
            :aria-label="$t('chat.attachFiles')"
            @click="fileInputRef?.click()"
          >
            <Paperclip :size="14" />
          </button>
          <button
            v-if="isRunning"
            class="composer-tool-btn composer-expand-btn"
            :title="$t('chat.expandEditor')"
            :aria-label="$t('chat.expandEditor')"
            @click="emit('expand')"
          >
            <Maximize :size="14" />
          </button>
          <button
            v-if="isStreaming"
            class="composer-tool-btn composer-stop-btn"
            :title="$t('chat.stopGeneration')"
            :aria-label="$t('chat.stopGeneration')"
            @click="emit('abort')"
          >
            <Square :size="14" />
          </button>
          <button
            v-if="isStreaming"
            class="composer-tool-btn composer-steer-btn"
            :title="$t('chat.insertNow')"
            :aria-label="$t('chat.insertNow')"
            :disabled="!modelValue.trim() && !attachments?.length"
            @click="emit('steer')"
          >
            <Zap :size="14" />
          </button>
          <button
            class="composer-send-btn"
            :disabled="!isRunning || (!modelValue.trim() && !attachments?.length)"
            :title="isStreaming ? $t('chat.sendAfter') : $t('chat.send')"
            :aria-label="$t('chat.send')"
            @click="emit('send')"
          >
            <Clock v-if="isStreaming" :size="15" />
            <Send v-else :size="15" />
          </button>
        </div>
      </div>
      <div class="composer-hint">
        <template v-if="isRunning">
          <template v-if="isStreaming">{{ $t("chat.composerHintStreaming") }}</template>
          <template v-else>{{ $t("chat.composerHintIdle") }}</template>
        </template>
        <template v-else>
          <button class="btn-ghost-sm" @click="emit('restart-pi')">{{ $t("chat.reconnect") }}</button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.composer { flex-shrink:0; border-top:1px solid var(--border); background:var(--bg-panel); box-shadow:var(--shadow-composer); }
.composer-box { position:relative; display:flex; flex-direction:column; padding:10px 12px; gap:6px; }
.composer-drop-overlay {
  position:absolute; inset:0; z-index:10;
  display:flex; align-items:center; justify-content:center; gap:8px;
  border:1.5px dashed var(--accent); border-radius:var(--radius-lg);
  background:color-mix(in srgb, var(--accent) 8%, var(--bg-panel));
  color:var(--accent); font-size:13px; font-weight:500;
  pointer-events:none;
}
.composer-box.is-dragging .composer-input { border-color:var(--accent); }
.composer-main { position:relative; }
.composer-input {
  width:100%; min-height:56px; max-height:180px;
  padding:12px 52px 12px 14px;
  border:1px solid var(--border); border-radius:var(--radius-lg);
  background:var(--bg); color:var(--text);
  font-size:13px; line-height:1.5; resize:none; outline:none;
  font-family:var(--font); overflow-y:auto;
  transition:border-color var(--duration-fast) var(--ease), box-shadow var(--duration-fast) var(--ease);
}
.composer-input:hover { border-color:var(--border-strong); }
.composer-input:focus { border-color:var(--accent); box-shadow:var(--focus-ring); }
.composer-input:disabled { opacity:0.4; }
.composer-btns { position:absolute; right:8px; bottom:8px; display:flex; align-items:center; gap:6px; }
.composer-tool-btn {
  display:flex; align-items:center; justify-content:center;
  width:30px; height:30px; border-radius:8px; border:none;
  background:transparent; color:var(--text-tertiary); cursor:pointer;
  transition:background 0.15s var(--ease), color 0.15s var(--ease);
}
.composer-tool-btn:hover { background:var(--bg-hover); color:var(--text); }
.composer-expand-btn { display:none; }
.composer-send-btn {
  display:flex; align-items:center; justify-content:center;
  width:34px; height:34px; border-radius:50%;
  border:1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  background:var(--accent-soft); color:var(--accent-strong); cursor:pointer; flex-shrink:0;
  transition:background var(--duration-fast) var(--ease), border-color var(--duration-fast) var(--ease), transform 0.1s var(--ease), opacity var(--duration-fast) var(--ease);
}
.composer-send-btn:hover { background:var(--accent-glow); border-color:var(--accent); }
.composer-send-btn:active { transform:scale(0.95); }
.composer-send-btn:disabled { opacity:0.3; cursor:default; transform:none; }
.composer-hint { font-size:10px; color:var(--text-tertiary); }

/* ── 提示条 + 附件预览 ─────────────────────────────── */
.file-input { display:none; }
.composer-hint-bar {
  display:flex; align-items:center; gap:6px;
  font-size:11px; line-height:1.4; color:var(--warning);
  background:color-mix(in srgb, var(--warning) 10%, transparent);
  border:1px solid color-mix(in srgb, var(--warning) 30%, transparent);
  border-radius:var(--radius-sm); padding:4px 8px; flex-shrink:0;
}
.composer-hint-bar svg { flex-shrink:0; }
.composer-attachments {
  display:flex; flex-wrap:wrap; gap:6px; flex-shrink:0;
  max-height:96px; overflow-y:auto;
}
.attachment-chip {
  display:inline-flex; align-items:center; gap:6px;
  max-width:200px; padding:3px 6px 3px 3px; border-radius:var(--radius-sm);
  border:1px solid var(--border); background:var(--bg-muted); flex-shrink:0;
}
.attachment-thumb {
  width:28px; height:28px; border-radius:4px; object-fit:cover;
  border:1px solid var(--border); background:var(--bg-panel); flex-shrink:0;
}
.attachment-file-icon {
  display:flex; align-items:center; justify-content:center;
  width:28px; height:28px; border-radius:4px; flex-shrink:0;
  background:var(--bg-panel); color:var(--text-tertiary);
  border:1px solid var(--border);
}
.attachment-name {
  font-size:11px; color:var(--text); min-width:0;
  overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
}
.attachment-size { font-size:10px; color:var(--text-tertiary); flex-shrink:0; }
.attachment-remove {
  display:flex; align-items:center; justify-content:center;
  width:18px; height:18px; border:none; border-radius:4px; padding:0;
  background:transparent; color:var(--text-tertiary); cursor:pointer; flex-shrink:0;
  transition:background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}
.attachment-remove:hover { background:var(--danger-soft); color:var(--danger); }

/* ── Queue indicator ─────────────────────────────── */
.composer-queue {
  display:flex; align-items:center; gap:6px;
  padding:6px 12px 0; overflow-x:auto; flex-shrink:0;
}
.queue-label { font-size:10px; color:var(--text-tertiary); flex-shrink:0; }
.queue-chip {
  display:inline-flex; align-items:center; gap:4px;
  max-width:150px; padding:2px 8px; border-radius:999px;
  font-size:10px; line-height:1.4; white-space:nowrap; overflow:hidden;
  text-overflow:ellipsis; border:1px solid var(--border);
  background:var(--bg); color:var(--text-secondary); flex-shrink:0;
}
.queue-chip svg { flex-shrink:0; }
/* 文本包 span 可收缩省略（min-width:0），⚡/✕ 按钮 flex-shrink:0 不被挤走，始终可见 */
.queue-chip-text { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.queue-chip-followup { border-color:var(--border); color:var(--text-secondary); }
.queue-chip-steer { border-color:var(--accent); color:var(--accent); }
.chip-btn {
  display:inline-flex; align-items:center; justify-content:center;
  width:16px; height:16px; border:none; border-radius:4px; padding:0;
  background:transparent; color:inherit; cursor:pointer; opacity:0.6; flex-shrink:0;
  transition:opacity 0.15s var(--ease), background 0.15s var(--ease);
}
.chip-btn:hover { opacity:1; background:var(--bg-hover); }
.chip-upgrade:hover { color:var(--accent); }
.chip-cancel:hover { color:var(--danger); }

.composer-stop-btn:hover { background:var(--danger-soft); color:var(--danger); }
.composer-steer-btn:hover { background:var(--accent-soft); color:var(--accent); }
.composer-steer-btn:disabled { opacity:0.3; cursor:default; }

/* 桌面端放宽队列 chip 宽度，长消息少省略 */
@media (min-width: 641px) {
  .queue-chip { max-width:400px; }
}

@media (max-width: 640px) {
  .composer-box { padding-bottom:calc(10px + env(safe-area-inset-bottom)); }
  .composer-input { font-size:16px; min-height:60px; }  /* prevent iOS zoom */
  .composer-expand-btn { display:flex; }
}
</style>
