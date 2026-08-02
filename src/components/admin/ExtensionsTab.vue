<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import {
  Globe, FolderKanban, Loader2, RefreshCw, Inbox, Puzzle,
} from "lucide-vue-next";
import {
  useAdmin,
  type ExtensionOverview,
  type ExtensionEntry,
} from "../../composables/useAdmin";

const {
  fetchExtensionOverview,
  fetchProjectExtensionOverview,
  saveGlobalExtensions,
  saveProjectAddedExtensions,
  saveProjectExcludedExtensions,
} = useAdmin();

const overview = ref<ExtensionOverview | null>(null);
const loading = ref(false);
const selectedProjectId = ref("");

// Mutable sets (preserve extra DB entries not shown as toggles)
const globalEnabled = ref<Set<string>>(new Set());
// project_added_extensions — the "added on top of global" list per project.
const projectEnabled = ref<Map<string, Set<string>>>(new Map());
// project_excluded_extensions — extensions explicitly disabled per project.
const projectExcluded = ref<Map<string, Set<string>>>(new Map());

// Per-project candidates, loaded lazily when a project is selected (the disk
// scan is the slow part). Cached so switching back is instant.
const projectCandidates = ref<Map<string, ExtensionEntry[]>>(new Map());
const loadingProject = ref(false);

const selectedProject = computed(() =>
  overview.value?.projects.find((p) => p.id === selectedProjectId.value)
);

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function loadOverview(preserveState = false) {
  loading.value = true;
  overview.value = await fetchExtensionOverview();
  // Background refresh events re-render candidates but must not clobber the
  // sets the user is currently toggling.
  if (!preserveState) {
    globalEnabled.value = new Set(overview.value?.enabled_global ?? []);
    projectEnabled.value = new Map(
      (overview.value?.projects ?? []).map((p) => [p.id, new Set(p.added)])
    );
    projectExcluded.value = new Map(
      (overview.value?.projects ?? []).map((p) => [p.id, new Set(p.excluded)])
    );
  }
  if (!selectedProjectId.value && overview.value?.projects.length) {
    selectedProjectId.value = overview.value.projects[0].id;
  }
  loading.value = false;
}

// Lazy-load candidates for the selected project (cache hit → instant).
async function loadProjectCandidates(pid: string) {
  if (projectCandidates.value.has(pid)) return;
  loadingProject.value = true;
  const detail = await fetchProjectExtensionOverview(pid);
  if (detail) {
    projectCandidates.value.set(pid, detail.extensions);
    projectCandidates.value = new Map(projectCandidates.value);
  }
  loadingProject.value = false;
}

watch(selectedProjectId, (pid) => {
  if (pid) loadProjectCandidates(pid);
});

// The backend rescans in the background (cached snapshot is returned first);
// when the scan differs, it emits `extension_overview_updated` so this tab
// refreshes automatically.
let unlisten: (() => void) | undefined;
onMounted(async () => {
  loadOverview();
  if (isTauri) {
    unlisten = await listen("extension_overview_updated", async () => {
      await loadOverview(true);
      // Disk layout may have changed — drop cached candidates and re-fetch the
      // currently selected project.
      projectCandidates.value = new Map();
      const pid = selectedProjectId.value;
      if (pid) await loadProjectCandidates(pid);
    });
  }
});
onUnmounted(() => unlisten?.());

// Serialize saves: `set_*_extensions` replaces the whole list, so concurrent
// toggles must not race each other. Each queued save reads the latest state.
let saveQueue: Promise<void> = Promise.resolve();
function enqueueSave(fn: () => Promise<void>) {
  saveQueue = saveQueue.then(fn).catch(() => {});
  return saveQueue;
}

async function toggleGlobal(name: string, checked: boolean) {
  const next = new Set(globalEnabled.value);
  if (checked) next.add(name);
  else next.delete(name);
  globalEnabled.value = next;
  enqueueSave(async () => {
    await saveGlobalExtensions([...globalEnabled.value]);
  });
}

