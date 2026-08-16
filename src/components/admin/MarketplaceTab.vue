<script setup lang="ts">
import { watch } from "vue";
import {
  Search, Package, Globe,
  Loader2, RefreshCw, ChevronLeft, ChevronRight,
} from "lucide-vue-next";
import { EmptyState } from "@piter/ui";
import MarketPackageCard from "./MarketPackageCard.vue";
import { useMarketplace } from "../../composables/useMarketplace";

// ─── Marketplace Tab ────────────────────────────────────────────────────
// 数据/过滤/分页/安装动作在 useMarketplace；卡片展示在 MarketPackageCard。
// 本组件只做模板组装（工具栏 + 网格 + 分页）。

const props = defineProps<{
  packages: Array<unknown>;
}>();

const emit = defineEmits<{
  (e: "packages-changed", packages: string[]): void;
}>();

// 取包的 source：字符串元素直接用；过滤对象元素取 .source 字段。
function packageSource(entry: unknown): string | null {
  if (typeof entry === "string") return entry;
  if (entry && typeof entry === "object") {
    const source = (entry as { source?: unknown }).source;
    if (typeof source === "string") return source;
  }
  return null;
}

const {
  loading, error, loaded, activeType,
  installedOnly, sortMode, currentPage,
  installingPkg, installError,
  filteredPackages, pagedPackages,
  totalPages, pageNumbers,
  searchInput, onSearchInput,
  typeFilters, sortOptions,
  loadAll, isInstalled,
  setType, setPage,
  handleInstall, handleUninstall, openLink,
} = useMarketplace({
  fallbackInstalled: () =>
    props.packages
      .map(packageSource)
      .filter((s): s is string => s !== null)
      .map((p) => p.replace(/^npm:/, "")),
  onPackagesChanged: (installed) => emit("packages-changed", installed),
});

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
        <h3 class="mp-title">{{ $t("admin.marketTitle") }}</h3>
        <p class="mp-desc">{{ $t("admin.marketDesc") }}</p>
      </div>
      <button
        v-if="loaded"
        class="btn btn-sm"
        :disabled="loading"
        @click="loadAll"
      >
        <RefreshCw :size="12" :class="{ spin: loading }" />
        {{ loading ? $t("common.loading") : $t("admin.refresh") }}
      </button>
    </div>

    <!-- Network notice: only shown when a load/install has failed -->
    <div v-if="error || installError" class="mp-proxy-note">
      <Globe :size="14" class="mp-proxy-note-icon" />
      <i18n-t keypath="admin.marketNetworkNote" tag="span">
        <template #code><code>npm config set proxy ...</code></template>
      </i18n-t>
    </div>

    <!-- Loading state -->
    <div v-if="loading && !loaded" class="mp-loading">
      <Loader2 :size="20" class="spin" />
      <span>{{ $t("admin.loadingPackages") }}</span>
    </div>

    <!-- Error state -->
    <div v-else-if="error && !loaded" class="mp-error">
      <p>{{ error }}</p>
      <button class="btn btn-sm" @click="loadAll">{{ $t("common.retry") }}</button>
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
            :placeholder="$t('admin.searchPackages')"
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
            {{ $t(f.labelKey) }}
          </button>
        </div>

        <!-- Bottom row: installed toggle + sort + count -->
        <div class="mp-toolbar-row">
          <label class="mp-installed-toggle">
            <input type="checkbox" v-model="installedOnly" />
            <span>{{ $t("admin.installedOnly") }}</span>
          </label>

          <select class="mp-sort" v-model="sortMode">
            <option v-for="s in sortOptions" :key="s.value" :value="s.value">
              {{ $t(s.labelKey) }}
            </option>
          </select>

          <span class="mp-count">
            {{ $t("admin.packageCount", { n: filteredPackages.length }) }}
          </span>
        </div>
      </div>

      <!-- Install error banner -->
      <div v-if="installError" class="mp-install-error">
        {{ installError }}
      </div>

      <!-- Package grid -->
      <EmptyState v-if="pagedPackages.length === 0" :title="$t('admin.noPackages')" :hint="$t('admin.noPackagesHint')">
        <template #icon><Package :size="32" /></template>
      </EmptyState>

      <div v-else class="mp-grid">
        <MarketPackageCard
          v-for="pkg in pagedPackages"
          :key="pkg.name"
          :pkg="pkg"
          :installed="isInstalled(pkg)"
          :installing="installingPkg === pkg.name"
          @install="handleInstall(pkg)"
          @uninstall="handleUninstall(pkg)"
          @open-link="openLink"
        />
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

/* Proxy notice */
.mp-proxy-note {
  display: flex;
  align-items: flex-start;
  gap: var(--space-sm);
  padding: var(--space-sm) var(--space-md);
  background: var(--warning-soft);
  color: var(--warning);
  border-radius: var(--radius-md);
  font-size: var(--font-size-caption);
  line-height: var(--line-height-caption);
}

.mp-proxy-note-icon {
  flex-shrink: 0;
  margin-top: 1px;
}

.mp-proxy-note code {
  font-family: var(--font-mono);
  background: transparent;
  color: inherit;
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
