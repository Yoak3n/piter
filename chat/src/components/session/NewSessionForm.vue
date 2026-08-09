<script setup lang="ts">
// ─── 新会话环境配置（工作目录 + 项目 + 新项目名）──
// 纯展示组件：目录/项目数据与选中状态来自父级，本组件只做模板与事件转发。

defineProps<{
  uniqueDirs: Array<{ path: string; dirName: string }>;
  selectedCwd: string;
  matchingProjects: Array<{ id: string; name: string; cwd: string }>;
  selectedProjectId: string;
  createNewProject: boolean;
  newProjectName: string;
  autoProjectName: string;
  error: string;
  mobileMode: boolean;
  isTauri: boolean;
}>();

defineEmits<{
  (e: "selectDir", path: string): void;
  (e: "selectProject", id: string): void;
  (e: "browse"): void;
  (e: "toggleNewProject", value: boolean): void;
  (e: "update:newProjectName", value: string): void;
}>();
</script>

<template>
  <div class="config-area">
    <!-- Working directory -->
    <div class="config-row">
      <span class="config-label">{{ $t("chat.directory") }}</span>
      <div class="config-chips">
        <button
          v-for="d in uniqueDirs"
          :key="d.path"
          class="chip"
          :class="{ active: selectedCwd === d.path }"
          @click="$emit('selectDir', d.path)"
        >
          {{ d.dirName }}
        </button>
        <button
          v-if="!mobileMode && isTauri"
          class="chip chip-dashed"
          @click="$emit('browse')"
        >
          + {{ $t("common.browse") }}
        </button>
      </div>
    </div>
    <div v-if="selectedCwd && !uniqueDirs.some(d => d.path === selectedCwd)" class="config-selected-path">
      {{ selectedCwd }}
    </div>

    <!-- Project -->
    <div class="config-row">
      <span class="config-label">{{ $t("chat.project") }}</span>
      <div class="config-chips">
        <button
          v-for="p in matchingProjects"
          :key="p.id"
          class="chip"
          :class="{ active: selectedProjectId === p.id }"
          @click="$emit('selectProject', p.id)"
        >
          {{ p.name }}
        </button>
        <button
          class="chip"
          :class="{ active: !selectedProjectId && !createNewProject }"
          :title="$t('chat.autoProjectTitle')"
          @click="$emit('selectProject', '')"
        >
          {{ $t("chat.autoProject") }}
        </button>
        <button
          v-if="!mobileMode"
          class="chip chip-dashed"
          :class="{ active: createNewProject }"
          @click="$emit('toggleNewProject', !createNewProject)"
        >
          {{ $t("chat.newProject") }}
        </button>
      </div>
    </div>
    <div
      v-if="!selectedProjectId && !createNewProject && autoProjectName"
      class="project-hint"
    >
      {{ $t("chat.autoHint", { name: autoProjectName }) }}
    </div>

    <!-- New project name (inline, only when creating) -->
    <div v-if="createNewProject" class="config-row">
      <span class="config-label">{{ $t("chat.name") }}</span>
      <input
        :value="newProjectName"
        type="text"
        class="config-input"
        :placeholder="$t('chat.projectPlaceholder')"
        @input="$emit('update:newProjectName', ($event.target as HTMLInputElement).value)"
      />
    </div>

    <p v-if="error" class="error">{{ error }}</p>
  </div>
</template>

<style scoped>
.config-area {
  display: flex;
  flex-direction: column;
  gap: 1.1rem;
  padding: 1.5rem 1.75rem;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg, 12px);
  background: var(--bg-panel);
  box-shadow: var(--shadow-md);
}

.config-row {
  display: flex;
  align-items: center;
  gap: 1.1rem;
}

.config-label {
  font-size: 0.85rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  min-width: 72px;
  flex-shrink: 0;
}

.config-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.chip {
  padding: 0.45rem 0.9rem;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: transparent;
  color: var(--text);
  font-size: 0.875rem;
  cursor: pointer;
  transition: all var(--duration-fast);
  white-space: nowrap;
}

.chip:hover {
  border-color: var(--accent);
}

.chip.active {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--accent-strong);
  font-weight: 500;
}

.chip-dashed {
  border-style: dashed;
  color: var(--text-secondary);
}

.config-input {
  flex: 1;
  padding: 0.45rem 0.75rem;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  font-size: 0.95rem;
  outline: none;
}

.config-input:focus {
  border-color: var(--accent);
}

.config-selected-path {
  margin-top: -0.25rem;
  margin-left: 84px;
  font-size: 0.8rem;
  color: var(--text-tertiary);
  font-family: monospace;
  word-break: break-all;
  line-height: 1.3;
}

.project-hint {
  margin-top: -0.4rem;
  margin-left: 84px;
  font-size: 0.8rem;
  color: var(--text-tertiary);
}

.error {
  color: var(--danger);
  font-size: 0.85rem;
  margin: 0;
}
</style>
