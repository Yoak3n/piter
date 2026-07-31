<script setup lang="ts">
import { computed } from "vue";
import { Package, Trash2, Puzzle } from "lucide-vue-next";
import type { PiAgentSettings } from "../../composables/useAdmin";

const props = defineProps<{
  piAgentSettings: PiAgentSettings | null;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "save-agent", settings: PiAgentSettings): void;
}>();

const packages = computed(() => props.piAgentSettings?.packages ?? []);

function removePackage(pkg: string) {
  if (!props.piAgentSettings) return;
  emit("save-agent", {
    ...props.piAgentSettings,
    packages: props.piAgentSettings.packages.filter((p) => p !== pkg),
  });
}
</script>

<template>
  <div class="tab-content">
    <h3 class="tab-title">Installed Extensions</h3>
    <p class="tab-desc">Pi agent extensions (npm packages and skills).</p>

    <div v-if="packages.length === 0" class="empty-state">
      <Puzzle :size="32" class="empty-icon" />
      <p>No extensions installed</p>
      <span class="empty-hint">Extensions are npm packages that extend Pi's capabilities.</span>
    </div>

    <div v-else class="ext-list">
      <div v-for="pkg in packages" :key="pkg" class="ext-item">
        <div class="ext-info">
          <Package :size="14" class="ext-icon" />
          <span class="ext-name">{{ pkg.replace(/^npm:/, "") }}</span>
        </div>
        <button
          class="btn btn-ghost btn-icon btn-sm"
          title="Uninstall"
          :disabled="disabled"
          @click="removePackage(pkg)"
        >
          <Trash2 :size="12" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  max-width: 540px;
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0 0 var(--space-xs) 0;
}

.tab-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0 0 var(--space-lg) 0;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-sm);
  color: var(--text-tertiary);
  padding: var(--space-xxl) 0;
  text-align: center;
}
.empty-icon {
  opacity: 0.4;
  margin-bottom: var(--space-xs);
}
.empty-state p {
  margin: 0;
  font-size: var(--font-size-body);
  color: var(--text-secondary);
}
.empty-hint {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.ext-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  background: var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  border: 1px solid var(--border);
}

.ext-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-sm) var(--space-md);
  background: var(--bg-panel);
  transition: background var(--duration-fast) var(--ease);
}
.ext-item:hover {
  background: var(--bg-muted);
}

.ext-info {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.ext-icon {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.ext-name {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text);
}
</style>
