<script setup lang="ts">
import { ref, watch } from "vue";
import {
  Store, Search, Package, Download, ExternalLink,
  Loader2, RefreshCw, ChevronLeft, ChevronRight,
  Puzzle, Paintbrush, MessageSquare, Zap,
} from "lucide-vue-next";
import { invoke } from "@tauri-apps/api/core";
import {
  useMarketplace,
  type MarketPackage, type PackageType, type SortMode,
} from "../../composables/useMarketplace";

const props = defineProps<{
  packages: string[];
}>();

const emit = defineEmits<{
  (e: "packages-changed", packages: string[]): void;
}>();

const {
  loading, error, loaded, activeType,
  installedOnly, sortMode, currentPage,
  installingPkg, installError,
  filteredPackages, pagedPackages,
  totalPages, pageNumbers,
  loadPackages, isInstalled, setInstalled,
  setType, setPage, setSearch,
} = useMarketplace();

const searchInput = ref("");
let searchTimer: ReturnType<typeof setTimeout> | null = null;

function onSearchInput(e: Event) {
  const val = (e.target as HTMLInputElement).value;
  searchInput.value = val;
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => setSearch(val), 180);
}

const typeFilters: { key: PackageType; label: string; icon: any }[] = [
  { key: "all", label: "All", icon: Store },
  { key: "extension", label: "Extensions", icon: Puzzle },
  { key: "skill", label: "Skills", icon: Zap },
  { key: "theme", label: "Themes", icon: Paintbrush },
  { key: "prompt", label: "Prompts", icon: MessageSquare },
];

const sortOptions: { value: SortMode; label: string }[] = [
  { value: "downloads", label: "Most Downloads" },
  { value: "name", label: "Name A-Z" },
  { value: "updated", label: "Recently Updated" },
];

function fmtDownloads(n: number): string {
  if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return String(n);
}

function badgeClass(pkg: MarketPackage): string {
  if (pkg.types?.includes("extension")) return "badge-accent";
  if (pkg.types?.includes("skill")) return "badge-success";
  return "badge-muted";
}

function primaryType(pkg: MarketPackage): string {
  if (pkg.types?.length) return pkg.types[0];
  return "package";
}

async function fetchInstalled(): Promise<string[]> {
  try {
    return await invoke<string[]>("list_pi_packages");
  } catch {
    return props.packages.map((p) => p.replace(/^npm:/, ""));
  }
}

async function loadAll() {
  const installed = await fetchInstalled();
  await loadPackages(installed);
}

async function handleInstall(pkg: MarketPackage) {
  installingPkg.value = pkg.name;
  installError.value = null;
  try {
    await invoke("install_pi_package", { source: `npm:${pkg.name}` });
    const installed = await fetchInstalled();
    setInstalled(pkg.name, true);
    emit("packages-changed", installed);
  } catch (e) {
    installError.value = `Failed to install ${pkg.name}: ${e}`;
  } finally {
    installingPkg.value = null;
  }
}

async function handleUninstall(pkg: MarketPackage) {
  installingPkg.value = pkg.name;
  installError.value = null;
  try {
    await invoke("remove_pi_package", { source: `npm:${pkg.name}` });
    const installed = await fetchInstalled();
    setInstalled(pkg.name, false);
    emit("packages-changed", installed);
  } catch (e) {
    installError.value = `Failed to uninstall ${pkg.name}: ${e}`;
  } finally {
    installingPkg.value = null;
  }
}

async function openLink(url: string) {
  try {
    await invoke("open_path", { path: url });
  } catch {
    window.open(url, "_blank");
  }
}

// Auto-load on mount
watch(() => props.packages, () => {
  if (!loaded.value && !loading.value) {
    loadAll();
  }
}, { immediate: true });
</script>

