<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'

// Only show the 'Browse' button when running inside Tauri (native dialog)
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
const props = defineProps<{
  projects: Array<{ path: string; name: string }>
  mobileMode: boolean
  /** 预选的工作目录（点击侧栏项目级 "+" 时传入，快速为该目录建会话） */
  initialCwd?: string
  /** 预选的项目名（与 initialCwd 配套，用于选中 DB 中匹配的项目） */
  initialName?: string
}>()

const emit = defineEmits<{
  (e: 'create', payload: { cwd: string; name: string; message?: string }): void
}>()

const selectedCwd = ref('')
const selectedProjectId = ref('')
const createNewProject = ref(false)
const newProjectName = ref('')
const firstMessage = ref('')
const error = ref('')

const dbProjects = ref<Array<{ id: string; name: string; cwd: string }>>([])

async function fetchProjects() {
  try {
    const resp = await fetch('/api/projects')
    const data = await resp.json()
    if (data.success) dbProjects.value = data.projects
  } catch (err) {
    const errorMsg = err instanceof Error ? `Failed to fetch projects: ${err.message}` : 'Unknown error'
    error.value = errorMsg
    console.error(errorMsg)
  }
  preselectProject()
}

// 预选 initialCwd 对应的 DB 项目：优先 (cwd+name) 精确匹配，其次 cwd 匹配；
// DB 尚未收录时退化为"新建项目"并预填名称，保证创建时项目名正确。
function preselectProject() {
  if (!props.initialCwd) return
  const match =
    dbProjects.value.find(p => p.cwd === props.initialCwd && p.name === props.initialName) ??
    dbProjects.value.find(p => p.cwd === props.initialCwd)
  if (match) {
    selectedProjectId.value = match.id
    createNewProject.value = false
  } else if (props.initialName) {
    createNewProject.value = true
    newProjectName.value = props.initialName
  }
}

// 根据项目的path来去重？我不需要啊
const uniqueDirs = computed(() => {
  const seen = new Set<string>()
  return props.projects.filter(p => {
    if (seen.has(p.path)) return false
    seen.add(p.path)
    return true
  }).map(p => ({
    path: p.path,
    dirName: p.path.split(/[/\\]/).filter(Boolean).pop() || p.name,
  }))
})

const matchingProjects = computed(() =>
  dbProjects.value.filter(p => p.cwd === selectedCwd.value)
)

onMounted(() => {
  fetchProjects()
  if (props.initialCwd) {
    selectedCwd.value = props.initialCwd
  } else if (uniqueDirs.value.length > 0) {
    selectedCwd.value = uniqueDirs.value[0].path
  }
})

function selectDir(path: string) {
  selectedCwd.value = path
  selectedProjectId.value = ''
  createNewProject.value = false
  error.value = ''
}

function selectProject(id: string) {
  selectedProjectId.value = id
  createNewProject.value = false
}

function handleBrowse() {
  import('@tauri-apps/plugin-dialog').then(({ open }) => {
    open({ directory: true, multiple: false, title: 'Select working directory' }).then((sel) => {
      if (sel && typeof sel === 'string') {
        selectedCwd.value = sel
        selectedProjectId.value = ''
        error.value = ''
      }
    })
  }).catch(() => {})
}

function handleCreate() {
  if (!selectedCwd.value.trim()) {
    error.value = 'Select a working directory'
    return
  }
  error.value = ''

  // Determine the project name: selected existing project, new project name, or directory name as fallback
  let name = ''
  if (selectedProjectId.value) {
    const proj = dbProjects.value.find(p => p.id === selectedProjectId.value)
    name = proj?.name || ''
  } else if (createNewProject.value && newProjectName.value.trim()) {
    name = newProjectName.value.trim()
  }
  if (!name) {
    name = selectedCwd.value.split(/[/\\]/).filter(Boolean).pop() || 'New Session'
  }

  const payload: { cwd: string; name: string; message?: string } = {
    cwd: selectedCwd.value,
    name,
  }
  if (firstMessage.value.trim()) {
    payload.message = firstMessage.value.trim()
  }
  emit('create', payload)
}
</script>

