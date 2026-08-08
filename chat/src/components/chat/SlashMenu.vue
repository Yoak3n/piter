<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { SlashCommand } from "../../types";

/** 浮层定位（相对 Composer 输入区）：left 必填（精确到 caret 字符列），top/bottom 二选一 */
export interface SlashMenuPosition {
  left: number;
  top?: number;
  bottom?: number;
}

const props = defineProps<{
  commands: SlashCommand[];
  /** 当前高亮项索引（键盘 ↑↓ 导航用） */
  highlight: number;
  /** 浮层定位（caret 字符列） */
  position?: SlashMenuPosition;
}>();

const emit = defineEmits<{
  (e: "select", cmd: SlashCommand): void;
}>();

const { t } = useI18n();

const sourceBadges = computed<Record<SlashCommand["source"], { label: string; cls: string }>>(() => ({
  extension: { label: t("chat.slashSourceExtension"), cls: "source-extension" },
  prompt: { label: t("chat.slashSourcePrompt"), cls: "source-prompt" },
  skill: { label: t("chat.slashSourceSkill"), cls: "source-skill" },
}));

const menuStyle = computed(() => {
  const p = props.position;
  if (!p) return {};
  return {
    left: `${p.left}px`,
    top: p.top !== undefined ? `${p.top}px` : undefined,
    bottom: p.bottom !== undefined ? `${p.bottom}px` : undefined,
  };
});

/** tooltip：多行展示（/name + 详细说明 description + sourceInfo.path） */
function tooltip(cmd: SlashCommand): string {
  const lines: string[] = [`/${cmd.name}`];
  if (cmd.description) lines.push(cmd.description);
  const path = cmd.sourceInfo?.path;
  if (typeof path === "string" && path) lines.push(path);
  return lines.join("\n");
}
</script>

<template>
  <div class="slash-menu" role="listbox" :style="menuStyle" :aria-label="$t('chat.slashCommands')">
    <div
      v-for="(cmd, i) in commands"
      :key="cmd.name"
      class="slash-menu__item"
      :class="{ 'is-active': i === highlight }"
      role="option"
      :aria-selected="i === highlight"
      :title="tooltip(cmd)"
      @mousedown.prevent
      @click="emit('select', cmd)"
    >
      <span class="slash-menu__name">/{{ cmd.name }}</span>
      <span v-if="cmd.description" class="slash-menu__desc">{{ cmd.description }}</span>
      <span class="slash-menu__badge" :class="sourceBadges[cmd.source].cls">
        {{ sourceBadges[cmd.source].label }}
      </span>
    </div>
  </div>
</template>

<style scoped>
.slash-menu {
  position: absolute;
  z-index: 30;
  width: min(420px, calc(100vw - 16px));
  min-width: 200px;
  max-height: 240px;
  overflow-y: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-panel);
  box-shadow: var(--shadow-md);
  padding: 4px;
}
.slash-menu__item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 12px;
  line-height: 1.4;
  transition: background var(--duration-fast) var(--ease);
}
.slash-menu__item.is-active,
.slash-menu__item:hover {
  background: var(--bg-hover);
}
.slash-menu__name {
  font-family: var(--font-mono);
  color: var(--text);
  flex-shrink: 0;
  font-size: 12px;
}
.slash-menu__desc {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-tertiary);
  font-size: 11px;
}
.slash-menu__badge {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 500;
  line-height: 1.5;
}
.source-extension {
  color: var(--chart-1);
  background: color-mix(in srgb, var(--chart-1) 14%, transparent);
}
.source-prompt {
  color: var(--chart-4);
  background: color-mix(in srgb, var(--chart-4) 14%, transparent);
}
.source-skill {
  color: var(--chart-2);
  background: color-mix(in srgb, var(--chart-2) 14%, transparent);
}
</style>
