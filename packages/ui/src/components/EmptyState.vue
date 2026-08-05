<script setup lang="ts">
withDefaults(
  defineProps<{
    title?: string;
    hint?: string;
    /** Dense inline row, used inside admin lists. */
    compact?: boolean;
    /** Fill the parent height and center, e.g. chat timeline. */
    fill?: boolean;
    /** Error tone for failure states. */
    error?: boolean;
    /** Full-color illustration slot (no dimming/tertiary tint). */
    illustration?: boolean;
  }>(),
  { compact: false, fill: false, error: false, illustration: false },
);
</script>

<template>
  <div
    class="empty-state"
    :class="{
      'empty-state--compact': compact,
      'empty-state--fill': fill,
      'empty-state--error': error,
    }"
  >
    <div v-if="$slots.icon" class="empty-state__icon" :class="{ 'empty-state__icon--illustration': illustration }">
      <slot name="icon" />
    </div>
    <p v-if="title" class="empty-state__title">{{ title }}</p>
    <p v-if="hint" class="empty-state__hint">{{ hint }}</p>
    <div v-if="$slots.actions" class="empty-state__actions">
      <slot name="actions" />
    </div>
    <slot />
  </div>
</template>

<style scoped>
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px 16px;
  gap: 8px;
  text-align: center;
}

.empty-state--fill {
  height: 100%;
}

.empty-state__icon {
  color: var(--text-tertiary);
  opacity: 0.6;
  line-height: 0;
}

.empty-state__icon--illustration {
  color: inherit;
  opacity: 1;
}

.empty-state__title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  margin: 0;
}

.empty-state__hint {
  font-size: 11px;
  color: var(--text-tertiary);
  margin: 0;
}

.empty-state__actions {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}

.empty-state--error .empty-state__title {
  color: var(--danger);
}

/* Dense inline row variant (admin list empties) */
.empty-state--compact {
  flex-direction: row;
  justify-content: flex-start;
  padding: 8px 0;
  gap: 8px;
  text-align: left;
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
}

.empty-state--compact .empty-state__icon {
  opacity: 0.4;
}

.empty-state--compact .empty-state__title {
  font-size: var(--font-size-caption);
  font-weight: 400;
  color: var(--text-tertiary);
}
</style>
