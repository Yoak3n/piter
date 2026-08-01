import { ref, computed } from "vue";

const API_BASE = "https://pi-packages-api.shixin.workers.dev";
const PAGE_SIZE = 250; // fetch page size from remote
const DISPLAY_PAGE_SIZE = 24; // items per page in UI

export interface MarketPackage {
  name: string;
  description: string;
  author: string;
  types: string[];
  downloads: number;
  updatedAt: string;
  links?: {
    npm?: string;
    repository?: string;
    homepage?: string;
  };
}

export type PackageType = "all" | "extension" | "skill" | "theme" | "prompt";
export type SortMode = "downloads" | "name" | "updated";

export function useMarketplace() {
  const allPackages = ref<MarketPackage[]>([]);
  const installedSources = ref<Set<string>>(new Set());
  const loading = ref(false);
  const error = ref("");
  const loaded = ref(false);

  // Filters
  const activeType = ref<PackageType>("all");
  const searchQuery = ref("");
  const installedOnly = ref(false);
  const sortMode = ref<SortMode>("downloads");
  const currentPage = ref(1);

  // Install tracking
  const installingPkg = ref<string | null>(null);
  const installError = ref<string | null>(null);

  async function fetchAllPackages(): Promise<MarketPackage[]> {
    const results: MarketPackage[] = [];
    let page = 1;
    while (true) {
      const url = `${API_BASE}/packages?page=${page}&pageSize=${PAGE_SIZE}`;
      const res = await fetch(url);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      const items: MarketPackage[] = Array.isArray(data) ? data : data.packages ?? data.data ?? [];
      if (items.length === 0) break;
      results.push(...items);
      if (items.length < PAGE_SIZE) break;
      page++;
    }
    return results;
  }

  async function loadPackages(installedPkgs: string[]) {
    loading.value = true;
    error.value = "";
    try {
      allPackages.value = await fetchAllPackages();
      installedSources.value = new Set(installedPkgs.map(p => p.replace(/^npm:/, "")));
      loaded.value = true;
      currentPage.value = 1;
    } catch (e) {
      error.value = `Failed to load packages: ${e}`;
    } finally {
      loading.value = false;
    }
  }

  const filteredPackages = computed(() => {
    let list = allPackages.value;

    // Type filter
    if (activeType.value !== "all") {
      list = list.filter(p => p.types?.includes(activeType.value));
    }

    // Search filter
    const q = searchQuery.value.trim().toLowerCase();
    if (q) {
      list = list.filter(p =>
        p.name.toLowerCase().includes(q) ||
        p.description?.toLowerCase().includes(q) ||
        p.author?.toLowerCase().includes(q)
      );
    }

    // Installed-only filter
    if (installedOnly.value) {
      list = list.filter(p => installedSources.value.has(p.name));
    }

    return list;
  });

  const sortedPackages = computed(() => {
    const list = [...filteredPackages.value];
    switch (sortMode.value) {
      case "downloads":
        list.sort((a, b) => (b.downloads ?? 0) - (a.downloads ?? 0));
        break;
      case "name":
        list.sort((a, b) => a.name.localeCompare(b.name));
        break;
      case "updated":
        list.sort((a, b) => new Date(b.updatedAt ?? 0).getTime() - new Date(a.updatedAt ?? 0).getTime());
        break;
    }
    return list;
  });

  const totalPages = computed(() =>
    Math.max(1, Math.ceil(sortedPackages.value.length / DISPLAY_PAGE_SIZE))
  );

  const pagedPackages = computed(() => {
    const start = (currentPage.value - 1) * DISPLAY_PAGE_SIZE;
    return sortedPackages.value.slice(start, start + DISPLAY_PAGE_SIZE);
  });

  const pageNumbers = computed(() => {
    const total = totalPages.value;
    const cur = currentPage.value;
    const pages: (number | "...")[] = [];
    if (total <= 7) {
      for (let i = 1; i <= total; i++) pages.push(i);
    } else {
      pages.push(1);
      if (cur > 3) pages.push("...");
      for (let i = Math.max(2, cur - 1); i <= Math.min(total - 1, cur + 1); i++) {
        pages.push(i);
      }
      if (cur < total - 2) pages.push("...");
      pages.push(total);
    }
    return pages;
  });

  function isInstalled(pkg: MarketPackage): boolean {
    return installedSources.value.has(pkg.name);
  }

  function setInstalled(name: string, installed: boolean) {
    if (installed) {
      installedSources.value.add(name);
    } else {
      installedSources.value.delete(name);
    }
  }

  function setType(t: PackageType) {
    activeType.value = t;
    currentPage.value = 1;
  }

  function setPage(p: number) {
    if (p >= 1 && p <= totalPages.value) {
      currentPage.value = p;
    }
  }

  function setSearch(q: string) {
    searchQuery.value = q;
    currentPage.value = 1;
  }

  return {
    allPackages,
    installedSources,
    loading,
    error,
    loaded,
    activeType,
    searchQuery,
    installedOnly,
    sortMode,
    currentPage,
    installingPkg,
    installError,
    filteredPackages,
    sortedPackages,
    pagedPackages,
    totalPages,
    pageNumbers,
    loadPackages,
    isInstalled,
    setInstalled,
    setType,
    setPage,
    setSearch,
  };
}
