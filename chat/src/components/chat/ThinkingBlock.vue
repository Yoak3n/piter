<script setup lang="ts">
import { ChevronRight, Brain } from "lucide-vue-next";

defineProps<{
  thinking: string;
  expanded: boolean;
  streaming?: boolean;
}>();

const emit = defineEmits<{
  (e: "toggle"): void;
}>();
</script>

<template>
  <div class="thinking-block" :class="{ streaming }">
    <div
      class="thinking-header"
      :class="{ expanded }"
      @click="streaming ? undefined : emit('toggle')"
    >
      <ChevronRight :size="12" class="thinking-chevron" :class="{ expanded }" />
      <Brain :size="12" />
      <span class="thinking-label">{{ $t("chat.thinking") }}</span>
      <span v-if="streaming" class="thinking-dots-inline">
        <span class="thinking-dot" />
        <span class="thinking-dot" />
        <span class="thinking-dot" />
      </span>
    </div>
    <div v-if="expanded" class="thinking-content" :class="{ expanded }">
      {{ thinking }}
    </div>
  </div>
</template>

<style scoped>
.thinking-block {
  background:var(--bg-muted);
  border:1px solid var(--border);
  border-radius:var(--radius-md);
  overflow:hidden;
  align-self:flex-start;
  max-width:90%;
  font-size:13px;
  transition:border-color var(--duration-fast) var(--ease);
}
.thinking-block:hover { border-color:var(--border-strong); }
.thinking-header {
  display:flex; align-items:center; gap:8px;
  padding:8px 12px; cursor:pointer; user-select:none;
  font-size:12px; color:var(--text-tertiary);
  transition:background 0.15s var(--ease);
}
.thinking-block.streaming .thinking-header { cursor:default; }
.thinking-header:hover { background:var(--bg-hover); }
.thinking-label { font-family:var(--font-mono); font-size:11px; }
.thinking-chevron { transition:transform 0.2s var(--ease); opacity:0.4; flex-shrink:0; }
.thinking-chevron.expanded { transform:rotate(90deg); }
.thinking-content {
  padding:0 12px 12px; white-space:pre-wrap; overflow-wrap:anywhere;
  font-style:italic; border-top:1px solid var(--border);
  max-height:260px; overflow-y:auto; overscroll-behavior:auto;
  font-size:12px; line-height:1.5; color:var(--text-secondary);
}
.thinking-content.expanded { display:block; }
.thinking-dots-inline { display:flex; gap:3px; margin-left:4px; }
.thinking-dot {
  width:6px; height:6px; border-radius:50%;
  background:var(--text-tertiary);
  animation:thinkBounce 1.4s ease-in-out infinite;
}
.thinking-dot:nth-child(1) { animation-delay:0s; }
.thinking-dot:nth-child(2) { animation-delay:0.2s; }
.thinking-dot:nth-child(3) { animation-delay:0.4s; }
.thinking-dots-inline .thinking-dot { width:4px; height:4px; }
@keyframes thinkBounce { 0%,80%,100%{ transform:scale(0.6); opacity:0.4; } 40%{ transform:scale(1); opacity:1; } }
</style>
