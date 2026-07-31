<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { useAdmin } from "../composables/useAdmin";
import type { AppSettings, PiSettings } from "../composables/useAdmin";
import AdminNav from "../components/admin/AdminNav.vue";
import StatusTab from "../components/admin/StatusTab.vue";
import PiConfigTab from "../components/admin/PiConfigTab.vue";
import PiVersionsTab from "../components/admin/PiVersionsTab.vue";
import ExtensionsTab from "../components/admin/ExtensionsTab.vue";
import AppearanceTab from "../components/admin/AppearanceTab.vue";

const { config, status, piSettings, piInstall, loading, error,
  fetchConfig, saveConfig, fetchStatus, restartPi, stopPi,
  fetchPiAgentSettings, savePiAgentSettings, openPath,
  fetchPiInstallInfo, downloadPiVersion, uninstallPi,
} = useAdmin();

const activeTab = ref("status");

onMounted(() => {
  document.documentElement.dataset.theme =
    window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  fetchConfig();
  fetchStatus();
  fetchPiAgentSettings();
  fetchPiInstallInfo();
});

function handleTabSelect(tab: string) {
  activeTab.value = tab;
  if (tab === "status") fetchStatus();
  if (tab === "versions") fetchPiInstallInfo();
}

const appSettings = computed<AppSettings>(() =>
  config.value?.app ?? { theme: "system", auto_start: true, start_minimized: true }
);

const piSettingsVal = computed<PiSettings>(() =>
  config.value?.pi ?? { default_model: "", request_timeout_secs: 300, auto_restart_on_crash: true }
);

const piMissing = computed(() => status.value?.pi_binary_missing ?? false);

function handleAppUpdate(settings: AppSettings) {
  if (!config.value) return;
  saveConfig({ ...config.value, app: settings });
}

function handlePiUpdate(settings: PiSettings) {
  if (!config.value) return;
  saveConfig({ ...config.value, pi: settings });
}
</script>

<template>
  <div class="admin-view">
    <AdminNav :activeTab="activeTab" @select="handleTabSelect" />
    <main class="admin-main">
      <div v-if="error" class="admin-error">{{ error }}</div>

      <div v-if="piMissing && activeTab !== 'versions'" class="admin-banner">
        <strong>Pi runtime not found.</strong>
        <span>Pi features are unavailable. Go to <a href="#" @click.prevent="activeTab = 'versions'">Versions</a> to download a Pi binary.</span>
      </div>

      <StatusTab
        v-if="activeTab === 'status'"
        :status="status"
        :loading="loading.status"
        @refresh="fetchStatus"
        @restart-pi="restartPi"
        @stop-pi="stopPi"
        @open-path="openPath"
      />

      <PiConfigTab
        v-if="activeTab === 'pi'"
        :settings="piSettingsVal"
        :piAgentSettings="piSettings"
        :disabled="loading.config || loading.piSettings"
        @update="handlePiUpdate"
        @save-agent="savePiAgentSettings"
      />

      <PiVersionsTab
        v-if="activeTab === 'versions'"
        :installInfo="piInstall"
        :loading="loading.piInstall"
        :downloading="loading.downloading"
        :uninstalling="loading.uninstalling"
        @refresh="fetchPiInstallInfo"
        @download="downloadPiVersion"
        @uninstall="uninstallPi"
      />

      <ExtensionsTab
        v-if="activeTab === 'extensions'"
        :piAgentSettings="piSettings"
        :disabled="loading.piSettings"
        @save-agent="savePiAgentSettings"
      />

      <AppearanceTab
        v-if="activeTab === 'settings'"
        :settings="appSettings"
        :disabled="loading.config"
        @update="handleAppUpdate"
      />
    </main>
  </div>
</template>

<style>
@import "../styles/design-system.css";
</style>

<style scoped>
.admin-view {
  display: flex;
  height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font);
  font-size: var(--font-size-body);
  line-height: var(--line-height-body);
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
  background: var(--warning-soft);
  color: var(--warning);
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
  color: var(--warning);
  text-decoration: underline;
  font-weight: 600;
}
</style>
