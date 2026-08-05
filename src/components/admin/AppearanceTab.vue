<script setup lang="ts">
import { ref, watch } from "vue";
import { setLocale } from "@piter/ui";
import type { AppSettings } from "../../composables/useAdmin";
import { applyTheme } from "../../utils/theme";
import { i18n } from "../../i18n";

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

// ─── Theme ───────────────────────────────────────────────────────────────
// Applies immediately (preview) AND persists on change — no Save click needed.
// The parent tracks the preview so system theme changes don't override it.
watch(() => local.value.theme, (t) => {
  applyTheme(t);
  emit("preview", t);
  if (!props.disabled) emit("update", { ...local.value });
});

// ─── Language ───────────────────────────────────────────────────────────
// Applies immediately and persists on change, like the theme.
function pickLanguage(lang: string) {
  local.value.language = lang;
  setLocale(i18n, lang);
  if (!props.disabled) emit("update", { ...local.value });
}

// The behavior toggles below (auto-start, start-minimized) are still
// committed explicitly via the Save button.
function handleSave() {
  saved.value = false;
  emit("update", { ...local.value });
  saved.value = true;
  setTimeout(() => (saved.value = false), 2000);
}

const themes = [
  { key: "light", labelKey: "admin.themeLight", preview: "preview-light" },
  { key: "dark", labelKey: "admin.themeDark", preview: "preview-dark" },
  { key: "system", labelKey: "admin.themeSystem", preview: "preview-system" },
];

const languages = [
  { key: "system", labelKey: "admin.languageSystem" },
  { key: "zh", label: "中文" },
  { key: "en", label: "English" },
];
</script>

<template>
  <div class="tab-content">
    <div class="section-card">
      <div class="section-header">
        <h3 class="tab-title">{{ $t("admin.theme") }}</h3>
        <p class="tab-desc">{{ $t("admin.themeDesc") }}</p>
      </div>
      <div class="theme-grid">
        <button
          v-for="t in themes"
          :key="t.key"
          class="theme-card"
          :class="{ active: local.theme === t.key }"
          @click="local.theme = t.key"
        >
          <span class="theme-card-preview" :class="t.preview"></span>
          <span class="theme-card-label">{{ $t(t.labelKey) }}</span>
        </button>
      </div>
    </div>

    <div class="section-card">
      <div class="section-header">
        <h3 class="tab-title">{{ $t("admin.language") }}</h3>
        <p class="tab-desc">{{ $t("admin.languageDesc") }}</p>
      </div>
      <div class="theme-grid">
        <button
          v-for="l in languages"
          :key="l.key"
          class="theme-card"
          :class="{ active: local.language === l.key }"
          @click="pickLanguage(l.key)"
        >
          <span class="language-preview">{{ l.key === "system" ? "A" : l.key.toUpperCase() }}</span>
          <span class="theme-card-label">{{ l.label ?? $t(l.labelKey) }}</span>
        </button>
      </div>
    </div>

    <div class="section-card">
      <div class="section-header">
        <h3 class="tab-title">{{ $t("admin.behavior") }}</h3>
      </div>

      <div class="settings-row">
        <div class="settings-label">
          <span class="settings-label-title">{{ $t("admin.autoStart") }}</span>
          <span class="settings-label-desc">{{ $t("admin.autoStartDesc") }}</span>
        </div>
        <label class="toggle" :class="{ on: local.auto_start }">
          <input type="checkbox" v-model="local.auto_start" :disabled="disabled" />
          <span class="toggle-track"></span>
        </label>
      </div>

      <div class="settings-row">
        <div class="settings-label">
          <span class="settings-label-title">{{ $t("admin.startMinimized") }}</span>
          <span class="settings-label-desc">{{ $t("admin.startMinimizedDesc") }}</span>
        </div>
        <label class="toggle" :class="{ on: local.start_minimized }">
          <input type="checkbox" v-model="local.start_minimized" :disabled="disabled" />
          <span class="toggle-track"></span>
        </label>
      </div>

      <div class="section-footer">
        <button class="btn btn-primary" :disabled="disabled" @click="handleSave">
          {{ saved ? $t("common.saved") : $t("admin.saveSettings") }}
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

.language-preview {
  width: 100%;
  height: 32px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--font-size-control);
  font-weight: 600;
  color: var(--text-secondary);
}

.preview-light {
  background: #fcfcfc;
}

.preview-dark {
  background: #1a1a1e;
}

.preview-system {
  background: linear-gradient(90deg, #fcfcfc 50%, #1a1a1e 50%);
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
