<script setup lang="ts">
import type { Component } from "vue";

withDefaults(
  defineProps<{
    title: string;
    value: string;
    /** Icon component (lucide). */
    icon?: Component;
    /** Tone key resolved by the consumer to a color via `--card-tone`. */
    tone?: string;
  }>(),
  { icon: undefined, tone: undefined },
);
</script>

<template>
  <article
    class="stat-card"
    :style="tone ? { '--card-tone': tone } : undefined"
  >
    <div class="stat-card__title">
      <span v-if="icon" class="stat-card__icon">
        <component :is="icon" :size="13" />
      </span>
      <span>{{ title }}</span>
    </div>
    <div class="stat-card__value">{{ value }}</div>
  </article>
</template>

<style scoped>
.stat-card {
  padding: var(--space-md);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
}

.stat-card__title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  line-height: 1.2;
}

.stat-card__icon {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  color: var(--card-tone);
  opacity: 0.8;
}

.stat-card__value {
  margin-top: var(--space-sm);
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.02em;
  line-height: 1;
  color: var(--text);
  font-variant-numeric: tabular-nums;
}
</style>
