<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue"
import { ChevronDown } from "lucide-vue-next"
import type { ModelInfo, ModelRef } from "../../types"
import { registerModelCapabilities } from "../../utils/modelCapability"

const props = defineProps<{
  modelRef?: ModelRef | null
  sessionStatus?: "running" | "idle" | null
}>()

const emit = defineEmits<{
  (e: "select-model", model: ModelRef): void
}>()

const isOpen = ref(false)
const searchText = ref("")
const models = ref<ModelInfo[]>([])
const loading = ref(false)
const unavailable = ref(false)
const dropdownRef = ref<HTMLDivElement | null>(null)

const displayName = computed(() => {
  if (!props.modelRef?.id) return "model"
  return props.modelRef.id.replace(/^claude-/, "").replace(/-\d{8}$/, "")
})

// 同一模型 id 可能来自多个 provider——key 必须带 provider 保证唯一
function modelKey(m: ModelInfo): string {
  return m.provider ? `${m.provider}/${m.id}` : m.id
}

// 高亮须同时匹配 id + provider，否则不同 provider 的同 id 模型会全部高亮
function isActive(m: ModelInfo): boolean {
  const sel = props.modelRef
  if (!sel || m.id !== sel.id) return false
  if (sel.provider && m.provider && m.provider !== sel.provider) return false
  return true
}

const filteredModels = computed(() => {
  const q = searchText.value.toLowerCase().trim()
  if (!q) return models.value
  return models.value.filter(
    (m) =>
      m.id.toLowerCase().includes(q) ||
      (m.provider || "").toLowerCase().includes(q),
  )
})

function toggle() {
  isOpen.value = !isOpen.value
  if (isOpen.value) {
    searchText.value = ""
    if (models.value.length === 0 && !unavailable.value) {
      fetchModels()
    }
  }
}

function close() {
  isOpen.value = false
}

function select(model: ModelInfo) {
  emit("select-model", { id: model.id, provider: model.provider })
  close()
}

async function fetchModels() {
  loading.value = true
  try {
    const res = await fetch("/api/rpc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ type: "get_available_models" }),
    })
    const data = await res.json()
    if (data.success && Array.isArray(data.data?.models)) {
      models.value = data.data.models
      // 登记模态声明（含自定义 provider 的最新模型），供多模态预检即时生效
      registerModelCapabilities(data.data.models)
      return
    }
  } catch {
    // network error
  } finally {
    loading.value = false
  }
  unavailable.value = true
}

async function fetchCurrentModel() {
  try {
    const res = await fetch("/api/pi/settings")
    const data = await res.json()
    console.log("[model] fetchCurrentModel:", data)
    if (data.success && data.default_model) {
      emit("select-model", { id: data.default_model, provider: data.default_provider })
    }
  } catch {
    // non-critical
  }
}

// When session status changes, clear stale model list so it refetches on next open
watch(() => props.sessionStatus, (status, oldStatus) => {
  if (status === "running" && oldStatus !== "running") {
    unavailable.value = false
    models.value = []
    fetchCurrentModel()
  }
  if (status === "idle" && oldStatus === "running") {
    if (!props.modelRef) {
      fetchCurrentModel()
    }
  }
})

function handleClickOutside(e: MouseEvent) {
  if (dropdownRef.value && !dropdownRef.value.contains(e.target as Node)) {
    close()
  }
}

onMounted(() => {
  document.addEventListener("click", handleClickOutside)
  fetchCurrentModel()
})
onUnmounted(() => {
  document.removeEventListener("click", handleClickOutside)
})
</script>

<template>
  <div ref="dropdownRef" class="model-selector" :class="{ open: isOpen }">
    <button
      class="model-selector-btn"
      :class="{ disabled: unavailable }"
      :title="unavailable ? $t('chat.modelUnavailable') : $t('chat.selectModel')"
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
        :placeholder="$t('chat.searchModels')"
        autocomplete="off"
      />

      <div class="model-list">
        <div v-if="loading" class="model-empty">{{ $t("common.loading") }}</div>
        <div v-else-if="unavailable" class="model-empty">
          {{ $t("chat.modelUnavailable") }}
        </div>
        <div v-else-if="filteredModels.length === 0" class="model-empty">
          {{ $t("chat.noModelsHint") }}
        </div>
        <button
          v-for="model in filteredModels"
          :key="modelKey(model)"
          class="model-item"
          :class="{ active: isActive(model) }"
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
  border: 1px solid var(--border);
  border-radius: var(--radius-pill);
  background: var(--bg-muted);
  color: var(--text);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
}

.model-selector-btn:hover:not(.disabled) {
  background: var(--bg-hover);
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
  right: 0;
  min-width: 220px;
  max-height: 280px;
  display: flex;
  flex-direction: column;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-modal);
  z-index: 50;
  overflow: hidden;
}

.model-search {
  padding: 8px 10px;
  border: none;
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel);
  color: var(--text);
  font-size: 12px;
  outline: none;
}

.model-search::placeholder {
  color: var(--text-tertiary);
}

.model-list {
  flex: 1;
  overflow-y: auto;
}

.model-empty {
  padding: 14px;
  color: var(--text-tertiary);
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
  color: var(--text);
  font-size: 12px;
  cursor: pointer;
  text-align: left;
}

.model-item:hover {
  background: var(--bg-hover);
}

.model-item.active {
  background: var(--accent-soft);
}

.model-item-name {
  display: flex;
  align-items: center;
  gap: 6px;
}

.model-item-provider {
  font-size: 10px;
  color: var(--text-tertiary);
  background: var(--bg-muted);
  padding: 0 5px;
  border-radius: 3px;
}

.model-item-ctx {
  font-size: 10px;
  color: var(--text-tertiary);
}
</style>
