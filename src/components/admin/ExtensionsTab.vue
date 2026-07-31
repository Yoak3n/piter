<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import {
  Globe, FolderKanban, Loader2, RefreshCw, Inbox, Puzzle,
} from "lucide-vue-next";
import {
  useAdmin,
  type ExtensionOverview,
} from "../../composables/useAdmin";

const { fetchExtensionOverview, saveGlobalExtensions, saveProjectExtensions } = useAdmin();

const overview = ref<ExtensionOverview | null>(null);
const loading = ref(false);
const selectedProjectId = ref("");

// Mutable enabled sets (preserve extra DB entries not shown as toggles)
const globalEnabled = ref<Set<string>>(new Set());
const projectEnabled = ref<Map<string, Set<string>>>(new Map());

const selectedProject = computed(() =>
  overview.value?.projects.find((p) => p.id === selectedProjectId.value)
);

async function loadOverview() {
  loading.value = true;
  overview.value = await fetchExtensionOverview();
  globalEnabled.value = new Set(overview.value?.enabled_global ?? []);
  projectEnabled.value = new Map(
    (overview.value?.projects ?? []).map((p) => [p.id, new Set(p.enabled)])
  );
  if (!selectedProjectId.value && overview.value?.projects.length) {
    selectedProjectId.value = overview.value.projects[0].id;
  }
  loading.value = false;
}

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

async function toggleProject(name: string, checked: boolean) {
  const projectId = selectedProjectId.value;
  if (!projectId) return;
  const next = new Set(projectEnabled.value.get(projectId) ?? []);
  if (checked) next.add(name);
  else next.delete(name);
  projectEnabled.value.set(projectId, next);
  projectEnabled.value = new Map(projectEnabled.value);
  enqueueSave(async () => {
    const current = projectEnabled.value.get(projectId) ?? new Set<string>();
    await saveProjectExtensions(projectId, [...current]);
  });
}

function isProjectEnabled(name: string): boolean {
  return projectEnabled.value.get(selectedProjectId.value)?.has(name) ?? false;
}

onMounted(loadOverview);
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
      <button class="btn btn-sm" :disabled="loading" @click="loadOverview">
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
        <p class="section-desc">Discovered in ~/.pi/agent/extensions/ and from installed packages. Enabled for every Pi session.</p>
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
        <p class="section-desc">Includes globally available and project-local extensions. Enabled only for this project.</p>
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

          <div v-if="selectedProject && selectedProject.extensions.length === 0" class="empty-row">
            <Inbox :size="20" class="empty-icon" />
            <span>No extensions or packages found for this project</span>
          </div>

          <div v-else-if="selectedProject" class="ext-list">
            <div v-for="ext in selectedProject.extensions" :key="ext.name" class="ext-item">
              <div class="ext-info">
                <Puzzle :size="13" class="ext-icon" />
                <span class="ext-name">{{ ext.name }}</span>
              </div>
              <label class="toggle" :class="{ on: isProjectEnabled(ext.name) }">
                <input
                  type="checkbox"
                  :checked="isProjectEnabled(ext.name)"
                  @change="toggleProject(ext.name, ($event.target as HTMLInputElement).checked)"
                />
                <span class="toggle-track"></span>
              </label>
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
  max-width: 560px;
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

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