// ── Tri-state project extension control ────────────────────────────────────
// Effective model: (global ∪ project_added) − project_excluded.
// Each candidate row shows the two relevant states for its kind:
//   global-backed → 继承全局 (default) ↔ 排除
//   project-only  → 未启用 ↔ 启用
type ExtState = "inherit" | "enabled" | "excluded" | "off";

function projectExtState(name: string): ExtState {
  const pid = selectedProjectId.value;
  if (!pid) return "off";
  if (projectExcluded.value.get(pid)?.has(name)) return "excluded";
  if (globalEnabled.value.has(name)) return "inherit";
  if (projectEnabled.value.get(pid)?.has(name)) return "enabled";
  return "off";
}

function extOptions(name: string): { value: ExtState; label: string }[] {
  return globalEnabled.value.has(name)
    ? [
        { value: "inherit", label: "继承全局" },
        { value: "excluded", label: "排除" },
      ]
    : [
        { value: "off", label: "未启用" },
        { value: "enabled", label: "启用" },
      ];
}

function setProjectExtState(name: string, next: ExtState) {
  const pid = selectedProjectId.value;
  if (!pid) return;
  const enabled = new Set(projectEnabled.value.get(pid) ?? []);
  const excluded = new Set(projectExcluded.value.get(pid) ?? []);
  if (next === "excluded") excluded.add(name);
  else excluded.delete(name);
  if (next === "enabled") enabled.add(name);
  else enabled.delete(name);
  projectEnabled.value.set(pid, enabled);
  projectExcluded.value.set(pid, excluded);
  projectEnabled.value = new Map(projectEnabled.value);
  projectExcluded.value = new Map(projectExcluded.value);
  enqueueSaveProject(pid);
}

function enqueueSaveProject(pid: string) {
  enqueueSave(async () => {
    const enabled = [...(projectEnabled.value.get(pid) ?? new Set<string>())];
    const excluded = [...(projectExcluded.value.get(pid) ?? new Set<string>())];
    await saveProjectAddedExtensions(pid, enabled);
    await saveProjectExcludedExtensions(pid, excluded);
  });
}
</script>

