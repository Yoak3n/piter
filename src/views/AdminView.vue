<script setup lang="ts">
import { ref, onMounted, computed, watch, defineAsyncComponent } from "vue";
import { TitleBar, setLocale } from "@piter/ui";
import { useAdmin } from "../composables/useAdmin";
import { i18n } from "../i18n";
import type { AppSettings, PiSettings } from "../composables/useAdmin";
import { applyTheme, darkMedia } from "../utils/theme";
import AdminNav from "../components/admin/AdminNav.vue";
import StatusTab from "../components/admin/StatusTab.vue";

// Non-default tabs load lazily so the admin entry stays light (dev module
// graph + production main bundle). UsageTab additionally pulls in echarts.
const PiConfigTab = defineAsyncComponent(() => import("../components/admin/PiConfigTab.vue"));
const ShareTab = defineAsyncComponent(() => import("../components/admin/ShareTab.vue"));
const ProvidersTab = defineAsyncComponent(() => import("../components/admin/ProvidersTab.vue"));
const PiVersionsTab = defineAsyncComponent(() => import("../components/admin/PiVersionsTab.vue"));
const ExtensionsTab = defineAsyncComponent(() => import("../components/admin/ExtensionsTab.vue"));
const MarketplaceTab = defineAsyncComponent(() => import("../components/admin/MarketplaceTab.vue"));
const AppearanceTab = defineAsyncComponent(() => import("../components/admin/AppearanceTab.vue"));
const UsageTab = defineAsyncComponent(() => import("../components/admin/UsageTab.vue"));

const { config, status, piSettings, piInstall, downloadProgress, loading, error,
  fetchConfig, saveConfig, fetchStatus, restartPi, stopPi,
  fetchPiAgentSettings, savePiAgentSettings, openPath,
  fetchPiInstallInfo, downloadPiVersion, uninstallPi,
} = useAdmin();

const activeTab = ref("status");

const appSettings = computed<AppSettings>(() =>
  config.value?.app ?? { theme: "system", language: "system", auto_start: true, start_minimized: true }
);

// Theme currently in effect. Normally follows the saved config, but the
// Appearance tab may preview an unsaved selection — system theme changes must
// not override that preview.
const activeTheme = ref(appSettings.value.theme);

function syncTheme() {
  applyTheme(activeTheme.value);
}

onMounted(() => {
  // Apply the saved theme once config loads (fall back to OS preference
  // until then to avoid a flash of the wrong scheme).
  syncTheme();
  darkMedia.addEventListener("change", syncTheme);
  fetchConfig();
  fetchStatus();
  fetchPiAgentSettings();
  fetchPiInstallInfo();
});

// Follow the saved config, but only when nothing is being previewed.
watch(appSettings, (s) => {
  activeTheme.value = s.theme;
  syncTheme();
  setLocale(i18n, s.language);
});

// Appearance tab previews an unsaved theme selection.
function handleThemePreview(theme: string) {
  activeTheme.value = theme;
}

function handleTabSelect(tab: string) {
  activeTab.value = tab;
  if (tab === "status") fetchStatus();
  if (tab === "share") fetchStatus(); // keep gateway base URL fresh for /api calls
  if (tab === "versions") fetchPiInstallInfo();
}

const piSettingsVal = computed<PiSettings>(() =>
  config.value?.pi ?? { request_timeout_secs: 300, auto_restart_on_crash: true }
);

const piMissing = computed(() => status.value?.pi_binary_missing ?? false);

// Chat view is reachable when the gateway is actually serving (pi available).
const chatAvailable = computed(() => !!status.value?.broker_http_url);

function handleAppUpdate(settings: AppSettings) {
  if (!config.value) return;
  saveConfig({ ...config.value, app: settings });
}

function handlePiUpdate(settings: PiSettings) {
  if (!config.value) return;
  saveConfig({ ...config.value, pi: settings });
}

