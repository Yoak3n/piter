<script setup lang="ts">
import { computed, ref, nextTick, watch, onMounted, onBeforeUnmount } from "vue";
import { ArrowDown } from "lucide-vue-next";
import { EmptyState } from "@piter/ui";
import type { ChatTurn, Message, ToolExecution } from "../../types";
import MessageTurn from "./MessageTurn.vue";
import ThinkingBlock from "./ThinkingBlock.vue";
import ToolCard from "./ToolCard.vue";
import MarkdownBubble from "./MarkdownBubble.vue";
import TimelineNav from "./TimelineNav.vue";

const props = defineProps<{
  turns: ChatTurn[];
  isStreaming: boolean;
  currentAssistantContent: string;
  currentThinking?: string;
  toolExecutions?: ToolExecution[];
  /** 跨会话搜索跳转目标：滚动到 timestamp/内容匹配的消息并高亮 */
  scrollTo?: { sessionId?: string; timestamp?: number; query: string } | null;
}>();

const emit = defineEmits<{
  (e: "respond-extension", payload: { id: string; answer: { value?: string; confirmed?: boolean; cancelled?: boolean } }): void;
  (e: "scroll-handled"): void;
}>();

const timelineRef = ref<HTMLDivElement | null>(null);

// ─── Search-jump scrolling ─────────────────────────────────────────────
// 搜索命中后 switchSession，消息快照异步到达（turns 逐次更新）。watch 在
// 每次 turns 变化时尝试定位目标消息，命中即滚动 + 高亮，并上报 scroll-handled
// 让父级清除目标（未命中则保持，等下一批消息/快照继续尝试）。
const turnRefs = ref<Record<number, HTMLElement | null>>({});
const highlightId = ref<number | null>(null);
let highlightTimer: ReturnType<typeof setTimeout> | null = null;

function flatMessages(): (Message & { turnId: number })[] {
  const out: (Message & { turnId: number })[] = [];
  for (const turn of props.turns) {
    const push = (m: Message | null) => {
      if (m && m.content) out.push({ ...m, turnId: turn.id });
    };
    push(turn.user);
    for (const a of turn.assistants) push(a);
    for (const s of turn.system) push(s);
  }
  return out;
}

function locateTarget(scrollTo: { timestamp?: number; query: string }): (Message & { turnId: number }) | null {
  const msgs = flatMessages();
  if (!msgs.length) return null;
  const q = scrollTo.query.toLowerCase();
  const tsMatches = scrollTo.timestamp !== undefined
    ? msgs.filter((m) => m.timestamp === scrollTo.timestamp)
    : [];
  if (tsMatches.length === 1) return tsMatches[0];
  const tsAndQ = tsMatches.find((m) => m.content.toLowerCase().includes(q));
  if (tsAndQ) return tsAndQ;
  return msgs.find((m) => m.content.toLowerCase().includes(q)) ?? tsMatches[0] ?? null;
}

/** MessageTurn 根元素 ref：优先取 defineExpose 暴露的 turnEl（多根 fragment 下
 *  $el 是注释占位节点，须避开）；未暴露时回退 $el，兜底 null */
function setTurnRef(id: number, el: unknown) {
  turnRefs.value[id] =
    (el as { turnEl?: HTMLElement } | null)?.turnEl ?? (el as { $el?: HTMLElement } | null)?.$el ?? null;
}

watch(
  [() => props.scrollTo, () => props.turns],
  async () => {
    const target = props.scrollTo;
    if (!target) return;
    const hit = locateTarget(target);
    if (!hit) return; // 消息还没到，等下一次 turns 变化
    await nextTick();
    // 加固：只有拿到真实元素才滚动（防御 ref 拿到的注释占位/空值）
    if (!turnRefs.value[hit.turnId]) return;
    scrollTurnIntoView(hit.turnId);
    highlightId.value = hit.id;
    if (highlightTimer) clearTimeout(highlightTimer);
    highlightTimer = setTimeout(() => {
      highlightId.value = null;
    }, 2600);
    emit("scroll-handled");
  },
);

// ─── Timeline nav (侧边时间轴导航) ──────────────────────────────────────
// 轨道上的每条横线 = 一轮 user 消息；当前 turn = 视口滚动位置对应的最近
// 轮次（视口驱动，与流式输出解耦）。锚点复用搜索跳转的 turnRefs。
const userTurns = computed(() => props.turns.filter((t) => t.user));

/** turnId → 该轮根元素在 .timeline 内的 offsetTop（px），布局后由 rAF 刷新 */
const positions = ref<Record<number, number>>({});
/** 当前视口对应的轮次 id（传给 TimelineNav 高亮为声波） */
const activeTurnId = ref<number | null>(null);

let rafId: number | null = null;
function scheduleLayoutUpdate() {
  if (rafId != null) return;
  rafId = requestAnimationFrame(() => {
    rafId = null;
    refreshPositions();
    updateActiveTurn();
  });
}

