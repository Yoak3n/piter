<script setup lang="ts">
import { Download, ExternalLink, Loader2 } from "lucide-vue-next";
import type { MarketPackage } from "../../composables/useMarketplace";

// ─── 市场包卡片（MarketplaceTab 子组件）───
// 纯展示：名称/badge/描述/meta/外链 + 安装/卸载按钮。
// 格式化（下载数缩写/类型 badge/主类型）在此；安装动作 emit 给父级。

defineProps<{
  pkg: MarketPackage;
  installed: boolean;
  installing: boolean;
}>();

const emit = defineEmits<{
  (e: "install"): void;
  (e: "uninstall"): void;
  (e: "open-link", url: string): void;
}>();

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
</script>

<template>
  <div class="mp-card">
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
          :title="$t('admin.linkNpm')"
          @click="emit('open-link', pkg.links.npm!)"
        >
          <ExternalLink :size="11" />
          npm
        </button>
        <button
          v-if="pkg.links?.repository"
          class="mp-link"
          :title="$t('admin.linkRepo')"
          @click="emit('open-link', pkg.links.repository!)"
        >
          <ExternalLink :size="11" />
          repo
        </button>
        <button
          v-if="pkg.links?.homepage"
          class="mp-link"
          :title="$t('admin.linkHome')"
          @click="emit('open-link', pkg.links.homepage!)"
        >
          <ExternalLink :size="11" />
          home
        </button>
      </div>

      <button
        v-if="installed"
        class="btn btn-sm btn-danger"
        :disabled="installing"
        @click="emit('uninstall')"
      >
        <Loader2 v-if="installing" :size="12" class="spin" />
        {{ $t("admin.uninstall") }}
      </button>
      <button
        v-else
        class="btn btn-sm btn-primary"
        :disabled="installing"
        @click="emit('install')"
      >
        <Loader2 v-if="installing" :size="12" class="spin" />
        <Download v-else :size="12" />
        {{ $t("admin.install") }}
      </button>
    </div>
  </div>
</template>

<style scoped>
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

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