function handlePackagesChanged(installed: string[]) {
  if (!piSettings.value) return;
  // 保留原有条目（含过滤对象），只剔除已卸载的、追加新安装的，
  // 避免从 marketplace 保存时把用户的过滤配置冲掉。
  const installedSet = new Set(installed);
  const merged: unknown[] = piSettings.value.packages.filter((entry) => {
    const source = typeof entry === "string" ? entry : (entry as { source?: unknown }).source;
    return typeof source === "string" && installedSet.has(source);
  });
  const known = new Set(merged.map((entry) =>
    typeof entry === "string" ? entry : (entry as { source?: unknown }).source
  ));
  for (const src of installed) {
    if (!known.has(src)) merged.push(src);
  }
  savePiAgentSettings({ ...piSettings.value, packages: merged });
}
</script>

<template>
  <div class="admin-view">
    <TitleBar>
      <template #left>
        <span class="admin-title">Piter</span>
      </template>
    </TitleBar>

    <div class="admin-body">
      <AdminNav :activeTab="activeTab" :chatAvailable="chatAvailable" @select="handleTabSelect" />
      <main class="admin-main">
      <div v-if="error" class="admin-error">
        <strong>{{ $t("admin.errorPrefix") }}</strong> {{ error }}
      </div>

      <div v-if="piMissing" class="admin-banner">
        <strong>{{ $t("admin.piMissingTitle") }}</strong>
        <span>
          <i18n-t keypath="admin.piMissingDesc">
            <template #link>
              <a href="#" @click.prevent="activeTab = 'versions'">{{ $t("admin.versions") }}</a>
            </template>
          </i18n-t>
        </span>
      </div>

      <StatusTab
        v-if="activeTab === 'status'"
        :status="status"
        :loading="loading.status"
        :pi-settings="piSettingsVal"
        :disabled="loading.config"
        @refresh="fetchStatus"
        @restart-pi="restartPi"
        @stop-pi="stopPi"
        @open-path="openPath"
        @update-pi-settings="handlePiUpdate"
      />

      <UsageTab v-if="activeTab === 'usage'" :broker-http-url="status?.broker_http_url ?? ''" />

      <ShareTab
        v-if="activeTab === 'share'"
        :status="status"
        :loading="loading.status"
        @refresh="fetchStatus"
      />

      <PiConfigTab
        v-if="activeTab === 'pi'"
        :piAgentSettings="piSettings"
        :disabled="loading.piSettings"
        @save-agent="savePiAgentSettings"
      />

      <PiVersionsTab
        v-if="activeTab === 'versions'"
        :installInfo="piInstall"
        :download-progress="downloadProgress"
        :loading="loading.piInstall"
        :downloading="loading.downloading"
        :uninstalling="loading.uninstalling"
        :download="downloadPiVersion"
        @refresh="fetchPiInstallInfo"
        @uninstall="uninstallPi"
      />

      <ProvidersTab
        v-if="activeTab === 'providers'"
        :broker-http-url="status?.broker_http_url ?? ''"
        :pi-running="status?.pi_running ?? false"
        @restart-pi="restartPi"
      />

      <ExtensionsTab v-if="activeTab === 'extensions'" />

      <MarketplaceTab
        v-if="activeTab === 'market'"
        :packages="piSettings?.packages ?? []"
        @packages-changed="handlePackagesChanged"
      />

      <AppearanceTab
        v-if="activeTab === 'settings'"
        :settings="appSettings"
        :disabled="loading.config"
        @update="handleAppUpdate"
        @preview="handleThemePreview"
      />
      </main>
    </div>
  </div>
</template>

<style>
@import "@piter/ui/styles/design-system.css";
</style>

<style scoped>
.admin-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font);
  font-size: var(--font-size-body);
  line-height: var(--line-height-body);
}

.admin-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.admin-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.admin-main {
  flex: 1;
  overflow-y: auto;
  background: var(--bg);
}

.admin-error {
  margin: var(--space-md) var(--space-xl);
  padding: var(--space-sm) var(--space-md);
  background: var(--danger-soft);
  color: var(--danger);
  border-radius: var(--radius-sm);
  font-size: var(--font-size-caption);
}

.admin-banner {
  margin: var(--space-md) var(--space-xl);
  padding: var(--space-sm) var(--space-md);
  background: var(--danger-soft);
  color: var(--danger);
  border-radius: var(--radius-sm);
  font-size: var(--font-size-caption);
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}
.admin-banner strong {
  white-space: nowrap;
}
.admin-banner a {
  color: var(--danger);
  text-decoration: underline;
  font-weight: 600;
}
</style>
