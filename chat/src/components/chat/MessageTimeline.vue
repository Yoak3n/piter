<script setup lang="ts">
import { ref, nextTick, watch } from "vue";
import { ArrowDown } from "lucide-vue-next";
import { EmptyState } from "@piter/ui";
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

const emit = defineEmits<{
  (e: "respond-extension", payload: { id: string; answer: { value?: string; confirmed?: boolean; cancelled?: boolean } }): void;
}>();

const timelineRef = ref<HTMLDivElement | null>(null);

// ─── Auto-scroll with user-override ──────────────────────────────────
// While streaming, the timeline follows the latest content. If the user
// scrolls away from the bottom (to re-read), auto-scroll pauses (sticky);
// it resumes when the user returns to the bottom or clicks the floating
// button. A floating button jumps straight back to the bottom.

const isPaused = ref(false);
const BOTTOM_THRESHOLD = 120; // px from bottom considered "at the bottom"

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
    isPaused.value = false;
    return;
  }
  // User is reading older content — pause auto-scroll (sticky). It stays
  // paused until the user returns to the bottom or clicks the floating
  // button. No timer: content growth (e.g. streaming thinking deltas)
  // must NOT yank the user back to the bottom while reading.
  isPaused.value = true;
}

// Wheel capture (capture phase): the streaming thinking block owns an inner
// scroll area (max-height + overflow-y:auto), so wheel events over it are
// swallowed there and never reach this container's @scroll handler. Pausing
// only on @scroll therefore never triggers while the user reads thinking
// text — the next think delta would yank the view back to the bottom (the
// "can't scroll up during thinking" bug). Capturing wheel at the timeline
// level sees every scroll intent (even ones the inner container eats) and
// pauses immediately.
// ⚠️ Do NOT judge by "current scroll position": in the capture phase the
// scroll has NOT happened yet, so when the user is at the bottom and wheels
// up, distFromBottom=0 and a position check never triggers. Judge by
// direction instead — deltaY<0 (wheel up) is an unambiguous "reading" intent.
function handleWheelCapture(e: WheelEvent) {
  if (isPaused.value) return;
  if (e.deltaY < 0) {
    // Immediately sticky-pause so think deltas can no longer yank the view.
    isPaused.value = true;
  }
}

// Touch scrolling (mobile): wheel events don't fire on touchscreens, and
// scroll doesn't bubble (so scrolling the thinking block's inner area never
// reaches this container's @scroll handler). Capturing touchmove here sees
// every gesture, and direction is judged by clientY movement — a finger
// swiping up means the user is reading.
let lastTouchY: number | null = null;

// Finger swipe up = reading intent → sticky pause (symmetric with wheel deltaY<0)
function handleTouchMoveCapture(e: TouchEvent) {
  const t = e.touches[0];
  if (!t) return;
  const y = t.clientY;
  if (lastTouchY !== null && y < lastTouchY) {
    isPaused.value = true;
  }
  lastTouchY = y;
}

function handleTouchEndCapture() {
  lastTouchY = null;
}

function jumpToBottom() {
  isPaused.value = false;
  scrollToBottom();
}

watch(() => props.turns.length, scrollToBottom);
watch(() => props.isStreaming, scrollToBottom);
// Auto-scroll during streaming
watch(() => props.currentAssistantContent, scrollToBottom);
watch(() => props.currentThinking, scrollToBottom);
</script>

<template>
  <div
    ref="timelineRef"
    class="timeline"
    @scroll="handleScroll"
    @wheel.capture="handleWheelCapture"
    @touchmove.capture="handleTouchMoveCapture"
    @touchend.capture="handleTouchEndCapture"
  >
    <EmptyState
      v-if="turns.length === 0"
      fill
      illustration
      :title="$t('chat.timelineEmptyTitle')"
      :hint="$t('chat.timelineEmptyHint')"
    >
      <template #icon>
        <svg
          class="empty-illustration"
          width="76"
          height="76"
          viewBox="0 0 76 76"
          fill="none"
          aria-hidden="true"
        >
          <!-- chat bubble -->
          <rect
            x="12"
            y="16"
            width="46"
            height="34"
            rx="15"
            fill="var(--accent-soft)"
            stroke="var(--accent)"
            stroke-width="1.5"
          />
          <path
            d="M24 50l5 -6h-9z"
            fill="var(--accent-soft)"
            stroke="var(--accent)"
            stroke-width="1.5"
            stroke-linejoin="round"
          />
          <!-- text lines -->
          <rect x="22" y="27" width="18" height="3.5" rx="1.75" fill="var(--accent)" opacity="0.45" />
          <rect x="22" y="36" width="26" height="3.5" rx="1.75" fill="var(--accent)" opacity="0.28" />
          <!-- sparkles -->
          <path
            d="M64 14l1.7 3.6 3.6 1.7-3.6 1.7L64 24.6l-1.7-3.6-3.6-1.7 3.6-1.7z"
            fill="var(--warning)"
            opacity="0.85"
          />
          <path
            d="M54 6l1.2 2.6 2.6 1.2-2.6 1.2L54 13.8l-1.2-2.6-2.6-1.2 2.6-1.2z"
            fill="var(--accent)"
            opacity="0.7"
          />
        </svg>
      </template>
    </EmptyState>

    <MessageTurn
      v-for="turn in turns"
      :key="turn.id"
      :turn="turn"
      @respond-extension="emit('respond-extension', $event)"
    />

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
      :aria-label="$t('chat.scrollToBottom')"
      :title="$t('chat.scrollToBottom')"
      @click="jumpToBottom"
    >
      <ArrowDown :size="18" />
    </button>
  </div>
</template>

<style scoped>
.timeline { flex:1; overflow-y:auto; overflow-x:hidden; padding:16px 12px; display:flex; flex-direction:column; gap:12px; }

.turn { display:flex; flex-direction:column; gap:6px; min-width:0; }
.msg { display:flex; max-width:90%; min-width:0; }
.assistant-msg { align-self:flex-start; }
.msg-bubble { border-radius:var(--radius-md); padding:8px 12px; line-height:1.5; font-size:13px; position:relative; min-width:0; }
.assistant-bubble { background:var(--bg-panel); border:1px solid var(--border); }

.thinking-bubble { min-height:32px; display:flex; align-items:center; }
.thinking-dots { display:flex; gap:4px; padding:4px 0; }
.thinking-dot { width:6px; height:6px; border-radius:50%; background:var(--text-tertiary); animation:thinkBounce 1.4s ease-in-out infinite; }
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
  border: 1px solid var(--border);
  background: var(--bg-panel);
  color: var(--text-secondary);
  cursor: pointer;
  box-shadow: var(--shadow-md);
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease), transform 0.1s var(--ease);
}
.scroll-to-bottom-btn:hover { background: var(--bg-hover); color: var(--text); }
.scroll-to-bottom-btn:active { transform: scale(0.94); }
</style>
