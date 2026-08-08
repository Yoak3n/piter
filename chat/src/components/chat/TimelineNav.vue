<script setup lang="ts">
import { computed } from "vue";
import type { ChatTurn } from "../../types";

const props = defineProps<{
  /** 仅含 user 消息的轮次（保持 DOM 顺序） */
  turns: ChatTurn[];
  /** 视口滚动位置对应的当前轮 id */
  activeTurnId: number | null;
  /** turnId → 该轮根元素在滚动容器内的 offsetTop（px） */
  positions: Record<number, number>;
}>();

const emit = defineEmits<{
  (e: "jump", turnId: number): void;
}>();

// ─── 《小星星》彩蛋 ────────────────────────────────────────────────
// 每个 turn 的波形高度 = 音阶（do=1..si=7）/7 × 最大高度；轨道上高度序列
// 连起来就是《小星星》第一句简谱（14 音，循环）：
//   1 1 5 5 6 6 5 | 4 4 3 3 2 2 1
const NOTES = [1, 1, 5, 5, 6, 6, 5, 4, 4, 3, 3, 2, 2, 1] as const;
const MAX_WAVE_PX = 20;

const noteByTurn = computed<Record<number, number>>(() => {
  const m: Record<number, number> = {};
  props.turns.forEach((t, i) => {
    m[t.id] = NOTES[i % NOTES.length];
  });
  return m;
});

function waveHeight(turnId: number): number {
  return (noteByTurn.value[turnId] / 7) * MAX_WAVE_PX;
}

// 连续波形 path：中间高两端低，竖直水平对称（上下镜像）。s=音阶/7 控制
// 幅度——低音矮、高音高。用二次贝塞尔（C）拼成平滑曲线，非离散柱状。
function wavePath(turnId: number): string {
  const s = noteByTurn.value[turnId] / 7;
  const top = 10 - 8 * s;
  const bot = 10 + 8 * s;
  return [
    "M 2 10",
    `C 5 ${top} 9 ${top} 12 ${top}`,
    `C 15 ${top} 19 ${top} 22 10`,
    `C 19 ${bot} 15 ${bot} 12 ${bot}`,
    `C 9 ${bot} 5 ${bot} 2 10`,
    "Z",
  ].join(" ");
}

/** hover 摘要：该轮 user 消息前 5 字（截断加省略号），不显示时间 */
function summaryOf(turn: ChatTurn): string {
  const c = (turn.user?.content ?? "").trim();
  if (!c) return "";
  return c.length > 5 ? `${c.slice(0, 5)}…` : c;
}
</script>

<template>
  <div v-if="turns.length >= 2" class="timeline-nav">
    <div
      v-for="turn in turns"
      :key="turn.id"
      v-show="positions[turn.id] != null"
      class="nav-item"
      :class="{ active: activeTurnId === turn.id }"
      :style="{
        top: `${positions[turn.id] ?? 0}px`,
        '--wave-h': `${waveHeight(turn.id).toFixed(1)}px`,
      }"
      :title="summaryOf(turn) || undefined"
      role="button"
      :aria-label="summaryOf(turn) || undefined"
      @click="emit('jump', turn.id)"
    >
      <span class="nav-line" />
      <svg class="nav-wave" viewBox="0 0 24 20" preserveAspectRatio="none">
        <path :d="wavePath(turn.id)" fill="var(--accent)" />
      </svg>
    </div>
  </div>
</template>

<style scoped>
.timeline-nav {
  /* 固定在包裹层（滚动容器外）右侧，铺满可见高度：不随内容滚动，
     全部横线常驻轨道（minimap），任意位置点击跳转 */
  position: absolute;
  top: 0;
  bottom: 0;
  right: 2px;
  width: 14px;
  overflow: hidden;
  z-index: 1;
  pointer-events: none;
}

.nav-item {
  position: absolute;
  left: 0;
  right: 0;
  /* 固定命中区高度，保证 hover/点击易用（波形在 flex 内居中，可向外溢出） */
  height: 12px;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  pointer-events: auto;
}

/* 非当前 turn：静态细横线 */
.nav-line {
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  height: 2px;
  transform: translateY(-50%);
  border-radius: 1px;
  background: var(--border);
  opacity: 0.55;
  transition: opacity var(--duration) var(--ease);
}

/* 当前 turn：细线淡出，波形从 0 高度生长出来（--duration/--ease 过渡） */
.nav-item.active .nav-line {
  opacity: 0;
}

.nav-wave {
  display: block;
  width: 100%;
  height: 0;
  transition: height var(--duration) var(--ease);
}

.nav-item.active .nav-wave {
  height: var(--wave-h);
}

/* 移动端：触摸目标小，收起轨道 */
@media (max-width: 640px) {
  .timeline-nav {
    display: none;
  }
}
</style>
