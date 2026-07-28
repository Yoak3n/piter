<script setup lang="ts">
import { ref, watch } from "vue";
import type { AppSettings } from "../../composables/useAdmin";

const props = defineProps<{
  settings: AppSettings;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "update", settings: AppSettings): void;
}>();

const local = ref<AppSettings>({ ...props.settings });
const saved = ref(false);

watch(
  () => props.settings,
  (s) => {
    local.value = { ...s };
  }
);

function handleSave() {
  saved.value = false;
  emit("update", { ...local.value });
  saved.value = true;
  setTimeout(() => (saved.value = false), 2000);
}

const themes = [
  { key: "light", label: "Light" },
  { key: "dark", label: "Dark" },
  { key: "system", label: "System" },
];
</script>

<template>
  <div class="tab-content">
    <h3 class="tab-title">Appearance</h3>
    <div class="theme-grid">
      <button
        v-for="t in themes"
        :key="t.key"
        class="theme-card"
        :class="{ active: local.theme === t.key }"
        @click="local.theme = t.key"
      >
        {{ t.label }}
      </button>
    </div>

    <h3 class="tab-title">Behavior</h3>

    <div class="settings-row">
      <div class="settings-label">
        <span class="settings-label-title">Auto-start on login</span>
        <span class="settings-label-desc">Launch Piter when you log in</span>
      </div>
      <label class="toggle" :class="{ on: local.auto_start }">
        <input
          type="checkbox"
          v-model="local.auto_start"
          :disabled="disabled"
        />
        <span class="toggle-track"></span>
      </label>
    </div>

    <div class="settings-row">
      <div class="settings-label">
        <span class="settings-label-title">Start minimized</span>
        <span class="settings-label-desc">Hide window on launch</span>
      </div>
      <label class="toggle" :class="{ on: local.start_minimized }">
        <input
          type="checkbox"
          v-model="local.start_minimized"
          :disabled="disabled"
        />
        <span class="toggle-track"></span>
      </label>
    </div>

    <div class="tab-actions">
      <button class="btn btn-primary" :disabled="disabled" @click="handleSave">
        {{ saved ? "Saved" : "Save Settings" }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  max-width: 520px;
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0 0 var(--space-md) 0;
  padding-top: var(--space-lg);
}

.tab-title:first-child {
  padding-top: 0;
}

.theme-grid {
  display: flex;
  gap: var(--space-sm);
}

.theme-card {
  flex: 1;
  padding: var(--space-md) var(--space-lg);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
  color: var(--text-secondary);
  font-size: var(--font-size-control);
  font-family: var(--font);
  cursor: pointer;
  text-align: center;
  transition: border-color var(--duration-fast) var(--ease), background var(--duration-fast) var(--ease);
}

.theme-card:hover {
  border-color: var(--border-hover);
}

.theme-card.active {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--accent-strong);
  font-weight: 500;
}

.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-sm) 0;
}

.settings-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.settings-label-title {
  font-size: var(--font-size-body);
  color: var(--text);
}

.settings-label-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.tab-actions {
  margin-top: var(--space-xl);
  padding-top: var(--space-lg);
  border-top: 1px solid var(--border);
}
</style>
