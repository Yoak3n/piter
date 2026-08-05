<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Paperclip, FileText, X } from 'lucide-vue-next'
import type { Attachment, ModelRef } from '../../types'
import { useFileDrop } from '../../composables/useFileDrop'
import { filesToAttachments, clipboardImageFiles } from '../../utils/attachments'
import { formatBytes, imageContentToSrc } from '../../utils/image'

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
const firstMessage = ref('')
const error = ref('')
/** 拖拽命中测试目标（Tauri 原生拖拽需按坐标判断是否落在输入区） */
const promptAreaRef = ref<HTMLElement | null>(null)
const fileInputRef = ref<HTMLInputElement | null>(null)

// ─── 附件（拖入文件暂存，随首条消息发送） ───────────────────────────────
const pendingAttachments = ref<Attachment[]>([])

const hintMsg = ref('')
let hintTimer: ReturnType<typeof setTimeout> | null = null
function showHint(msg: string) {
  hintMsg.value = msg
  if (hintTimer) clearTimeout(hintTimer)
  hintTimer = setTimeout(() => { hintMsg.value = '' }, 4000)
}

function removeAttachment(id: string) {
  pendingAttachments.value = pendingAttachments.value.filter((a) => a.id !== id)
}

/** 处理拖入/选中的文件（与 Composer 共用 filesToAttachments） */
async function addFiles(files: File[]) {
  const added = await filesToAttachments(files, {
    t: (k) => t(k),
    currentModel: props.currentModel,
    onHint: showHint,
  })
  if (added.length) pendingAttachments.value.push(...added)
}

/** 按钮选择文件（移动端/浏览器没有 OS 拖拽时的入口） */
function handleFiles(e: Event) {
  const input = e.target as HTMLInputElement
  const files = Array.from(input.files || [])
  // 清空 value 以便再次选择同一文件
  input.value = ''
  if (!files.length) return
  addFiles(files)
}

/** 粘贴图片（如 QQ 截图 Ctrl+V）→ 进入附件链路；剪贴板无图片时走默认文本粘贴 */
function handlePaste(e: ClipboardEvent) {
  if (!props.isRunning) return
  const files = clipboardImageFiles(e)
  if (!files.length) return
  e.preventDefault()
  addFiles(files)
}

const { isDragging, onDragEnter, onDragOver, onDragLeave, onDrop } = useFileDrop({
  enabled: () => props.isRunning,
  onFiles: addFiles,
  target: promptAreaRef,
})

// ─── First-launch onboarding ───────────────────────────────────────────
// Shown once until dismissed or the first session is created — three quick
// steps to get a new user from zero to their first question.
const ONBOARDING_KEY = 'piter-onboarded'
const showGuide = ref(false)

function dismissGuide() {
  showGuide.value = false
  try { localStorage.setItem(ONBOARDING_KEY, '1') } catch { /* ignore */ }
}

onMounted(() => {
  try {
    showGuide.value = !localStorage.getItem(ONBOARDING_KEY)
  } catch {
    showGuide.value = true
  }
})

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

// Onboarding step 1 → jump to the desktop settings (Providers tab area).
async function openSettings() {
  if (!isTauri) return
  try {
    const { emit } = await import('@tauri-apps/api/event')
    await emit('navigate-to-admin')
  } catch { /* non-critical */ }
}

function handleCreate() {
  if (!selectedCwd.value.trim()) {
    error.value = t('chat.selectDirError')
    return
  }
  error.value = ''
  dismissGuide()

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

  const payload: { cwd: string; name: string; message?: string; attachments?: Attachment[] } = {
    cwd: selectedCwd.value,
    name,
  }
  if (firstMessage.value.trim()) {
    payload.message = firstMessage.value.trim()
  }
  if (pendingAttachments.value.length) {
    payload.attachments = pendingAttachments.value
  }
  emit('create', payload)
}
</script>

