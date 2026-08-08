<script setup lang="ts">
import { ref, computed, watch, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { Search } from "lucide-vue-next";
import type { PaletteItem } from "../../types";

const props = defineProps<{
  open: boolean;
  /** 全部候选（动作命令 + 会话），命令源在 App.vue 构建（可扩展） */
  items: PaletteItem[];
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "run", item: PaletteItem): void;
}>();

const { t } = useI18n();

const query = ref("");
const highlight = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);

/** 子序列模糊匹配打分：全命中返回正分（首字符+3 / 连续+2 / 跳跃+1），未全命中返回 -1 */
function fuzzyScore(text: string, q: string): number {
  const tl = text.toLowerCase();
  let qi = 0;
  let score = 0;
  let last = -2;
  for (let ti = 0; ti < tl.length && qi < q.length; ti++) {
    if (tl[ti] === q[qi]) {
      score += ti === last + 1 ? 2 : 1;
      if (ti === 0) score += 3;
      last = ti;
      qi++;
    }
  }
  return qi === q.length ? score : -1;
}

interface Scored {
  item: PaletteItem;
  score: number;
}

/** 过滤 + 排序：空查询显示全部（按注册序），否则按分数降序 */
function match(items: PaletteItem[]): PaletteItem[] {
  const q = query.value.trim().toLowerCase();
  if (!q) return items;
  return items
    .map((item): Scored => {
      const text = `${item.title} ${item.keywords ?? ""} ${item.hint ?? ""}`;
      return { item, score: fuzzyScore(text, q) };
    })
    .filter((s) => s.score >= 0)
    .sort((a, b) => b.score - a.score)
    .map((s) => s.item);
}

const actions = computed(() => match(props.items.filter((i) => i.kind === "action")));
const slashes = computed(() => match(props.items.filter((i) => i.kind === "slash")));
const sessions = computed(() => match(props.items.filter((i) => i.kind === "session")));

type Row =
  | { type: "group"; label: string }
  | { type: "item"; item: PaletteItem; idx: number };

/** 分组行 + 条目行（idx 为条目在全部结果中的序号，供 ↑↓ 导航） */
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  let idx = 0;
  if (actions.value.length) {
    out.push({ type: "group", label: t("chat.cmdGroupActions") });
    for (const a of actions.value) out.push({ type: "item", item: a, idx: idx++ });
  }
  if (slashes.value.length) {
    out.push({ type: "group", label: t("chat.cmdGroupSlash") });
    for (const s of slashes.value) out.push({ type: "item", item: s, idx: idx++ });
  }
  if (sessions.value.length) {
    out.push({ type: "group", label: t("chat.cmdGroupSessions") });
    for (const s of sessions.value) out.push({ type: "item", item: s, idx: idx++ });
  }
  return out;
});

const itemCount = computed(() => actions.value.length + slashes.value.length + sessions.value.length);

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    if (itemCount.value) highlight.value = (highlight.value + 1) % itemCount.value;
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    if (itemCount.value) highlight.value = (highlight.value - 1 + itemCount.value) % itemCount.value;
  } else if (e.key === "Enter") {
    e.preventDefault();
    const row = rows.value.find((r) => r.type === "item" && r.idx === highlight.value);
    if (row && row.type === "item") emit("run", row.item);
  } else if (e.key === "Escape") {
    e.preventDefault();
    emit("close");
  }
}

function runItem(item: PaletteItem) {
  emit("run", item);
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      query.value = "";
      highlight.value = 0;
      nextTick(() => inputRef.value?.focus());
    }
  },
);

// 查询变化时高亮回到顶部
watch(query, () => {
  highlight.value = 0;
});

