<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { Plus, Search, X, RefreshCw } from "lucide-vue-next";
import { EmptyState, SkeletonList } from "@piter/ui";
import ProjectGroup from "./ProjectGroup.vue";
import { mapProjectGroups } from "../../utils/projects";
import type { ProjectGroup as ProjectGroupType } from "../../types";

const props = defineProps<{
  activeSessionId: string | null;
  projects?: ProjectGroupType[];
  sessionStatus?: "running" | "idle" | null;
  mobileMode?: boolean;
}>();

const { t } = useI18n();

const emit = defineEmits<{
  (e: "select-session", instanceId: string): void;
  (e: "delete-session", instanceId: string): void;
  (e: "new-session", cwd?: string, name?: string): void;
}>();

const projects = ref<ProjectGroupType[]>([]);
const loading = ref(true);
const error = ref("");
const searchQuery = ref("");
// Explicit collapse choices keyed by unique project identity (id ?? path).
// Absence means "use the default": expanded for active projects, collapsed
// for archived ones. Keyed by id/path instead of name so same-named projects
// (different cwd) don't share a collapse state.
const collapseChoice = ref<Map<string, boolean>>(new Map());
const showDeleteConfirm = ref<string | null>(null);
const deleteLoading = ref(false);

// ─── Project pin / archive ─────────────────────────────────────────────
const archiveConfirm = ref<string | null>(null);
const actionLoading = ref(false);

async function togglePin(project: ProjectGroupType) {
  if (!project.id || actionLoading.value) return;
  actionLoading.value = true;
  try {
    await fetch(`/api/projects/${encodeURIComponent(project.id)}/pin`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ pinned: project.pinned ? 0 : 1 }),
    });
    await fetchSessions();
  } catch (e) {
    console.error("Pin failed:", e);
  } finally {
    actionLoading.value = false;
  }
}

function confirmArchive(project: ProjectGroupType) {
  if (!project.id) return;
  archiveConfirm.value = project.id;
}

async function doArchive() {
  const id = archiveConfirm.value;
  if (!id || actionLoading.value) return;
  actionLoading.value = true;
  try {
    await fetch(`/api/projects/${encodeURIComponent(id)}/archive`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ archived: true }),
    });
    archiveConfirm.value = null;
    await fetchSessions();
  } catch (e) {
    console.error("Archive failed:", e);
  } finally {
    actionLoading.value = false;
  }
}

async function restoreProject(id: string) {
  if (actionLoading.value) return;
  actionLoading.value = true;
  try {
    await fetch(`/api/projects/${encodeURIComponent(id)}/archive`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ archived: false }),
    });
    await fetchSessions();
  } catch (e) {
    console.error("Restore failed:", e);
  } finally {
    actionLoading.value = false;
  }
}

function handleNewSession() {
  emit("new-session");
}

const filteredProjects = computed(() => {
  const q = searchQuery.value.toLowerCase().trim();
  if (!q) return projects.value;

  return projects.value
    .map((p) => ({
      ...p,
      // Archived projects are matched by project name/path (sessions kept as-is);
      // active projects match by session label/preview.
      sessions: p.archived
        ? p.sessions
        : p.sessions.filter(
            (s) =>
              s.label.toLowerCase().includes(q) ||
              s.preview.toLowerCase().includes(q),
          ),
    }))
    .filter((p) =>
      p.archived
        ? p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q)
        : p.sessions.length > 0,
    );
});

// Archived projects are returned by the backend at the end of the list;
// render them under a dedicated "Archive" section.
const normalProjects = computed(() =>
  filteredProjects.value.filter((p) => !p.archived),
);
const archivedFiltered = computed(() =>
  filteredProjects.value.filter((p) => p.archived),
);

// Sync externally pushed session data into local state
watch(
  () => props.projects,
  (ext) => {
    if (ext) {
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
    // Same normalization as the WS sessions_list path / useSessions, so the
    // sidebar list stays consistent no matter which path refreshed it.
    projects.value = mapProjectGroups(data.projects || []);
  } catch (e: any) {
    error.value = e.message || t("chat.loadErrorTitle");
  } finally {
    loading.value = false;
  }
}

function projectKey(p: ProjectGroupType): string {
  return p.id ?? p.path;
}

// Session identity matches the select/delete flow (instanceId with a fallback
// to the DB id), so pinning targets the same key the rest of the UI uses.
function sessionKey(s: { instanceId?: string; id: string }): string {
  return s.instanceId ?? s.id;
}

async function toggleSessionPin(session: ProjectGroupType["sessions"][number]) {
  if (actionLoading.value) return;
  actionLoading.value = true;
  try {
    await fetch(`/api/sessions/${encodeURIComponent(sessionKey(session))}/pin`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ pinned: session.pinned ? 0 : 1 }),
    });
    await fetchSessions();
  } catch (e) {
    console.error("Session pin failed:", e);
  } finally {
    actionLoading.value = false;
  }
}

function isCollapsed(p: ProjectGroupType): boolean {
  const choice = collapseChoice.value.get(projectKey(p));
  return choice === undefined ? !!p.archived : choice;
}

