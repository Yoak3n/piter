<script setup lang="ts">
import { ref } from "vue";
import type { ChatTurn } from "../../types";
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
    <MarkdownBubble v-if="turn.user" mode="user" :content="turn.user.content" />
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
.system-msg { align-self:center; font-size:10px; color:var(--color-text-tertiary); background:var(--color-bg-muted); padding:2px 10px; border-radius:10px; min-width:0; }
.tool-executions { display:flex; flex-direction:column; gap:4px; align-self:flex-start; max-width:90%; min-width:0; }
</style>
