<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import type { Attachment, ModelRef } from "../../types";
import OnboardingGuide from "./OnboardingGuide.vue";
import NewSessionForm from "./NewSessionForm.vue";
import NewSessionPrompt from "./NewSessionPrompt.vue";

// ─── 新会话准备页（组合三块：首启引导 + 环境配置 + 首条消息）──
// 配置区（目录/项目选择）、首条消息+附件、引导卡分别在子组件；本组件保留
// 会话创建编排（目录/项目状态、DB 项目加载、名称解析与 create 载荷组装）。

const { t } = useI18n()

// Only show the 'Browse' button when running inside Tauri (native dialog)
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
const props = defineProps<{
  projects: Array<{ path: string; name: string }>
  mobileMode: boolean
  /** 预选的工作目录（点击侧栏项目级 "+" 时传入，快速为该目录建会话） */
  initialCwd?: string
  /** 预选的项目名（与 initialCwd 配套，用于选中 DB 中匹配的项目） */
  initialName?: string
  /** pi 是否已连接（未连接时不响应文件拖拽） */
  isRunning: boolean
  /** 当前会话模型（用于多模态预检弱提示） */
  currentModel?: ModelRef | null
}>()

const emit = defineEmits<{
  (e: 'create', payload: { cwd: string; name: string; message?: string; attachments?: Attachment[] }): void
}>()

const selectedCwd = ref('')
const selectedProjectId = ref('')
const createNewProject = ref(false)
const newProjectName = ref('')
const error = ref('')

/** 创建成功后关闭首启引导（子组件内部自持显示状态，经 ref 调用） */
const guideRef = ref<InstanceType<typeof OnboardingGuide> | null>(null)

// ─── DB 项目加载与预选 ─────────────────────────────────────────────
const dbProjects = ref<Array<{ id: string; name: string; cwd: string }>>([])

async function fetchProjects() {
  try {
    const resp = await fetch('/api/projects')
    const data = await resp.json()
    if (data.success) dbProjects.value = data.projects
  } catch (err) {
    const errorMsg = err instanceof Error ? `Failed to fetch projects: ${err.message}` : 'Unknown error'
    error.value = t('chat.fetchProjectsError', { msg: errorMsg })
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

// 根据项目的path来去重
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

// Project name when "Auto" is selected: falls back to the directory name.
const autoProjectName = computed(() =>
  selectedCwd.value
    ? selectedCwd.value.split(/[/\\]/).filter(Boolean).pop() || 'New Session'
    : ''
)

/** 应用预选：挂载时 + 面板已打开再点其他项目 "+"（props 变化）时都要生效 */
function applyInitialSelection() {
  const cwd = props.initialCwd
  if (cwd) {
    selectedCwd.value = cwd
    // preselectProject 依赖异步加载的 dbProjects；DB 未收录时退化为新建项目并预填名称
    preselectProject()
  } else {
    selectedCwd.value = uniqueDirs.value[0]?.path ?? ''
    selectedProjectId.value = ''
    createNewProject.value = false
  }
}

onMounted(() => {
  fetchProjects()
  applyInitialSelection()
})

// 准备页可能已经打开（showNewSession 未变、组件未重挂载）：
// 此时点侧栏其他项目的 "+" 只改 props，需重新预选，否则停留在旧目录。
watch(
  () => [props.initialCwd, props.initialName] as const,
  () => applyInitialSelection(),
)

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
    open({ directory: true, multiple: false, title: t('chat.browseTitle') }).then((sel) => {
      if (sel && typeof sel === 'string') {
        selectedCwd.value = sel
        selectedProjectId.value = ''
        error.value = ''
      }
    })
  }).catch(() => {})
}

/** 首条消息区提交：目录校验 + 项目名解析 + create 载荷组装（消息/附件来自 NewSessionPrompt） */
function handleCreate(payload: { message?: string; attachments?: Attachment[] } = {}) {
  if (!selectedCwd.value.trim()) {
    error.value = t('chat.selectDirError')
    return
  }
  error.value = ''
  guideRef.value?.dismiss()

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

  const out: { cwd: string; name: string; message?: string; attachments?: Attachment[] } = {
    cwd: selectedCwd.value,
    name,
  }
  if (payload.message?.trim()) {
    out.message = payload.message.trim()
  }
  if (payload.attachments?.length) {
    out.attachments = payload.attachments
  }
  emit('create', out)
}
</script>

<template>
  <div class="welcome-pane">
    <div class="welcome-content">
      <div class="welcome-header">
        <h1>{{ $t("chat.paneTitle") }}</h1>
        <p class="tagline">{{ $t("chat.paneTagline") }}</p>
      </div>

      <OnboardingGuide ref="guideRef" :is-tauri="isTauri" />

      <NewSessionForm
        :unique-dirs="uniqueDirs"
        :selected-cwd="selectedCwd"
        :matching-projects="matchingProjects"
        :selected-project-id="selectedProjectId"
        :create-new-project="createNewProject"
        :new-project-name="newProjectName"
        :auto-project-name="autoProjectName"
        :error="error"
        :mobile-mode="mobileMode"
        :is-tauri="isTauri"
        @select-dir="selectDir"
        @select-project="selectProject"
        @browse="handleBrowse"
        @toggle-new-project="(v) => { createNewProject = v; selectedProjectId = '' }"
        @update:new-project-name="(v) => (newProjectName = v)"
      />

      <NewSessionPrompt
        :is-running="isRunning"
        :current-model="currentModel"
        :can-create="!!selectedCwd"
        @submit="handleCreate"
      />
    </div>
  </div>
</template>

<style scoped>
.welcome-pane {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: 3rem;
}

.welcome-content {
  max-width: 680px;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 2.25rem;
}

.welcome-header {
  text-align: center;
  margin-bottom: 0.75rem;
}

.welcome-header h1 {
  font-size: 2.6rem;
  font-weight: 600;
  margin: 0;
  color: var(--text);
}

.tagline {
  color: var(--text-secondary);
  font-size: 1.05rem;
  margin: 0.6rem 0 0;
}
</style>