<template>
  <div class="marketplace">
    <!-- Header -->
    <div class="mp-header">
      <div class="mp-header-info">
        <h3 class="mp-title">Package Marketplace</h3>
        <p class="mp-desc">Browse and install community packages for Pi agent.</p>
      </div>
      <button
        v-if="loaded"
        class="btn btn-sm"
        :disabled="loading"
        @click="loadAll"
      >
        <RefreshCw :size="12" :class="{ spin: loading }" />
        {{ loading ? "Loading..." : "Refresh" }}
      </button>
    </div>

    <!-- Loading state -->
    <div v-if="loading && !loaded" class="mp-loading">
      <Loader2 :size="20" class="spin" />
      <span>Loading packages...</span>
    </div>

    <!-- Error state -->
    <div v-else-if="error && !loaded" class="mp-error">
      <p>{{ error }}</p>
      <button class="btn btn-sm" @click="loadAll">Retry</button>
    </div>

    <!-- Content -->
    <template v-else-if="loaded">
      <!-- Filters toolbar -->
      <div class="mp-toolbar">
        <!-- Search -->
        <div class="mp-search">
          <Search :size="14" class="mp-search-icon" />
          <input
            class="mp-search-input"
            type="text"
            placeholder="Search packages..."
            :value="searchInput"
            @input="onSearchInput"
          />
        </div>

        <!-- Type pills -->
        <div class="mp-pills">
          <button
            v-for="f in typeFilters"
            :key="f.key"
            class="mp-pill"
            :class="{ active: activeType === f.key }"
            @click="setType(f.key)"
          >
            <component :is="f.icon" :size="12" />
            {{ f.label }}
          </button>
        </div>

        <!-- Bottom row: installed toggle + sort + count -->
        <div class="mp-toolbar-row">
          <label class="mp-installed-toggle">
            <input type="checkbox" v-model="installedOnly" />
            <span>Installed only</span>
          </label>

          <select class="mp-sort" v-model="sortMode">
            <option v-for="s in sortOptions" :key="s.value" :value="s.value">
              {{ s.label }}
            </option>
          </select>

          <span class="mp-count">
            {{ filteredPackages.length }} package{{ filteredPackages.length !== 1 ? "s" : "" }}
          </span>
        </div>
      </div>

      <!-- Install error banner -->
      <div v-if="installError" class="mp-install-error">
        {{ installError }}
      </div>

      <!-- Package grid -->
      <div v-if="pagedPackages.length === 0" class="mp-empty">
        <Package :size="32" class="mp-empty-icon" />
        <p>No packages found</p>
        <span class="mp-empty-hint">Try adjusting your filters or search query.</span>
      </div>

      <div v-else class="mp-grid">
        <div v-for="pkg in pagedPackages" :key="pkg.name" class="mp-card">
          <div class="mp-card-header">
            <div class="mp-card-title-row">
              <span class="mp-card-name truncate">{{ pkg.name }}</span>
              <span class="badge" :class="badgeClass(pkg)">{{ primaryType(pkg) }}</span>
            </div>
            <p v-if="pkg.description" class="mp-card-desc truncate">{{ pkg.description }}</p>
          </div>

          <div class="mp-card-meta">
            <span v-if="pkg.author" class="mp-card-author">{{ pkg.author }}</span>
            <span v-if="pkg.downloads" class="mp-card-downloads">
              <Download :size="10" />
              {{ fmtDownloads(pkg.downloads) }}
            </span>
          </div>

          <div class="mp-card-footer">
            <div class="mp-card-links">
              <button
                v-if="pkg.links?.npm"
                class="mp-link"
                title="npm"
                @click="openLink(pkg.links.npm!)"
              >
                <ExternalLink :size="11" />
                npm
              </button>
              <button
                v-if="pkg.links?.repository"
                class="mp-link"
                title="Repository"
                @click="openLink(pkg.links.repository!)"
              >
                <ExternalLink :size="11" />
                repo
              </button>
              <button
                v-if="pkg.links?.homepage"
                class="mp-link"
                title="Homepage"
                @click="openLink(pkg.links.homepage!)"
              >
                <ExternalLink :size="11" />
                home
              </button>
            </div>

            <button
              v-if="isInstalled(pkg)"
              class="btn btn-sm btn-danger"
              :disabled="installingPkg === pkg.name"
              @click="handleUninstall(pkg)"
            >
              <Loader2 v-if="installingPkg === pkg.name" :size="12" class="spin" />
              Uninstall
            </button>
            <button
              v-else
              class="btn btn-sm btn-primary"
              :disabled="installingPkg === pkg.name"
              @click="handleInstall(pkg)"
            >
              <Loader2 v-if="installingPkg === pkg.name" :size="12" class="spin" />
              <Download v-else :size="12" />
              Install
            </button>
          </div>
        </div>
      </div>

      <!-- Pagination -->
      <div v-if="totalPages > 1" class="mp-pagination">
        <button
          class="btn btn-sm btn-icon"
          :disabled="currentPage <= 1"
          @click="setPage(currentPage - 1)"
        >
          <ChevronLeft :size="14" />
        </button>

        <template v-for="(p, i) in pageNumbers" :key="i">
          <span v-if="p === '...'" class="mp-page-ellipsis">...</span>
          <button
            v-else
            class="btn btn-sm mp-page-btn"
            :class="{ 'btn-primary': p === currentPage }"
            @click="setPage(p as number)"
          >
            {{ p }}
          </button>
        </template>

        <button
          class="btn btn-sm btn-icon"
          :disabled="currentPage >= totalPages"
          @click="setPage(currentPage + 1)"
        >
          <ChevronRight :size="14" />
        </button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.marketplace {
  padding: var(--space-xl);
  display: flex;
  flex-direction: column;
  gap: var(--space-lg);
}

