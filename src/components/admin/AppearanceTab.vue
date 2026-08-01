<script setup lang="ts">
import { ref, watch, onBeforeUnmount } from "vue";
import type { AppSettings } from "../../composables/useAdmin";
import { applyTheme } from "../../utils/theme";

const props = defineProps<{
  settings: AppSettings;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "update", settings: AppSettings): void;
  (e: "preview", theme: string): void;
}>();

const local = ref<AppSettings>({ ...props.settings });
const saved = ref(false);

watch(() => props.settings, (s) => { local.value = { ...s }; }, { immediate: true });

// Preview the selected theme immediately; it persists on save. The parent
// tracks the preview so system theme changes don't override it.
watch(() => local.value.theme, (t) => {
  applyTheme(t);
  emit("preview", t);
});

// Leaving the tab restores the saved theme (an unsaved preview is transient).
onBeforeUnmount(() => {
  applyTheme(props.settings.theme);
  emit("preview", props.settings.theme);
});

function handleSave() {
  saved.value = false;
  emit("update", { ...local.value });
  saved.value = true;
  setTimeout(() => (saved.value = false), 2000);
}

const themes = [
  { key: "light", label: "Light", preview: "bg-white text-gray-900" },
  { key: "dark", label: "Dark", preview: "bg-gray-900 text-gray-100" },
  { key: "system", label: "System", preview: "bg-gradient" },
];
</script>

<template>
  <div class="tab-content">
    <div class="section-card">
      <div class="section-header">
        <h3 class="tab-title">Theme</h3>
        <p class="tab-desc">Choose your preferred color scheme</p>
      </div>
      <div class="theme-grid">
        <button
          v-for="t in themes"
          :key="t.key"
          class="theme-card"
          :class="{ active: local.theme === t.key }"
          @click="local.theme = t.key"
        >
          <span class="theme-card-preview" :class="`preview-${t.key}`"></span>
          <span class="theme-card-label">{{ t.label }}</span>
        </button>
      </div>
    </div>

    <div class="section-card">
      <div class="section-header">
        <h3 class="tab-title">Behavior</h3>
      </div>

      <div class="settings-row">
        <div class="settings-label">
          <span class="settings-label-title">Auto-start on login</span>
          <span class="settings-label-desc">Launch Piter when you log in</span>
        </div>
        <label class="toggle" :class="{ on: local.auto_start }">
          <input type="checkbox" v-model="local.auto_start" :disabled="disabled" />
          <span class="toggle-track"></span>
        </label>
      </div>

      <div class="settings-row">
        <div class="settings-label">
          <span class="settings-label-title">Start minimized</span>
          <span class="settings-label-desc">Hide window on launch</span>
        </div>
        <label class="toggle" :class="{ on: local.start_minimized }">
          <input type="checkbox" v-model="local.start_minimized" :disabled="disabled" />
          <span class="toggle-track"></span>
        </label>
      </div>

      <div class="section-footer">
        <button class="btn btn-primary" :disabled="disabled" @click="handleSave">
          {{ saved ? "Saved" : "Save Settings" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.section-card {
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-lg);
}

.section-header {
  margin-bottom: var(--space-md);
}

.section-footer {
  margin-top: var(--space-lg);
  padding-top: var(--space-md);
  border-top: 1px solid var(--border);
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
  margin: 0;
}

.theme-grid {
  display: flex;
  gap: var(--space-sm);
}

.theme-card {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-sm);
  padding: var(--space-md);
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

.theme-card-preview {
  width: 100%;
  height: 32px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
}

.preview-light {
  background: #f8f7f4;
}

.preview-dark {
  background: #1a1a1e;
}

.preview-system {
  background: linear-gradient(90deg, #f8f7f4 50%, #1a1a1e 50%);
}

.theme-card-label {
  font-size: var(--font-size-caption);
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
</style>