function toggleProject(p: ProjectGroupType) {
  const m = new Map(collapseChoice.value);
  m.set(projectKey(p), !isCollapsed(p));
  collapseChoice.value = m;
}

async function handleDelete(instanceId: string) {
  deleteLoading.value = true;
  try {
    await fetch(
      `/api/delete-session?instanceId=${encodeURIComponent(instanceId)}`,
    );
    await fetchSessions();
    emit("delete-session", instanceId);
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
          :placeholder="$t('chat.searchSessions')"
          autocomplete="off"
        />
        <button
          v-if="searchQuery"
          class="search-clear"
          @click="searchQuery = ''"
          :title="$t('chat.clearSearch')"
        >
          <X :size="12" />
        </button>
      </div>
      <div class="sidebar-actions">
        <button
          class="btn btn-ghost btn-icon btn-sm"
          :title="$t('chat.refreshSessions')"
          @click="fetchSessions"
          :disabled="loading"
        >
          <RefreshCw :size="14" :class="{ spinning: loading }" />
        </button>
        <button
          class="btn btn-ghost btn-icon btn-sm"
          :title="$t('chat.newSession')"
          @click="handleNewSession"
        >
          <Plus :size="16" />
        </button>
      </div>
    </div>

    <!-- Session list -->
    <div class="sidebar-sessions">
      <!-- Loading skeleton -->
      <SkeletonList v-if="loading" :rows="6" />

      <!-- Error state -->
      <EmptyState
        v-else-if="error"
        error
        :title="$t('chat.loadErrorTitle')"
        :hint="$t('chat.loadErrorHint')"
      >
        <template #actions>
          <button class="btn btn-ghost btn-sm" @click="fetchSessions">
            {{ $t("common.retry") }}
          </button>
        </template>
      </EmptyState>

      <!-- Empty state -->
      <EmptyState
        v-else-if="projects.length === 0"
        :title="$t('chat.noSessionsTitle')"
        :hint="$t('chat.noSessionsHint')"
      >
        <template #icon>
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
        </template>
      </EmptyState>

      <!-- Project groups -->
      <template v-else>
        <ProjectGroup
          v-for="project in normalProjects"
          :key="project.path"
          :project="project"
          :collapsed="isCollapsed(project)"
          :active-session-id="activeSessionId"
          :session-status="sessionStatus ?? null"
          :archive-confirm="archiveConfirm === project.id"
          :action-loading="actionLoading"
          :delete-confirm-id="showDeleteConfirm"
          :delete-loading="deleteLoading"
          @toggle="toggleProject(project)"
          @pin="togglePin(project)"
          @archive="confirmArchive(project)"
          @confirm-archive="doArchive"
          @cancel-archive="archiveConfirm = null"
          @restore="project.id && restoreProject(project.id)"
          @new-session="emit('new-session', project.path, project.name)"
          @select-session="emit('select-session', $event)"
          @pin-session="toggleSessionPin($event)"
          @request-delete="showDeleteConfirm = $event ?? null"
          @confirm-delete="handleDelete($event)"
          @cancel-delete="showDeleteConfirm = null"
          @renamed="fetchSessions"
        />

        <!-- Archived projects stay visible under an "Archive" section -->
        <template v-if="archivedFiltered.length > 0">
          <div class="archived-section-title">{{ $t("chat.archiveSection") }}</div>
          <ProjectGroup
            v-for="project in archivedFiltered"
            :key="'arch-' + project.path"
            :project="project"
            :collapsed="isCollapsed(project)"
            :active-session-id="activeSessionId"
            :session-status="sessionStatus ?? null"
            :archive-confirm="false"
            :action-loading="actionLoading"
            :delete-confirm-id="showDeleteConfirm"
            :delete-loading="deleteLoading"
            @toggle="toggleProject(project)"
            @restore="project.id && restoreProject(project.id)"
            @new-session="emit('new-session', project.path, project.name)"
            @select-session="emit('select-session', $event)"
            @pin-session="toggleSessionPin($event)"
            @request-delete="showDeleteConfirm = $event ?? null"
            @confirm-delete="handleDelete($event)"
            @cancel-delete="showDeleteConfirm = null"
            @renamed="fetchSessions"
          />
        </template>
      </template>
    </div>
  </div>
</template>

<style scoped>
.sidebar-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border);
}

.sidebar-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 10px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.sidebar-search-wrap {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  min-width: 0;
}

.search-icon {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.sidebar-search-input {
  flex: 1;
  border: none;
  background: none;
  outline: none;
  color: var(--text);
  font-size: 12px;
  min-width: 0;
}

.sidebar-search-input::placeholder {
  color: var(--text-tertiary);
}

.search-clear {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
}

.search-clear:hover {
  color: var(--text);
  background: var(--bg-hover);
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

/* Archive section */
.archived-section-title {
  padding: 10px 10px 4px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-tertiary);
  border-top: 1px solid var(--border);
  margin-top: 4px;
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
}
</style>