/* Header */
.mp-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
}

.mp-header-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.mp-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0;
}

.mp-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
}

/* Loading */
.mp-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-sm);
  padding: var(--space-xxl) 0;
  color: var(--text-tertiary);
  font-size: var(--font-size-body);
}

/* Error */
.mp-error {
  text-align: center;
  padding: var(--space-xxl) 0;
  color: var(--text-secondary);
}
.mp-error p {
  margin: 0 0 var(--space-md) 0;
}

/* Toolbar */
.mp-toolbar {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.mp-search {
  position: relative;
  display: flex;
  align-items: center;
}

.mp-search-icon {
  position: absolute;
  left: 10px;
  color: var(--text-tertiary);
  pointer-events: none;
}

.mp-search-input {
  width: 100%;
  height: 32px;
  padding: 0 12px 0 32px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
  color: var(--text);
  font-size: 13px;
  outline: none;
  transition: border-color var(--duration) var(--ease), box-shadow var(--duration) var(--ease);
}
.mp-search-input:hover {
  border-color: var(--border-hover);
}
.mp-search-input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

/* Pills */
.mp-pills {
  display: flex;
  gap: var(--space-xs);
  flex-wrap: wrap;
}

.mp-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-pill);
  background: var(--bg-panel);
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease);
  white-space: nowrap;
}
.mp-pill:hover {
  background: var(--bg-hover);
  border-color: var(--border-hover);
  color: var(--text);
}
.mp-pill.active {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--accent-strong);
}

/* Toolbar row */
.mp-toolbar-row {
  display: flex;
  align-items: center;
  gap: var(--space-md);
}

.mp-installed-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
}
.mp-installed-toggle input {
  accent-color: var(--accent);
}

.mp-sort {
  height: 26px;
  padding: 0 24px 0 8px;
  font-size: 11px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-panel);
  color: var(--text-secondary);
  cursor: pointer;
  outline: none;
}
.mp-sort:focus {
  border-color: var(--accent);
}

.mp-count {
  margin-left: auto;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

/* Install error */
.mp-install-error {
  padding: var(--space-sm) var(--space-md);
  background: var(--danger-soft);
  color: var(--danger);
  border-radius: var(--radius-sm);
  font-size: var(--font-size-caption);
}

/* Empty */
.mp-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-sm);
  color: var(--text-tertiary);
  padding: var(--space-xxl) 0;
  text-align: center;
}
.mp-empty-icon {
  opacity: 0.4;
}
.mp-empty p {
  margin: 0;
  font-size: var(--font-size-body);
  color: var(--text-secondary);
}
.mp-empty-hint {
  font-size: var(--font-size-caption);
}

/* Grid */
.mp-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-sm);
}
@media (max-width: 720px) {
  .mp-grid {
    grid-template-columns: 1fr;
  }
}

/* Card */
.mp-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
  padding: var(--space-md);
  background: var(--bg-muted);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  transition: border-color var(--duration-fast) var(--ease), background var(--duration-fast) var(--ease);
}
.mp-card:hover {
  border-color: var(--border-hover);
  background: var(--bg-hover);
}

.mp-card-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-height: 0;
}

.mp-card-title-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  min-width: 0;
}

.mp-card-name {
  font-family: var(--font-mono);
  font-size: var(--font-size-control);
  font-weight: 600;
  color: var(--text);
  min-width: 0;
}

.mp-card-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
  line-height: var(--line-height-caption);
}

.mp-card-meta {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

.mp-card-author {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mp-card-downloads {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  flex-shrink: 0;
}

.mp-card-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: auto;
  padding-top: var(--space-xs);
}

.mp-card-links {
  display: flex;
  gap: var(--space-xs);
}

.mp-link {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  font-size: 10px;
  color: var(--text-tertiary);
  background: transparent;
  border: none;
  cursor: pointer;
  transition: color var(--duration-fast) var(--ease), background var(--duration-fast) var(--ease);
}
.mp-link:hover {
  color: var(--accent);
  background: var(--accent-soft);
}

/* Pagination */
.mp-pagination {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-xs);
  padding: var(--space-md) 0;
}

.mp-page-ellipsis {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  padding: 0 4px;
}

.mp-page-btn {
  min-width: 28px;
  padding: 0 6px;
}

/* Spinner */
.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
