<script setup lang="ts">
import { ref, watch } from "vue";
import type { PiAgentSettings } from "../../composables/useAdmin";

const props = defineProps<{
  piAgentSettings: PiAgentSettings | null;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "save-agent", settings: PiAgentSettings): void;
}>();

const agentSaved = ref(false);

// Editable copies of Pi agent settings
const agentModel = ref("");
const agentProvider = ref("");
const agentThinking = ref("");

watch(() => props.piAgentSettings, (s) => {
  if (s) {
    agentModel.value = s.defaultModel;
    agentProvider.value = s.defaultProvider;
    agentThinking.value = s.defaultThinkingLevel;
  }
}, { immediate: true });

function handleSaveAgent() {
  if (!props.piAgentSettings) return;
  agentSaved.value = false;
  emit("save-agent", {
    ...props.piAgentSettings,
    defaultModel: agentModel.value,
    defaultProvider: agentProvider.value,
    defaultThinkingLevel: agentThinking.value,
  });
  agentSaved.value = true;
  setTimeout(() => (agentSaved.value = false), 2000);
}
</script>

<template>
  <div class="tab-content">
    <div class="section-card">
      <div class="section-header">
        <h3 class="tab-title">Pi Agent Defaults</h3>
        <p class="tab-desc">Read from ~/.pi/agent/settings.json</p>
      </div>

      <div class="settings-row">
        <div class="settings-label">
          <span class="settings-label-title">Default Provider</span>
        </div>
        <input class="input model-input" type="text" v-model="agentProvider" :disabled="disabled" placeholder="e.g. openai" />
      </div>

      <div class="settings-row">
        <div class="settings-label">
          <span class="settings-label-title">Default Model</span>
        </div>
        <input class="input model-input" type="text" v-model="agentModel" :disabled="disabled" placeholder="e.g. gpt-4o" />
      </div>

      <div class="settings-row">
        <div class="settings-label">
          <span class="settings-label-title">Thinking Level</span>
        </div>
        <select class="input" v-model="agentThinking" :disabled="disabled">
          <option value="off">Off</option>
          <option value="low">Low</option>
          <option value="medium">Medium</option>
          <option value="high">High</option>
        </select>
      </div>

      <div class="section-footer">
        <button class="btn btn-primary" :disabled="disabled" @click="handleSaveAgent">
          {{ agentSaved ? "Saved" : "Save Agent Settings" }}
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
  width: 240px;
  flex-shrink: 0;
}
</style>
