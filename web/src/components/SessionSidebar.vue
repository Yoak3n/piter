<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { Trash2, Plus, Search, X, RefreshCw } from "lucide-vue-next";

export interface SessionInfo {
  id: string;
  label: string;
  createdAt: string;
  filePath: string;
  updatedAt: number;
  preview: string;
  cwd: string;
  // Runtime state (from backend session manager)
  instanceId?: string;
  state?: "active" | "idle" | "unloaded";
  model?: string;
  thinkingLevel?: string;
  messageCount?: number;
  messageSeq?: number;
}

export interface ProjectGroup {
  path: string;
  dirName: string;
  sessions: SessionInfo[];
}

const props = defineProps<{
  activeSessionPath: string | null;
  projects?: ProjectGroup[];
  sessionStatus?: "running" | "idle" | null;
  mobileMode?: boolean;
}>();

const emit = defineEmits<{
  (e: "select-session", path: string): void;
  (e: "delete-session", path: string): void;
  (e: "new-session", cwd?: string): void;
}>();

const projects = ref<ProjectGroup[]>([]);
const loading = ref(true);
const error = ref("");
const searchQuery = ref("");
const collapsedProjects = ref<Set<string>>(new Set());
const showDeleteConfirm = ref<string | null>(null);
const deleteLoading = ref(false);

function handleNewSession() {
  emit("new-session");
}

const filteredProjects = computed(() => {
  const q = searchQuery.value.toLowerCase().trim();
  if (!q) return projects.value;

  return projects.value
    .map((p) => ({
      ...p,
      sessions: p.sessions.filter(
        (s) =>
          s.label.toLowerCase().includes(q) ||
          s.preview.toLowerCase().includes(q),
      ),
    }))
    .filter((p) => p.sessions.length > 0);
});

// Sync externally pushed session data into local state
watch(
  () => props.projects,
  (ext) => {
    if (ext && ext.length > 0) {
      projects.value = ext;
      loading.value = false;
    }
  },
);

async function fetchSessions() {
  loading.value = true;
  error.value = "";
  try {
    const res = await fetch("/api/sessions");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    projects.value = data.projects || [];
  } catch (e: any) {
    error.value = e.message || "Failed to load sessions";
  } finally {
    loading.value = false;
  }
}

function toggleProject(dirName: string) {
  const s = new Set(collapsedProjects.value);
  if (s.has(dirName)) s.delete(dirName);
  else s.add(dirName);
  collapsedProjects.value = s;
}

function formatTime(updatedAt: number): string {
  const now = Date.now();
  const diffMs = now - updatedAt * 1000;
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const days = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return "Just now";
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (days === 1) return "Yesterday";
  if (days < 7)
    return new Date(updatedAt * 1000).toLocaleDateString([], {
      weekday: "long",
    });
  return new Date(updatedAt * 1000).toLocaleDateString([], {
    month: "short",
    day: "numeric",
  });
}

async function handleDelete(filePath: string) {
  deleteLoading.value = true;
  try {
    await fetch(
      `/api/delete-session?path=${encodeURIComponent(filePath)}`,
    );
    await fetchSessions();
    emit("delete-session", filePath);
  } catch (e) {
    console.error("Delete failed:", e);
  } finally {
    deleteLoading.value = false;
    showDeleteConfirm.value = null;
  }
}

onMounted(fetchSessions);
</script>