// positions = minimap 坐标：内容高度按比例缩放到固定轨道高度
// （clientHeight/scrollHeight），全部横线常驻轨道 → 小星星全貌可见、
// 任意横线点击跳转（与滚动位置无关）。clamp 到轨道可见范围内，保证
// 首尾横线完整可点（首轮 offsetTop 含 16px padding，末轮可能越出轨道底）。
function refreshPositions() {
  const el = timelineRef.value;
  if (!el) return;
  const total = el.scrollHeight;
  if (total <= 0) return;
  const scale = el.clientHeight / total;
  const trackBottom = Math.max(0, el.clientHeight - 6); // 命中区半高 6px
  const next: Record<number, number> = {};
  for (const t of userTurns.value) {
    const turnEl = turnRefs.value[t.id];
    if (turnEl && typeof turnEl.offsetTop === "number") {
      next[t.id] = Math.min(Math.max(turnEl.offsetTop * scale, 0), trackBottom);
    }
  }
  positions.value = next;
}

// 当前 turn = 视口中心线经过的最后一个 user turn 锚点（用内容坐标 offsetTop
// 判定，与 minimap 显示坐标无关；几百轮内线性遍历即可）
function updateActiveTurn() {
  const el = timelineRef.value;
  if (!el) {
    activeTurnId.value = null;
    return;
  }
  const list = userTurns.value;
  if (!list.length) {
    activeTurnId.value = null;
    return;
  }
  const refLine = el.scrollTop + el.clientHeight / 2;
  let active: number | null = null;
  for (const t of list) {
    const turnEl = turnRefs.value[t.id];
    if (!turnEl) continue;
    if (turnEl.offsetTop <= refLine) active = t.id;
    else break; // turns 按 DOM 顺序，offsetTop 递增
  }
  activeTurnId.value = active ?? list[0].id;
}

// 流式增长 / 图片加载 / 窗口尺寸变化都会改变 turn 高度 → ResizeObserver
// 捕获滚动容器内容盒变化，rAF 合并刷新位置 + 当前轮。
let layoutObserver: ResizeObserver | null = null;
onMounted(() => {
  if (timelineRef.value && typeof ResizeObserver !== "undefined") {
    layoutObserver = new ResizeObserver(() => scheduleLayoutUpdate());
    layoutObserver.observe(timelineRef.value);
  }
  scheduleLayoutUpdate();
});
onBeforeUnmount(() => layoutObserver?.disconnect());

// 轮次增删后（flush post 保证新元素已渲染）重新测量
watch(userTurns, () => scheduleLayoutUpdate(), { flush: "post" });

/** 滚动 .timeline 让该轮精确居中。手动计算 scrollTop（offsetTop 相对
 *  .timeline 的 padding 内边，与 updateActiveTurn 同一坐标系），绕开
 *  scrollIntoView 在不同浏览器/WebView 对 flex+gap+padding 容器的居中
 *  实现差异——这是"跳转不准确"的根因；且只滚动本容器，不碰祖先。 */
function scrollTurnIntoView(turnId: number, smooth = true) {
  const wrap = timelineRef.value;
  const el = turnRefs.value[turnId];
  if (!wrap || !el || typeof el.offsetTop !== "number") return;
  const centerTop = el.offsetTop - wrap.clientHeight / 2 + el.offsetHeight / 2;
  const maxTop = Math.max(0, wrap.scrollHeight - wrap.clientHeight);
  const target = Math.min(Math.max(centerTop, 0), maxTop);
  wrap.scrollTo({ top: target, behavior: smooth ? "smooth" : "auto" });
}

function jumpToTurn(turnId: number) {
  scrollTurnIntoView(turnId);
  // 与搜索跳转同款：跳转后置 paused，流式输出不把用户拽回底部；
  // scroll 事件会随之触发 handleScroll，rAF 内更新当前轮。
  isPaused.value = true;
}

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
  // 视口驱动的当前轮（rAF 节流）——与自动滚动判定共用滚动事件。
  scheduleLayoutUpdate();
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
  <div class="timeline-wrap">
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
      :ref="(el) => setTurnRef(turn.id, el)"
      :turn="turn"
      :highlight-id="highlightId"
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

    <!-- 侧边时间轴导航：固定 minimap 轨道（在滚动容器外，不随内容滚动），
         横线 = 全部 user 轮次的分布；当前轮显示声波；点击任意横线跳转 -->
    <TimelineNav
      :turns="userTurns"
      :active-turn-id="activeTurnId"
      :positions="positions"
      @jump="jumpToTurn"
    />
  </div>
</template>

<style scoped>
/* 固定包裹层：容纳滚动容器 + 固定在右侧的时间轴轨道（轨道不随内容滚动） */
.timeline-wrap {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.timeline {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  /* turn 元素 offsetTop 的 offsetParent 基准（内容坐标） */
  position: relative;
}

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
  z-index: 2; /* 位于时间轴轨道之上 */
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