<template>
  <div class="welcome-pane">
    <div class="welcome-content">
      <div class="welcome-header">
        <h1>Piter</h1>
        <p class="tagline">Start a conversation</p>
      </div>

      <!-- Environment configuration -->
      <div class="config-area">
        <!-- Working directory -->
        <div class="config-row">
          <span class="config-label">Directory</span>
          <div class="config-chips">
            <button
              v-for="d in uniqueDirs"
              :key="d.path"
              class="chip"
              :class="{ active: selectedCwd === d.path }"
              @click="selectDir(d.path)"
            >
              {{ d.dirName }}
            </button>
            <button
              v-if="!mobileMode && isTauri"
              class="chip chip-dashed"
              @click="handleBrowse"
            >
              + Browse
            </button>
          </div>
        </div>
        <div v-if="selectedCwd && !uniqueDirs.some(d => d.path === selectedCwd)" class="config-selected-path">
          {{ selectedCwd }}
        </div>

        <!-- Project -->
        <div class="config-row">
          <span class="config-label">Project</span>
          <div class="config-chips">
            <button
              v-for="p in matchingProjects"
              :key="p.id"
              class="chip"
              :class="{ active: selectedProjectId === p.id }"
              @click="selectProject(p.id)"
            >
              {{ p.name }}
            </button>
            <button
              class="chip"
              :class="{ active: !selectedProjectId && !createNewProject }"
              @click="selectedProjectId = ''; createNewProject = false"
            >
              None
            </button>
            <button
              v-if="!mobileMode"
              class="chip chip-dashed"
              :class="{ active: createNewProject }"
              @click="createNewProject = !createNewProject; selectedProjectId = ''"
            >
              + New
            </button>
          </div>
        </div>

        <!-- New project name (inline, only when creating) -->
        <div v-if="createNewProject" class="config-row">
          <span class="config-label">Name</span>
          <input
            v-model="newProjectName"
            type="text"
            class="config-input"
            placeholder="My project"
          />
        </div>

        <p v-if="error" class="error">{{ error }}</p>
      </div>

      <!-- Prompt input — the actual first message -->
      <div class="prompt-area">
        <input
          v-model="firstMessage"
          type="text"
          class="prompt-input"
          placeholder="What do you want to work on?"
          @keydown.enter="handleCreate"
        />
        <button class="send-btn" @click="handleCreate" :disabled="!selectedCwd">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/>
          </svg>
        </button>
      </div>
      <p class="hint">Press Enter or click send to start — session name is auto-generated</p>
    </div>
  </div>
</template>

<style scoped>
.welcome-pane {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: 2rem;
}

.welcome-content {
  max-width: 600px;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.welcome-header {
  text-align: center;
}

.welcome-header h1 {
  font-size: 1.8rem;
  font-weight: 600;
  margin: 0;
  color: var(--text-primary);
}

.tagline {
  color: var(--text-secondary);
  font-size: 0.95rem;
  margin: 0.4rem 0 0;
}

/* Config area */
.config-area {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  padding: 1rem 1.25rem;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg, 12px);
  background: var(--bg-panel);
}

.config-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.config-label {
  font-size: 0.75rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  min-width: 60px;
  flex-shrink: 0;
}

.config-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}

.chip {
  padding: 0.3rem 0.65rem;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: transparent;
  color: var(--text-primary);
  font-size: 0.78rem;
  cursor: pointer;
  transition: all 0.15s;
  white-space: nowrap;
}

.chip:hover {
  border-color: var(--accent);
}

.chip.active {
  background: var(--accent, #3b82f6);
  border-color: var(--accent);
  color: white;
}

.chip-dashed {
  border-style: dashed;
  color: var(--text-secondary);
}

.config-input {
  flex: 1;
  padding: 0.3rem 0.6rem;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm, 6px);
  background: transparent;
  color: var(--text-primary);
  font-size: 0.85rem;
  outline: none;
}

.config-input:focus {
  border-color: var(--accent);
}

.config-selected-path {
  margin-top: -0.25rem;
  margin-left: 68px;
  font-size: 0.72rem;
  color: var(--text-tertiary, var(--text-secondary));
  font-family: monospace;
  word-break: break-all;
  line-height: 1.3;
}

.error {
  color: var(--text-error, #ef4444);
  font-size: 0.8rem;
  margin: 0;
}

/* Prompt area */
.prompt-area {
  display: flex;
  gap: 0.5rem;
}

.prompt-input {
  flex: 1;
  padding: 0.85rem 1rem;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg, 12px);
  background: var(--bg-input, var(--bg-panel));
  color: var(--text-primary);
  font-size: 1rem;
  outline: none;
}

.prompt-input:focus {
  border-color: var(--accent);
}

.send-btn {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-lg, 12px);
  border: none;
  background: var(--accent, #3b82f6);
  color: white;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.send-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.send-btn:not(:disabled):hover {
  opacity: 0.9;
}

.hint {
  font-size: 0.75rem;
  color: var(--text-tertiary, var(--text-secondary));
  margin: 0;
  text-align: center;
}
</style>
tyle>
