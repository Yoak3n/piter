<script setup lang="ts">
import { ref } from "vue";
import type { ChatTurn } from "../../types";
import { imageContentToSrc } from "../../utils/image";
import ThinkingBlock from "./ThinkingBlock.vue";
import ToolCard from "./ToolCard.vue";
import MarkdownBubble from "./MarkdownBubble.vue";
import ExtensionUiCard from "./ExtensionUiCard.vue";

defineProps<{
  turn: ChatTurn;
  /** 跨会话搜索命中消息的 id（非 null 时对应消息闪一下高亮） */
  highlightId?: number | null;
}>();

const emit = defineEmits<{
  (e: "respond-extension", payload: { id: string; answer: { value?: string; confirmed?: boolean; cancelled?: boolean } }): void;
}>();

// Per-turn expand state (isolated per turn, no cross-turn collisions)
const expandedThinking = ref<Set<number>>(new Set());

function toggleThinking(id: number) {
  if (expandedThinking.value.has(id)) expandedThinking.value.delete(id);
  else expandedThinking.value.add(id);
}

// 根元素 ref：父级（MessageTimeline）跳转滚动用。
// 通过 defineExpose 暴露真实元素而非依赖 $el——不关心组件是否单根，
// 即使模板结构变为多根 fragment，也不会拿到注释占位节点。
const turnEl = ref<HTMLElement | null>(null);
defineExpose({ turnEl });
</script>

<template>
  <div ref="turnEl" class="turn">
    <template v-for="sysMsg in turn.system" :key="sysMsg.id">
      <ExtensionUiCard
        v-if="sysMsg.extUi"
        :request="sysMsg.extUi"
        @respond="emit('respond-extension', $event)"
      />
      <div
        v-else
        class="msg system-msg"
        :class="{ 'msg-highlight': highlightId === sysMsg.id }"
      >{{ sysMsg.content }}</div>
    </template>
    <div
      v-if="turn.user"
      class="user-block"
      :class="{ 'is-slash': turn.user.meta?.slashCommand, 'msg-highlight': highlightId === turn.user.id }"
    >
      <span v-if="turn.user.meta?.slashCommand" class="slash-exec-label">{{ $t("chat.slashExecuted") }}</span>
      <div v-if="turn.user.images?.length" class="msg-images user-images">
        <div v-for="(img, i) in turn.user.images" :key="i" class="msg-image-item">
          <img :src="imageContentToSrc(img)" :alt="img.mimeType" :title="img.mimeType" class="msg-image" />
        </div>
      </div>
      <MarkdownBubble
        v-if="turn.user.content"
        mode="user"
        :content="turn.user.content"
        :muted="turn.user.meta?.slashCommand === true"
        :class="{ 'msg-highlight': highlightId === turn.user.id }"
      />
    </div>
    <template v-for="(assistant, idx) in turn.assistants" :key="assistant.id">
      <ThinkingBlock
        v-if="assistant.thinking"
        :thinking="assistant.thinking"
        :expanded="expandedThinking.has(assistant.id)"
        @toggle="toggleThinking(assistant.id)"
      />
      <div v-if="assistant.toolExecutions?.length" class="tool-executions">
        <ToolCard v-for="te in assistant.toolExecutions" :key="te.toolCallId" :tool="te" />
      </div>
      <div v-if="assistant.images?.length" class="msg-images assistant-images">
        <div v-for="(img, i) in assistant.images" :key="i" class="msg-image-item">
          <img :src="imageContentToSrc(img)" :alt="img.mimeType" :title="img.mimeType" class="msg-image" />
        </div>
      </div>
      <MarkdownBubble
        v-if="assistant.content"
        mode="assistant"
        :content="assistant.content"
        :timestamp="idx === turn.assistants.length - 1 ? assistant.timestamp : undefined"
        :class="{ 'msg-highlight': highlightId === assistant.id }"
      />
    </template>
  </div>
</template>

<style scoped>
.turn { display:flex; flex-direction:column; gap:6px; min-width:0; }
/* 跨会话搜索跳转命中的消息：短暂闪光提示 */
.msg-highlight { animation: msgFlash 2.4s var(--ease); }
@keyframes msgFlash {
  0% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--chart-3) 60%, transparent); border-radius: var(--radius-md); }
  100% { box-shadow: 0 0 0 3px transparent; }
}
.system-msg { align-self:center; font-size:10px; color:var(--text-tertiary); background:var(--bg-muted); padding:2px 10px; border-radius:var(--radius-sm); min-width:0; }
.tool-executions { display:flex; flex-direction:column; gap:4px; align-self:flex-start; max-width:90%; min-width:0; }
.user-block { display:flex; flex-direction:column; align-items:flex-end; align-self:flex-end; max-width:90%; min-width:0; gap:4px; }
/* 面板执行的 slash 命令（meta.slashCommand）：整组弱化 + 灰显标签 */
.user-block.is-slash { gap:2px; opacity:0.85; }
.slash-exec-label { font-size:10px; color:var(--text-tertiary); padding:0 4px; user-select:none; }

/* ── Message images (user thumbnails + assistant image blocks) ── */
.msg-images { display:flex; flex-wrap:wrap; gap:6px; max-width:100%; min-width:0; }
.assistant-images { align-self:flex-start; }
.msg-image-item {
  border-radius:var(--radius-md); overflow:hidden; flex-shrink:0;
  border:1px solid var(--border); background:var(--bg-panel);
  max-width:100%;
}
.msg-image { display:block; max-width:100%; max-height:320px; object-fit:contain; }
@media (max-width: 640px) {
  .user-block { max-width:95%; }
  .msg-image-item { max-width:180px; }
  .msg-image { max-height:220px; }
}
</style>
