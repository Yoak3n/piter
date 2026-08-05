<script setup lang="ts">
defineProps<{
  title?: string;
  subtitle?: string;
}>();
</script>

<template>
  <section class="chart-card">
    <div v-if="title || subtitle || $slots.head" class="chart-card__head">
      <h4 v-if="title" class="chart-card__title">{{ title }}</h4>
      <span v-if="subtitle" class="chart-card__subtitle">{{ subtitle }}</span>
      <slot name="head" />
    </div>
    <div class="chart-card__body">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.chart-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
  padding: var(--space-md);
}

.chart-card__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-md);
  margin-bottom: var(--space-sm);
}

.chart-card__title {
  margin: 0;
  font-size: var(--font-size-control);
  font-weight: 600;
  color: var(--text);
  letter-spacing: 0.01em;
}

.chart-card__subtitle {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}
</style>
