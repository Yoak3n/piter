<script setup lang="ts">
import { ref, watch } from "vue";
import type { PiSettings, PiAgentSettings } from "../../composables/useAdmin";

const props = defineProps<{
  settings: PiSettings;
  piAgentSettings: PiAgentSettings | null;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "update", settings: PiSettings): void;
}>();

const local = ref<PiSettings>({ ...props.settings });
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
</script>

<template>
  <div class="tab-content">
    <!-- Pi Agent Settings (read-only, from ~/.pi/agent/settings.json) -->
    <h3 class="tab-title">Pi Agent</h3>
    <p class="tab-desc">Read from ~/.pi/agent/settings.json — no Pi process needed.</p>

    <div class="pi-info-grid">
      <div class="pi-info-card">
        <span class="pi-info-label">Default Model</span>
        <code class="pi-info-value">{{ piAgentSettings?.default_model || "—" }}</code>
      </div>
      <div class="pi-info-card">
        <span class="pi-info-label">Provider</span>
        <code class="pi-info-value">{{ piAgentSettings?.default_provider || "—" }}</code>
      </div>
      <div class="pi-info-card">
        <span class="pi-info-label">Thinking Level</span>
        <code class="pi-info-value">{{ piAgentSettings?.default_thinking_level || "—" }}</code>
      </div>
    </div>

    <!-- Piter's own Pi-related settings (editable) -->
    <h3 class="tab-title">Piter Settings</h3>
    <p class="tab-desc">Application-level configuration for Pi process management.</p>

    <div class="settings-row">
      <div class="settings-label">
        <span class="settings-label-title">Override default model</span>
        <span class="settings-label-desc">If set, passed to Pi on session start</span>
      </div>
      <input
        class="input model-input"
        type="text"
        v-model="local.default_model"
        placeholder="Use Pi default"
        :disabled="disabled"
      />
    </div>

    <div class="settings-row">
      <div class="settings-label">
        <span class="settings-label-title">Request timeout</span>
        <span class="settings-label-desc">Seconds before a request is cancelled</span>
      </div>
      <input
        class="input number-input"
        type="number"
        v-model.number="local.request_timeout_secs"
        min="30"
        max="3600"
        :disabled="disabled"
      />
    </div>

    <h3 class="tab-title">Reliability</h3>

    <div class="settings-row">
      <div class="settings-label">
        <span class="settings-label-title">Auto-restart on crash</span>
        <span class="settings-label-desc">Restart Pi process if it exits unexpectedly</span>
      </div>
      <label class="toggle" :class="{ on: local.auto_restart_on_crash }">
        <input
          type="checkbox"
          v-model="local.auto_restart_on_crash"
          :disabled="disabled"
        />
        <span class="toggle-track"></span>
      </label>
    </div>

    <div class="tab-actions">
      <button class="btn btn-primary" :disabled="disabled" @click="handleSave">
        {{ saved ? "Saved" : "Save Pi Config" }}
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
  margin: 0 0 var(--space-xs) 0;
  padding-top: var(--space-lg);
}

.tab-title:first-child {
  padding-top: 0;
}

.tab-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0 0 var(--space-md) 0;
}

.pi-info-grid {
  display: flex;
  flex-direction: column;
  gap: 1px;
  background: var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  border: 1px solid var(--border);
}

.pi-info-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-sm) var(--space-md);
  background: var(--bg-panel);
}

.pi-info-label {
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
}

.pi-info-value {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text);
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
  flex: 1;
}

.settings-label-title {
  font-size: var(--font-size-body);
  color: var(--text);
}

.settings-label-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.model-input {
  width: 260px;
  flex-shrink: 0;
}

.number-input {
  width: 100px;
  flex-shrink: 0;
}

.tab-actions {
  margin-top: var(--space-xl);
  padding-top: var(--space-lg);
  border-top: 1px solid var(--border);
}
</style>