<template>
  <div class="welcome-pane">
    <div class="welcome-content">
      <div class="welcome-header">
        <h1>{{ $t("chat.paneTitle") }}</h1>
        <p class="tagline">{{ $t("chat.paneTagline") }}</p>
      </div>

      <!-- First-launch guide: three steps to your first question -->
      <div v-if="showGuide" class="onboarding">
        <button
          class="onboarding-close"
          :aria-label="$t('chat.dismiss')"
          :title="$t('chat.dismiss')"
          @click="dismissGuide"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
        <div class="onboarding-emoji">👋</div>
        <h2 class="onboarding-title">{{ $t("chat.welcomeTitle") }}</h2>
        <ol class="onboarding-steps">
          <li class="onboarding-step">
            <span class="onboarding-num">1</span>
            <div class="onboarding-body">
              <strong>{{ $t("chat.stepAddProvider") }}</strong>
              <i18n-t keypath="chat.stepAddProviderDesc" tag="span">
                <template #settings>
                  <button v-if="isTauri" class="step-link" @click="openSettings">{{ $t("chat.settingsLink") }}</button>
                  <template v-else>{{ $t("chat.settingsLink") }}</template>
                </template>
              </i18n-t>
            </div>
          </li>
          <li class="onboarding-step">
            <span class="onboarding-num">2</span>
            <div class="onboarding-body">
              <strong>{{ $t("chat.stepCreateSession") }}</strong>
              <span>{{ $t("chat.stepCreateSessionDesc") }}</span>
            </div>
          </li>
          <li class="onboarding-step">
            <span class="onboarding-num">3</span>
            <div class="onboarding-body">
              <strong>{{ $t("chat.stepAsk") }}</strong>
              <span>{{ $t("chat.stepAskDesc") }}</span>
            </div>
          </li>
        </ol>
        <button class="btn btn-primary onboarding-done" @click="dismissGuide">{{ $t("common.gotIt") }}</button>
      </div>

      <!-- Environment configuration -->
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
              @click="selectDir(d.path)"
            >
              {{ d.dirName }}
            </button>
            <button
              v-if="!mobileMode && isTauri"
              class="chip chip-dashed"
              @click="handleBrowse"
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
              @click="selectProject(p.id)"
            >
              {{ p.name }}
            </button>
            <button
              class="chip"
              :class="{ active: !selectedProjectId && !createNewProject }"
              :title="$t('chat.autoProjectTitle')"
              @click="selectedProjectId = ''; createNewProject = false"
            >
              {{ $t("chat.autoProject") }}
            </button>
            <button
              v-if="!mobileMode"
              class="chip chip-dashed"
              :class="{ active: createNewProject }"
              @click="createNewProject = !createNewProject; selectedProjectId = ''"
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
            v-model="newProjectName"
            type="text"
            class="config-input"
            :placeholder="$t('chat.projectPlaceholder')"
          />
        </div>

        <p v-if="error" class="error">{{ error }}</p>
      </div>

      <!-- Prompt input — the actual first message -->
      <div class="prompt-wrap">
        <div v-if="pendingAttachments.length" class="pending-attachments">
          <div
            v-for="att in pendingAttachments"
            :key="att.id"
            class="attachment-chip"
            :title="att.fileName"
          >
            <img v-if="att.type === 'image' && att.data" :src="imageContentToSrc(att)" class="attachment-thumb" alt="" />
            <span v-else class="attachment-file-icon"><FileText :size="14" /></span>
            <span class="attachment-name">{{ att.fileName }}</span>
            <span class="attachment-size">{{ formatBytes(att.size) }}</span>
            <button
              class="attachment-remove"
              :title="t('chat.removeAttachment')"
              :aria-label="t('chat.removeAttachment')"
              @click="removeAttachment(att.id)"
            >
              <X :size="12" />
            </button>
          </div>
        </div>
        <div
          ref="promptAreaRef"
          class="prompt-area"
          :class="{ 'is-dragging': isDragging }"
          @dragenter="onDragEnter"
          @dragover="onDragOver"
          @dragleave="onDragLeave"
          @drop="onDrop"
        >
          <div v-if="isDragging" class="prompt-drop-overlay" aria-hidden="true">
            <Paperclip :size="18" />
            <span>{{ $t("chat.dropFilesHint") }}</span>
          </div>
          <input
            v-model="firstMessage"
            type="text"
            class="prompt-input"
            :placeholder="$t('chat.promptPlaceholder')"
            @keydown.enter="handleCreate"
            @paste="handlePaste"
          />
          <input
            ref="fileInputRef"
            type="file"
            class="file-input"
            accept="image/*,.txt,.md,.json,.csv,.log"
            multiple
            @change="handleFiles"
          />
          <button
            v-if="isRunning"
            class="attach-btn"
            :title="t('chat.attachFiles')"
            :aria-label="t('chat.attachFiles')"
            @click="fileInputRef?.click()"
          >
            <Paperclip :size="16" />
          </button>
          <button class="send-btn" @click="handleCreate" :disabled="!selectedCwd">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/>
            </svg>
          </button>
        </div>
        <p v-if="hintMsg" class="prompt-hint" role="status">{{ hintMsg }}</p>
        <p class="hint">{{ $t("chat.paneHint") }}</p>
      </div>
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

