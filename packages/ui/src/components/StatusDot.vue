<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    state?: "idle" | "busy" | "review" | "unloaded" | "running" | "error";
    /** Native tooltip title (e.g. the raw session state). */
    title?: string;
  }>(),
  { state: "unloaded", title: undefined },
);

const cls = computed(() => `status-dot--${props.state}`);
</script>

<template>
  <span class="status-dot" :class="cls" :title="title" />
</template>

<style scoped>
.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  background: var(--text-tertiary);
  opacity: 0.3;
}

.status-dot--idle {
  background: var(--state-idle);
  opacity: 1;
}

.status-dot--busy {
  background: var(--state-busy);
  opacity: 1;
  animation: status-pulse 1.2s ease-in-out infinite;
}

.status-dot--review {
  background: var(--state-review);
  opacity: 1;
}

.status-dot--unloaded {
  background: var(--text-tertiary);
  opacity: 0.3;
}

.status-dot--running {
  background: var(--accent);
  opacity: 1;
  animation: status-pulse 1.2s ease-in-out infinite;
}

.status-dot--error {
  background: var(--state-error);
  opacity: 1;
}

@keyframes status-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.3;
  }
}
</style>
