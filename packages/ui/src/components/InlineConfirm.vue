<script setup lang="ts">
withDefaults(
  defineProps<{
    /** Confirmation prompt, e.g. "Delete?" */
    prompt?: string;
    /** Disable both buttons while an action is in flight. */
    busy?: boolean;
  }>(),
  { prompt: "Delete?", busy: false },
);

const emit = defineEmits<{
  (e: "confirm"): void;
  (e: "cancel"): void;
}>();
</script>

<template>
  <span class="inline-confirm" @click.stop>
    <span class="inline-confirm__text">{{ prompt }}</span>
    <button
      type="button"
      class="btn btn-sm btn-danger"
      :disabled="busy"
      @click="emit('confirm')"
    >
      Yes
    </button>
    <button
      type="button"
      class="btn btn-sm btn-ghost"
      :disabled="busy"
      @click="emit('cancel')"
    >
      No
    </button>
  </span>
</template>

<style scoped>
.inline-confirm {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 4px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.inline-confirm__text {
  font-size: 11px;
  color: var(--danger);
  white-space: nowrap;
}
</style>
