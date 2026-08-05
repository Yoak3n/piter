<script setup lang="ts">
import { ref } from "vue";
import type { ChatTurn } from "../../types";
import { imageContentToSrc } from "../../utils/image";
import ThinkingBlock from "./ThinkingBlock.vue";
import ToolCard from "./ToolCard.vue";
import MarkdownBubble from "./MarkdownBubble.vue";

defineProps<{
  turn: ChatTurn;
}>();

// Per-turn expand state (isolated per turn, no cross-turn collisions)
const expandedThinking = ref<Set<number>>(new Set());

function toggleThinking(id: number) {
  if (expandedThinking.value.has(id)) expandedThinking.value.delete(id);
  else expandedThinking.value.add(id);
}
</script>

<template>
  <div class="turn">
    <div v-if="turn.system" class="msg system-msg">{{ turn.system.content }}</div>
    <div v-if="turn.user" class="user-block">
      <div v-if="turn.user.images?.length" class="msg-images user-images">
        <div v-for="(img, i) in turn.user.images" :key="i" class="msg-image-item">
          <img :src="imageContentToSrc(img)" :alt="img.mimeType" :title="img.mimeType" class="msg-image" />
        </div>
      </div>
      <MarkdownBubble v-if="turn.user.content" mode="user" :content="turn.user.content" />
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
      />
    </template>
  </div>
</template>

<style scoped>
.turn { display:flex; flex-direction:column; gap:6px; min-width:0; }
.system-msg { align-self:center; font-size:10px; color:var(--text-tertiary); background:var(--bg-muted); padding:2px 10px; border-radius:var(--radius-sm); min-width:0; }
.tool-executions { display:flex; flex-direction:column; gap:4px; align-self:flex-start; max-width:90%; min-width:0; }
.user-block { display:flex; flex-direction:column; align-items:flex-end; align-self:flex-end; max-width:90%; min-width:0; gap:4px; }

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
