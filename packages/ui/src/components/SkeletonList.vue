<script setup lang="ts">
withDefaults(defineProps<{ rows?: number }>(), { rows: 6 });
</script>

<template>
  <div class="skeleton-list">
    <div v-for="i in rows" :key="i" class="skeleton-item">
      <div class="skeleton-line skeleton-line--title" />
      <div class="skeleton-line skeleton-line--meta" />
    </div>
  </div>
</template>

<style scoped>
.skeleton-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 8px;
}

.skeleton-item {
  padding: 10px;
  border-radius: var(--radius-md);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.skeleton-line {
  height: 10px;
  border-radius: 4px;
  background: var(--bg-muted);
  animation: skeleton-shimmer 1.5s ease-in-out infinite;
}

.skeleton-line--title {
  width: 70%;
}

.skeleton-line--meta {
  width: 40%;
}

@keyframes skeleton-shimmer {
  0%,
  100% {
    opacity: 0.4;
  }
  50% {
    opacity: 0.8;
  }
}
</style>
