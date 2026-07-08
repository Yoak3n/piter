<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { ChevronDown } from "lucide-vue-next";

export interface ModelInfo {
  id: string;
  provider?: string;
  contextWindow?: number;
}

const props = defineProps<{
  modelId?: string;
  sessionStatus?: "running" | "idle" | null;
}>();

const emit = defineEmits<{
  (e: "select-model", modelId: string): void;
}>();

const isOpen = ref(false);
const searchText = ref("");
const models = ref<ModelInfo[]>([]);
const loading = ref(false);
const unavailable = ref(false);
const triedOnce = ref(false);
const dropdownRef = ref<HTMLDivElement | null>(null);

const displayName = computed(() => {
  if (!props.modelId) return "model";
  return props.modelId.replace(/^claude-/, "").replace(/-\d{8}$/, "");
});

const filteredModels = computed(() => {
  const q = searchText.value.toLowerCase().trim();
  if (!q) return models.value;
  return models.value.filter(
    (m) =>
      m.id.toLowerCase().includes(q) ||
      (m.provider || "").toLowerCase().includes(q),
  );
});

function toggle() {
  isOpen.value = !isOpen.value;
  if (isOpen.value) {
    searchText.value = "";
    if (models.value.length === 0 && !unavailable.value) {
      fetchModels();
    }
  }
}

function close() {
  isOpen.value = false;
}

function select(model: ModelInfo) {
  emit("select-model", model.id);
  close();
}

async function fetchModels() {
  loading.value = true;
  try {
    const res = await fetch("/api/rpc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ type: "get_available_models" }),
    });
    const data = await res.json();
    if (data.success && Array.isArray(data.data?.models)) {
      models.value = data.data.models;
      return;
    }
  } catch {
    // network error
  } finally {
    loading.value = false;
  }
  unavailable.value = true;
}

async function fetchCurrentModel() {
  try {
    const res = await fetch("/api/rpc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ type: "get_state" }),
    });
    const data = await res.json();
    if (data.success && data.data?.model?.id) {
      emit("select-model", data.data.model.id);
      unavailable.value = false;
    }
  } catch {
    // ignore
  }
}

// When session becomes active (pi starts responding), retry fetching current model
// (in case initial mount failed because no pi was running yet)
watch(() => props.sessionStatus, (status, oldStatus) => {
  if (status === "running" && oldStatus !== "running") {
    unavailable.value = false;
    triedOnce.value = false;
    models.value = [];
    fetchCurrentModel();
  }
});

function handleClickOutside(e: MouseEvent) {
  if (dropdownRef.value && !dropdownRef.value.contains(e.target as Node)) {
    close();
  }
}

onMounted(() => {
  document.addEventListener("click", handleClickOutside);
  fetchCurrentModel();
});
onUnmounted(() => document.removeEventListener("click", handleClickOutside));
</script>

<template>
  <div ref="dropdownRef" class="model-selector" :class="{ open: isOpen }">
    <button
      class="model-selector-btn"
      :class="{ disabled: unavailable }"
      :title="unavailable ? 'Model switching not available' : 'Select model'"
      @click.stop="unavailable ? null : toggle()"
    >
      <span class="model-selector-label">{{ displayName }}</span>
      <ChevronDown v-if="!unavailable" :size="10" class="model-chevron" />
    </button>

    <div v-if="isOpen" class="model-dropdown" @click.stop>
      <input
        v-model="searchText"
        type="text"
        class="model-search"
        placeholder="Search models..."
        autocomplete="off"
      />

      <div class="model-list">
        <div v-if="loading" class="model-empty">Loading...</div>
        <div v-else-if="unavailable" class="model-empty">
          Model switching unavailable
        </div>
        <div v-else-if="filteredModels.length === 0" class="model-empty">
          No models available
        </div>
        <button
          v-for="model in filteredModels"
          :key="model.id"
          class="model-item"
          :class="{ active: model.id === modelId }"
          @click="select(model)"
        >
          <span class="model-item-name">
            {{ model.id.replace(/^claude-/, "").replace(/-\d{8}$/, "") }}
            <span
              v-if="model.provider && model.provider !== 'anthropic'"
              class="model-item-provider"
            >
              {{ model.provider }}
            </span>
          </span>
          <span v-if="model.contextWindow" class="model-item-ctx">
            {{ (model.contextWindow / 1000).toFixed(0) }}k
          </span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.model-selector {
  position: relative;
  display: inline-flex;
}

.model-selector-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: var(--color-bg-muted);
  color: var(--color-text-primary);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
}

.model-selector-btn:hover:not(.disabled) {
  background: var(--color-bg-hover);
}

.model-selector-btn.disabled {
  opacity: 0.5;
  cursor: default;
}

.model-chevron {
  transition: transform 0.15s ease;
}

.model-selector.open .model-chevron {
  transform: rotate(180deg);
}

.model-dropdown {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  min-width: 220px;
  max-height: 280px;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-panel);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-modal);
  z-index: 50;
  overflow: hidden;
}

.model-search {
  padding: 8px 10px;
  border: none;
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--color-bg-panel);
  color: var(--color-text-primary);
  font-size: 12px;
  outline: none;
}

.model-search::placeholder {
  color: var(--color-text-tertiary);
}

.model-list {
  flex: 1;
  overflow-y: auto;
}

.model-empty {
  padding: 14px;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
}

.model-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 8px 10px;
  border: none;
  background: none;
  color: var(--color-text-primary);
  font-size: 12px;
  cursor: pointer;
  text-align: left;
}

.model-item:hover {
  background: var(--color-bg-hover);
}

.model-item.active {
  background: var(--color-accent-soft);
}

.model-item-name {
  display: flex;
  align-items: center;
  gap: 6px;
}

.model-item-provider {
  font-size: 10px;
  color: var(--color-text-tertiary);
  background: var(--color-bg-muted);
  padding: 0 5px;
  border-radius: 3px;
}

.model-item-ctx {
  font-size: 10px;
  color: var(--color-text-tertiary);
}
</style>