<template>
  <div class="sidebar-panel">
    <!-- Sidebar header -->
    <div class="sidebar-header">
      <div class="sidebar-search-wrap">
        <Search :size="14" class="search-icon" />
        <input
          v-model="searchQuery"
          type="text"
          class="sidebar-search-input"
          placeholder="Search sessions..."
          autocomplete="off"
        />
        <button
          v-if="searchQuery"
          class="search-clear"
          @click="searchQuery = ''"
          title="Clear search"
        >
          <X :size="12" />
        </button>
      </div>
      <div class="sidebar-actions">
        <button
          class="btn btn-ghost btn-icon btn-sm"
          title="Refresh sessions"
          @click="fetchSessions"
          :disabled="loading"
        >
          <RefreshCw :size="14" :class="{ spinning: loading }" />
        </button>
        <button
          class="btn btn-ghost btn-icon btn-sm"
          title="New session"
          @click="handleNewSession"
        >
          <Plus :size="16" />
        </button>
      </div>
    </div>

    <!-- Session list -->
    <div class="sidebar-sessions">
      <!-- Loading skeleton -->
      <div v-if="loading" class="skeleton-list">
        <div
          v-for="i in 6"
          :key="i"
          class="skeleton-item"
        >
          <div class="skeleton-line skeleton-title" />
          <div class="skeleton-line skeleton-meta" />
        </div>
      </div>

      <!-- Error state -->
      <div v-else-if="error" class="sidebar-empty">
        <p class="empty-text">Failed to load sessions</p>
        <button class="btn btn-ghost btn-sm" @click="fetchSessions">
          Retry
        </button>
      </div>

      <!-- Empty state -->
      <div
        v-else-if="projects.length === 0"
        class="sidebar-empty"
      >
        <div class="empty-icon">
          <svg
            width="36"
            height="36"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path
              d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
            />
            <line x1="12" y1="12" x2="12" y2="18" />
            <line x1="9" y1="15" x2="15" y2="15" />
          </svg>
        </div>
        <p class="empty-title">No sessions yet</p>
        <p class="empty-hint">
          Start a new chat to create your first session.
        </p>
      </div>

      <!-- Project groups -->
      <template v-else>
        <div
          v-for="project in filteredProjects"
          :key="project.path"
          class="project-group"
        >
          <button
            class="project-header"
            :title="project.path"
            @click="toggleProject(project.dirName)"
          >
            <span
              class="project-chevron"
              :class="{ collapsed: collapsedProjects.has(project.dirName) }"
            >&#9660;</span>
            <span class="project-name">{{ project.dirName }}</span>
            <span class="project-count">{{ project.sessions.length }}</span>
            <button
              class="project-new-btn"
              title="New chat"
              @click.stop="emit('new-session', project.path)"
            >
              <Plus :size="12" />
            </button>
          </button>

          <div
            v-if="!collapsedProjects.has(project.dirName)"
            class="project-sessions"
          >
            <button
              v-for="session in project.sessions"
              :key="session.filePath"
              class="session-item"
              :class="{
                active: session.filePath === activeSessionPath,
              }"
              @click="emit('select-session', session.filePath)"
            >
              <div class="session-item-main">
                <div class="session-title">
                  <span
                    class="session-status-dot"
                    :class="{
                      'status-active': session.state === 'active',
                      'status-idle': session.state === 'idle',
                    }"
                    :title="session.state || 'unloaded'"
                  />
                  <span
                    v-if="session.filePath === activeSessionPath && sessionStatus === 'running'"
                    class="session-running-indicator"
                    title="Pi is processing..."
                  />
                  {{ session.label || "Untitled" }}
                </div>
                <div class="session-meta">
                  <span class="session-time">{{
                    formatTime(session.updatedAt)
                  }}</span>
                </div>
              </div>

              <!-- Delete confirm -->
              <template v-if="showDeleteConfirm === session.filePath">
                <div class="delete-confirm" @click.stop>
                  <span class="delete-confirm-text">Delete?</span>
                  <button
                    class="btn btn-sm btn-danger"
                    :disabled="deleteLoading"
                    @click="handleDelete(session.filePath)"
                  >
                    Yes
                  </button>
                  <button
                    class="btn btn-sm btn-ghost"
                    @click="showDeleteConfirm = null"
                  >
                    No
                  </button>
                </div>
              </template>
              <template v-else>
                <button
                  class="session-delete-btn"
                  title="Delete session"
                  @click.stop="showDeleteConfirm = session.filePath"
                >
                  <Trash2 :size="12" />
                </button>
              </template>
            </button>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.sidebar-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--color-bg-sidebar);
  border-right: 1px solid var(--color-border-subtle);
}

.sidebar-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 10px;
  border-bottom: 1px solid var(--color-border-subtle);
  flex-shrink: 0;
}

.sidebar-search-wrap {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  background: var(--color-bg-panel);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-md);
  min-width: 0;
}