<template>
  <div class="tab-content">
    <div class="tab-header">
      <div class="tab-header-info">
        <h3 class="tab-title">Extensions</h3>
        <p class="tab-desc">
          Enable the extensions Pi uses. Checked extensions are persisted to the
          gateway database. Installing/uninstalling packages is done in the Market tab.
        </p>
      </div>
      <button class="btn btn-sm" :disabled="loading" @click="loadOverview()">
        <RefreshCw :size="12" :class="{ spin: loading }" />
        {{ loading ? "Loading..." : "Refresh" }}
      </button>
    </div>

    <!-- Global extensions -->
    <div class="section-card">
      <div class="section-header">
        <div class="section-title">
          <Globe :size="14" class="section-icon" />
          <span>Global Extensions</span>
        </div>
        <p class="section-desc">Discovered in ~/.pi/agent/extensions/ and from installed packages. Enabled for every Pi session. Only enabled extensions are passed to Pi — auto-discovery is disabled.</p>
      </div>

      <div v-if="loading" class="loading-row">
        <Loader2 :size="12" class="spin" />
        <span>Loading...</span>
      </div>

      <template v-else>
        <div v-if="!overview || overview.global_extensions.length === 0" class="empty-row">
          <Inbox :size="20" class="empty-icon" />
          <span>No extensions or packages found</span>
        </div>

        <div v-else class="ext-list">
          <div v-for="ext in overview.global_extensions" :key="ext.name" class="ext-item">
            <div class="ext-info">
              <Puzzle :size="13" class="ext-icon" />
              <span class="ext-name">{{ ext.name }}</span>
            </div>
            <label class="toggle" :class="{ on: globalEnabled.has(ext.name) }">
              <input
                type="checkbox"
                :checked="globalEnabled.has(ext.name)"
                @change="toggleGlobal(ext.name, ($event.target as HTMLInputElement).checked)"
              />
              <span class="toggle-track"></span>
            </label>
          </div>
        </div>
      </template>
    </div>

    <!-- Project extensions -->
    <div class="section-card">
      <div class="section-header">
        <div class="section-title">
          <FolderKanban :size="14" class="section-icon" />
          <span>Project Extensions</span>
        </div>
        <p class="section-desc">
          Based on the global list: global-enabled extensions are inherited by
          default and can be excluded for this project; project-local/package
          extensions can be enabled per project.
        </p>
      </div>

      <div v-if="loading" class="loading-row">
        <Loader2 :size="12" class="spin" />
        <span>Loading...</span>
      </div>

      <template v-else>
        <div v-if="!overview || overview.projects.length === 0" class="empty-row">
          <Inbox :size="20" class="empty-icon" />
          <span>No projects found</span>
        </div>

        <template v-else>
          <select
            class="input project-select"
            :value="selectedProjectId"
            @change="selectedProjectId = ($event.target as HTMLSelectElement).value"
          >
            <option v-for="p in overview.projects" :key="p.id" :value="p.id">
              {{ p.name }} — {{ p.cwd }}
            </option>
          </select>

          <div v-if="selectedProject && (loadingProject || !projectCandidates.get(selectedProject.id))" class="loading-row">
            <Loader2 :size="12" class="spin" />
            <span>Loading project extensions...</span>
          </div>

          <div v-else-if="selectedProject && (projectCandidates.get(selectedProject.id) ?? []).length === 0" class="empty-row">
            <Inbox :size="20" class="empty-icon" />
            <span>No extensions or packages found for this project</span>
          </div>

          <div v-else-if="selectedProject" class="ext-list">
            <div
              v-for="ext in projectCandidates.get(selectedProject.id) ?? []"
              :key="ext.name"
              class="ext-item"
            >
              <div class="ext-info">
                <Puzzle :size="13" class="ext-icon" />
                <span class="ext-name">{{ ext.name }}</span>
              </div>
              <div class="ext-seg">
                <button
                  v-for="opt in extOptions(ext.name)"
                  :key="opt.value"
                  class="seg-btn"
                  :class="{ active: projectExtState(ext.name) === opt.value }"
                  @click="setProjectExtState(ext.name, opt.value)"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>
          </div>
        </template>
      </template>
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

.tab-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
}

.tab-header-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0;
}

.tab-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
  line-height: var(--line-height-caption);
}

.section-card {
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.section-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.section-title {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: var(--font-size-body);
  font-weight: 600;
  color: var(--text);
}

.section-icon {
  color: var(--accent);
}

.section-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
  line-height: var(--line-height-caption);
}

.loading-row,
.empty-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
  padding: var(--space-sm) 0;
}

.empty-icon {
  opacity: 0.4;
  flex-shrink: 0;
}

.ext-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  background: var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  border: 1px solid var(--border);
}

.ext-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-xs) var(--space-md);
  background: var(--bg-panel);
  transition: background var(--duration-fast) var(--ease);
}
.ext-item:hover {
  background: var(--bg-hover);
}

.ext-info {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  min-width: 0;
}

.ext-icon {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.ext-name {
  font-family: var(--font-mono);
  font-size: var(--font-size-caption);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-select {
  font-size: var(--font-size-caption);
}

.ext-seg {
  display: inline-flex;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  overflow: hidden;
  flex-shrink: 0;
}

.seg-btn {
  border: none;
  background: var(--bg-panel);
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
  padding: 2px 10px;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}
.seg-btn + .seg-btn {
  border-left: 1px solid var(--border);
}
.seg-btn:hover {
  background: var(--bg-hover);
  color: var(--text);
}
.seg-btn.active {
  background: var(--accent);
  color: var(--bg-panel);
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
