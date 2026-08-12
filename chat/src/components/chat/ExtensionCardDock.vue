<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { Message } from "../../types";
import ExtensionUiCard from "./ExtensionUiCard.vue";

// ── BUG-019 修复（方案 C：卡片置底悬浮）───────────────────────────────
// 未应答的扩展 UI 卡片会随消息流被长输出挤出视口（卡片插在 turn.system，
// 渲染在 user 消息上方，流式输出后视口贴底 → 卡片不可见）。本 dock 从当前
// 会话消息流中收集未应答卡片，固定悬浮在 Composer 上方：贴底视口天然可见，
// 不打断阅读；卡片应答/超时取消后（extUi.answered=true）自动从 dock 消失，
// 消息流中的卡片仍保留为只读历史（数据同源，无需改动事件链路）。

const props = defineProps<{
  messages: Message[];
}>();

const emit = defineEmits<{
  (e: "respond", payload: { id: string; answer: { value?: string; confirmed?: boolean; cancelled?: boolean } }): void;
}>();

const { t } = useI18n();

/** 当前会话未应答的扩展 UI 卡片（与消息流同源：messages 里的 extUi） */
const pendingCards = computed(() =>
  props.messages.filter((m) => m.extUi && !m.extUi.answered).map((m) => m.extUi!),
);

// 至多同时展示 2 张（最新在下、贴近 Composer）；超出折叠为 +N，点击展开全部
const LIMIT = 2;
const expanded = ref(false);
const visibleCards = computed(() =>
  expanded.value ? pendingCards.value : pendingCards.value.slice(-LIMIT),
);
const hiddenCount = computed(() => Math.max(0, pendingCards.value.length - LIMIT));
</script>

<template>
  <div v-if="pendingCards.length" class="ext-dock" :class="{ 'ext-dock--expanded': expanded }">
    <div class="ext-dock__list">
      <ExtensionUiCard
        v-for="card in visibleCards"
        :key="card.id"
        :request="card"
        @respond="emit('respond', $event)"
      />
    </div>
    <button v-if="hiddenCount > 0" class="ext-dock__more" @click="expanded = !expanded">
      <template v-if="expanded">{{ t("chat.extDockCollapse") }}</template>
      <template v-else>{{ t("chat.extDockMore", { n: hiddenCount }) }}</template>
    </button>
  </div>
</template>

<style scoped>
/* 悬浮 dock：flex 布局插入 Composer 上方（时间线随之缩短，绝不遮挡输入区与消息）；
   与 Composer 同底色/分隔线，视觉上是一条"确认条" */
.ext-dock {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 12px;
  border-top: 1px solid var(--border);
  background: var(--bg-panel);
  box-shadow: var(--shadow-composer);
  min-width: 0;
}
/* 卡片列表独立滚动：多卡片/展开时整体限高，避免挤占时间线与输入区 */
.ext-dock__list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
  overflow-y: auto;
  overflow-x: hidden;
  max-height: min(440px, 42vh);
}
/* "+N 更多"气泡：始终可见（位于滚动列表之外） */
.ext-dock__more {
  flex-shrink: 0;
  align-self: center;
  padding: 2px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-pill);
  background: var(--bg);
  color: var(--text-secondary);
  font-size: 11px;
  cursor: pointer;
  user-select: none;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease), border-color var(--duration-fast) var(--ease);
}
.ext-dock__more:hover { background: var(--bg-hover); color: var(--text); border-color: var(--border-hover); }
.ext-dock__more:active { transform: scale(0.97); }

@media (max-width: 640px) {
  /* 窄视口：高度受限于视口 40%，内部滚动，不遮挡 Composer 输入 */
  .ext-dock__list { max-height: 40vh; }
}
</style>