/* Config area */
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

/* Prompt area */
.prompt-wrap {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.prompt-area {
  position: relative;
  display: flex;
  gap: 0.75rem;
}

.prompt-drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  border: 1.5px dashed var(--accent);
  border-radius: var(--radius-lg);
  background: color-mix(in srgb, var(--accent) 8%, var(--bg-panel));
  color: var(--accent);
  font-size: 0.95rem;
  font-weight: 500;
  pointer-events: none;
}

.prompt-area.is-dragging .prompt-input {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.pending-attachments {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.attachment-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 220px;
  padding: 3px 6px 3px 3px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-muted);
}

.attachment-thumb {
  width: 28px;
  height: 28px;
  border-radius: 4px;
  object-fit: cover;
  border: 1px solid var(--border);
  background: var(--bg-panel);
  flex-shrink: 0;
}

.attachment-file-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 4px;
  flex-shrink: 0;
  background: var(--bg-panel);
  color: var(--text-tertiary);
  border: 1px solid var(--border);
}

.attachment-name {
  font-size: 0.75rem;
  color: var(--text);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.attachment-size {
  font-size: 0.7rem;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.attachment-remove {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  padding: 0;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.attachment-remove:hover {
  background: var(--danger-soft);
  color: var(--danger);
}

.prompt-hint {
  color: var(--warning);
  font-size: 0.85rem;
  margin: 0;
}

.prompt-input {
  flex: 1;
  padding: 1rem 1.25rem;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-panel);
  color: var(--text);
  font-size: 1.1rem;
  outline: none;
  box-shadow: var(--shadow-sm);
}

.file-input { display: none; }

.attach-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  background: var(--bg-panel);
  color: var(--text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease), border-color var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.attach-btn:hover {
  color: var(--text);
  border-color: var(--border-strong);
  background: var(--bg-hover);
}

.prompt-input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.send-btn {
  width: 52px;
  height: 52px;
  border-radius: var(--radius-lg);
  border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  background: var(--accent-soft);
  color: var(--accent-strong);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease), border-color var(--duration-fast) var(--ease), transform 0.1s var(--ease);
}

.send-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.send-btn:not(:disabled):hover {
  background: var(--accent-glow);
  border-color: var(--accent);
}

.send-btn:not(:disabled):active {
  transform: scale(0.96);
}

.hint {
  font-size: 0.85rem;
  color: var(--text-tertiary);
  margin: 0;
  text-align: center;
}

/* ── First-launch onboarding ── */
.onboarding {
  position: relative;
  padding: 1.75rem 2rem;
  border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
  border-radius: var(--radius-lg);
  background: var(--accent-soft);
  text-align: center;
  animation: onboard-pop var(--duration) var(--spring);
}

@keyframes onboard-pop {
  from { opacity: 0; transform: translateY(6px); }
  to { opacity: 1; transform: translateY(0); }
}

.onboarding-close {
  position: absolute;
  top: 10px;
  right: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.onboarding-close:hover {
  background: var(--bg-panel);
  color: var(--text);
}

.onboarding-emoji {
  font-size: 2.5rem;
  line-height: 1;
}

.onboarding-title {
  margin: 0.75rem 0 0.5rem;
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text);
}

.onboarding-steps {
  list-style: none;
  margin: 1rem 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  text-align: left;
}

.onboarding-step {
  display: flex;
  align-items: flex-start;
  gap: 0.8rem;
}

.onboarding-num {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  flex-shrink: 0;
  margin-top: 1px;
  border-radius: 50%;
  background: var(--accent-soft);
  border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  color: var(--accent-strong);
  font-size: 0.8rem;
  font-weight: 600;
  line-height: 1;
}

.onboarding-body {
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: 0.95rem;
  color: var(--text-secondary);
}

.onboarding-body strong {
  color: var(--text);
  font-weight: 600;
  font-size: 1.05rem;
}

.step-link {
  border: none;
  background: none;
  padding: 0;
  color: var(--accent);
  font-weight: 600;
  font-size: inherit;
  cursor: pointer;
  text-decoration: underline;
}

.onboarding-done {
  margin-top: 1.25rem;
  height: 38px;
  padding: 0 26px;
  font-size: 0.9rem;
}
</style>