.search-icon {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.sidebar-search-input {
  flex: 1;
  border: none;
  background: none;
  outline: none;
  color: var(--color-text-primary);
  font-size: 12px;
  min-width: 0;
}

.sidebar-search-input::placeholder {
  color: var(--color-text-tertiary);
}

.search-clear {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  border-radius: 3px;
  color: var(--color-text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
}

.search-clear:hover {
  color: var(--color-text-primary);
  background: var(--color-bg-hover);
}

.sidebar-actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
}

/* Sessions */
.sidebar-sessions {
  flex: 1;
  overflow-y: auto;
  padding: 6px 0;
}

/* Skeleton */
.skeleton-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 8px;
}

.skeleton-item {
  padding: 10px 10px;
  border-radius: var(--radius-md);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.skeleton-line {
  height: 10px;
  border-radius: 4px;
  background: var(--color-bg-muted);
  animation: shimmer 1.5s ease-in-out infinite;
}

.skeleton-title {
  width: 70%;
}

.skeleton-meta {
  width: 40%;
}

@keyframes shimmer {
  0%,
  100% {
    opacity: 0.4;
  }
  50% {
    opacity: 0.8;
  }
}

/* Empty */
.sidebar-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px 16px;
  gap: 8px;
  text-align: center;
}

.empty-icon {
  color: var(--color-text-tertiary);
  opacity: 0.6;
}

.empty-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--color-text-secondary);
  margin: 0;
}

.empty-text,
.empty-hint {
  font-size: 11px;
  color: var(--color-text-tertiary);
  margin: 0;
}

/* Project group */
.project-group {
  border-bottom: 1px solid var(--color-border-subtle);
}

.project-group:last-child {
  border-bottom: none;
}

.project-header {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 8px 10px;
  font-size: 11px;
  font-weight: 600;
  color: var(--color-text-secondary);
  cursor: pointer;
  border: none;
  background: none;
  text-align: left;
}

.project-header:hover {
  background: var(--color-bg-hover);
}

.project-chevron {
  font-size: 8px;
  transition: transform 0.15s ease;
  flex-shrink: 0;
}

.project-chevron.collapsed {
  transform: rotate(-90deg);
}

.project-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.project-count {
  font-size: 10px;
  color: var(--color-text-tertiary);
  font-weight: 400;
}

.project-new-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 4px;
  color: var(--color-text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.project-header:hover .project-new-btn {
  opacity: 1;
}

.project-new-btn:hover {
  background: var(--color-bg-active);
  color: var(--color-text-primary);
}

/* Session item */
.project-sessions {
  display: flex;
  flex-direction: column;
}

.session-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px 8px 22px;
  cursor: pointer;
  border: none;
  background: none;
  width: 100%;
  text-align: left;
  font-size: 12px;
  color: var(--color-text-primary);
  gap: 6px;
}

.session-item:hover {
  background: var(--color-bg-hover);
}

.session-item.active {
  background: var(--color-accent-soft);
  border-left: 2px solid var(--color-accent);
  padding-left: 20px;
}

.session-item-main {
  flex: 1;
  min-width: 0;
}

.session-title {
  font-size: 12px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  display: flex;
  align-items: center;
  gap: 5px;
}

.session-running-indicator {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-accent);
  flex-shrink: 0;
  animation: pulse-dot 1.2s ease-in-out infinite;
}

@keyframes pulse-dot {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.session-status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  background: var(--color-text-tertiary);
  opacity: 0.3;
}

.session-status-dot.status-active {
  background: #22c55e;
  opacity: 1;
}

.session-status-dot.status-idle {
  background: #f59e0b;
  opacity: 0.7;
}

.session-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 2px;
}

.session-time {
  font-size: 10px;
  color: var(--color-text-tertiary);
}

.session-delete-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 4px;
  color: var(--color-text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.session-item:hover .session-delete-btn {
  opacity: 1;
}

.session-delete-btn:hover {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

/* Delete confirm inline */
.delete-confirm {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 4px;
  background: var(--color-bg-panel);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.delete-confirm-text {
  font-size: 11px;
  color: var(--color-danger);
  white-space: nowrap;
}

.spinning {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* Mobile */
@media (max-width: 640px) {
  .sidebar-panel {
    position: fixed;
    inset: 0;
    z-index: 40;
    max-width: 300px;
  }

  .session-item .session-delete-btn {
    opacity: 1;
  }
}
</style>