// items 动态变化（slash 懒加载完成 / 会话更新）时：
// 若当前高亮项仍存在则按 id 保持位置，否则钳制到新列表边界——避免 Enter 选中错项
watch(
  () => props.items,
  () => {
    const rowsArr = rows.value;
    const cur = rowsArr.find((r) => r.type === "item" && r.idx === highlight.value);
    if (cur && cur.type === "item") {
      const target = rowsArr.find((r) => r.type === "item" && r.item.id === cur.item.id);
      if (target && target.type === "item") {
        highlight.value = target.idx;
        return;
      }
    }
    if (itemCount.value > 0) {
      highlight.value = Math.min(highlight.value, itemCount.value - 1);
    }
  },
);
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="palette-backdrop" @mousedown.self="emit('close')">
      <div class="palette" role="dialog" aria-modal="true" :aria-label="$t('chat.cmdPalette')">
        <div class="palette__input-wrap">
          <Search :size="15" class="palette__search-icon" />
          <input
            ref="inputRef"
            v-model="query"
            class="palette__input"
            :placeholder="$t('chat.cmdPlaceholder')"
            @keydown="onKeydown"
          />
          <kbd class="palette__kbd">Esc</kbd>
        </div>
        <div v-if="itemCount" class="palette__list">
          <template v-for="row in rows" :key="row.type === 'group' ? `g:${row.label}` : `i:${row.item.id}`">
            <div v-if="row.type === 'group'" class="palette__group">{{ row.label }}</div>
            <button
              v-else
              class="palette__item"
              :class="{ 'is-active': row.idx === highlight }"
              @mousedown.prevent
              @mouseenter="highlight = row.idx"
              @click="runItem(row.item)"
            >
              <span class="palette__item-title">{{ row.item.title }}</span>
              <span v-if="row.item.hint" class="palette__item-hint">{{ row.item.hint }}</span>
              <span
                class="palette__item-kind"
                :class="`kind-${row.item.kind}`"
              >
                {{
                  row.item.kind === "action"
                    ? $t("chat.cmdKindAction")
                    : row.item.kind === "slash"
                      ? $t("chat.cmdKindSlash")
                      : $t("chat.cmdKindSession")
                }}
              </span>
            </button>
          </template>
        </div>
        <div v-else class="palette__empty">{{ $t("chat.cmdEmpty") }}</div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.palette-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  justify-content: center;
  padding: 0 12px;
  padding-top: min(20vh, 160px);
  background: var(--overlay-backdrop);
}
.palette {
  width: min(560px, 100%);
  max-height: 60vh;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-panel);
  box-shadow: var(--shadow-md);
  overflow: hidden;
  align-self: flex-start;
}
.palette__input-wrap {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.palette__search-icon {
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.palette__input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text);
  font-size: 14px;
  font-family: var(--font);
}
.palette__kbd {
  flex-shrink: 0;
  font-size: 10px;
  color: var(--text-tertiary);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 1px 6px;
  background: var(--bg);
}
.palette__list {
  overflow-y: auto;
  padding: 6px;
  flex-shrink: 1;
}
.palette__group {
  padding: 6px 10px 4px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
}
.palette__item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  font-size: 13px;
  font-family: var(--font);
  text-align: left;
  cursor: pointer;
}
.palette__item.is-active {
  background: var(--bg-hover);
}
.palette__item-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.palette__item-hint {
  color: var(--text-tertiary);
  font-size: 11px;
  flex-shrink: 0;
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.palette__item-kind {
  flex-shrink: 0;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  font-weight: 500;
}
.kind-action {
  color: var(--chart-1);
  background: color-mix(in srgb, var(--chart-1) 14%, transparent);
}
.kind-slash {
  color: var(--chart-2);
  background: color-mix(in srgb, var(--chart-2) 14%, transparent);
}
.kind-session {
  color: var(--chart-4);
  background: color-mix(in srgb, var(--chart-4) 14%, transparent);
}
.palette__empty {
  padding: 24px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12px;
}

/* 移动端：16px 输入框避免 iOS 聚焦缩放 */
@media (max-width: 640px) {
  .palette-backdrop {
    padding-top: min(12vh, 96px);
  }
  .palette__input {
    font-size: 16px;
  }
}
</style>
