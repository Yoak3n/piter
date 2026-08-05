<script setup lang="ts">
defineProps<{
  title?: string;
  desc?: string;
}>();
</script>

<template>
  <section class="panel-card">
    <header v-if="title || desc || $slots.head" class="panel-card__head">
      <div class="panel-card__titles">
        <h3 v-if="title" class="panel-card__title">{{ title }}</h3>
        <p v-if="desc" class="panel-card__desc">{{ desc }}</p>
        <slot name="head" />
      </div>
      <slot name="head-actions" />
    </header>
    <div class="panel-card__body">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.panel-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
  padding: var(--space-md);
}

.panel-card__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-md);
  margin-bottom: var(--space-sm);
}

.panel-card__titles {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.panel-card__title {
  margin: 0;
  font-size: var(--font-size-control);
  font-weight: 600;
  color: var(--text);
}

.panel-card__desc {
  margin: 0;
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
  line-height: var(--line-height-caption);
}
</style>
