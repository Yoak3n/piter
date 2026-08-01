<script setup lang="ts">
import { ref, nextTick, watch, onUnmounted } from "vue";
import { ArrowDown } from "lucide-vue-next";
import type { ChatTurn, ToolExecution } from "../../types";
import MessageTurn from "./MessageTurn.vue";
import ThinkingBlock from "./ThinkingBlock.vue";
import ToolCard from "./ToolCard.vue";
import MarkdownBubble from "./MarkdownBubble.vue";

const props = defineProps<{
  turns: ChatTurn[];
  isStreaming: boolean;
  currentAssistantContent: string;
  currentThinking?: string;
  toolExecutions?: ToolExecution[];
}>();

const timelineRef = ref<HTMLDivElement | null>(null);

// ─── Auto-scroll with user-override ──────────────────────────────────
// While streaming, the timeline follows the latest content. If the user
// scrolls away from the bottom (to re-read), auto-scroll pauses; it resumes
// either when the user returns to the bottom or after a period of
// inactivity. A floating button jumps straight back to the bottom.

const isPaused = ref(false);
let pauseTimer: ReturnType<typeof setTimeout> | null = null;
const PAUSE_MS = 5000; // resume auto-scroll after this long without scrolling
const BOTTOM_THRESHOLD = 120; // px from bottom considered "at the bottom"

function clearPauseTimer() {
  if (pauseTimer) {
    clearTimeout(pauseTimer);
    pauseTimer = null;
  }
}

function scrollToBottom() {
  if (isPaused.value) return;
  nextTick(() => {
    if (timelineRef.value) timelineRef.value.scrollTop = timelineRef.value.scrollHeight;
  });
}

function handleScroll() {
  const el = timelineRef.value;
  if (!el) return;
  const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
  if (distFromBottom <= BOTTOM_THRESHOLD) {
    // User is back at the bottom — resume following.
    clearPauseTimer();
    isPaused.value = false;
    return;
  }
  // User is reading older content — pause, and arm a resume timer that
  // restarts on every scroll so active reading keeps it paused.
  isPaused.value = true;
  clearPauseTimer();
  pauseTimer = setTimeout(() => {
    isPaused.value = false;
    pauseTimer = null;
  }, PAUSE_MS);
}

function jumpToBottom() {
  clearPauseTimer();
  isPaused.value = false;
  scrollToBottom();
}

onUnmounted(clearPauseTimer);

watch(() => props.turns.length, scrollToBottom);
watch(() => props.isStreaming, scrollToBottom);
// Auto-scroll during streaming
watch(() => props.currentAssistantContent, scrollToBottom);
watch(() => props.currentThinking, scrollToBottom);
</script>

<template>
  <div ref="timelineRef" class="timeline" @scroll="handleScroll">
    <div v-if="turns.length === 0" class="empty-state">
      <div class="empty-icon">💬</div>
      <p>Chat with Pi, your coding agent.</p>
      <p class="empty-hint">Type a message below and press Enter.</p>
    </div>

    <MessageTurn v-for="turn in turns" :key="turn.id" :turn="turn" />

    <!-- Streaming state -->
    <div v-if="isStreaming" class="turn streaming-turn">
      <ThinkingBlock
        v-if="currentThinking"
        :thinking="currentThinking"
        :expanded="true"
        streaming
      />
      <div v-if="toolExecutions?.length" class="tool-executions">
        <ToolCard v-for="te in toolExecutions" :key="te.toolCallId" :tool="te" />
      </div>
      <MarkdownBubble v-if="currentAssistantContent" mode="streaming" :content="currentAssistantContent" />
      <!-- Thinking dots when nothing else to show -->
      <div v-if="!currentThinking && !currentAssistantContent && (!toolExecutions?.length)" class="msg assistant-msg">
        <div class="msg-bubble assistant-bubble thinking-bubble">
          <div class="thinking-dots">
            <span class="thinking-dot" />
            <span class="thinking-dot" />
            <span class="thinking-dot" />
          </div>
        </div>
      </div>
    </div>

    <!-- Floating "back to bottom" button, shown while auto-scroll is paused -->
    <button
      v-if="isPaused"
      class="scroll-to-bottom-btn"
      aria-label="Scroll to bottom"
      title="Scroll to bottom"
      @click="jumpToBottom"
    >
      <ArrowDown :size="18" />
    </button>
  </div>
</template>

<style scoped>
.timeline { flex:1; overflow-y:auto; overflow-x:hidden; padding:16px 12px; display:flex; flex-direction:column; gap:12px; }
.empty-state { display:flex; flex-direction:column; align-items:center; justify-content:center; height:100%; color:var(--color-text-tertiary); text-align:center; gap:4px; }
.empty-icon { font-size:2.5rem; }
.empty-hint { font-size:11px; }

.turn { display:flex; flex-direction:column; gap:6px; min-width:0; }
.msg { display:flex; max-width:90%; min-width:0; }
.assistant-msg { align-self:flex-start; }
.msg-bubble { border-radius:12px; padding:8px 12px; line-height:1.5; font-size:13px; position:relative; min-width:0; }
.assistant-bubble { background:var(--color-bg-panel); border:1px solid var(--color-border-subtle); }

.thinking-bubble { min-height:32px; display:flex; align-items:center; }
.thinking-dots { display:flex; gap:4px; padding:4px 0; }
.thinking-dot { width:6px; height:6px; border-radius:50%; background:var(--color-text-tertiary); animation:thinkBounce 1.4s ease-in-out infinite; }
.thinking-dot:nth-child(1) { animation-delay:0s; }
.thinking-dot:nth-child(2) { animation-delay:0.2s; }
.thinking-dot:nth-child(3) { animation-delay:0.4s; }
@keyframes thinkBounce { 0%,80%,100%{ transform:scale(0.6); opacity:0.4; } 40%{ transform:scale(1); opacity:1; } }

.tool-executions { display:flex; flex-direction:column; gap:4px; align-self:flex-start; max-width:90%; }

/* Floating "back to bottom" button — sticky inside the scroll container so it
   stays pinned to the visible bottom regardless of scroll position. */
.scroll-to-bottom-btn {
  position: sticky;
  bottom: 12px;
  align-self: flex-end;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  margin-top: auto;
  border-radius: 50%;
  border: 1px solid var(--color-border-subtle);
  background: var(--color-bg-panel);
  color: var(--color-text-secondary);
  cursor: pointer;
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.18);
  transition: background 0.15s var(--ease), color 0.15s var(--ease), transform 0.1s var(--ease);
}
.scroll-to-bottom-btn:hover { background: var(--color-bg-hover); color: var(--color-text-primary); }
.scroll-to-bottom-btn:active { transform: scale(0.94); }
</style>
