<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { useAdmin } from "../composables/useAdmin";
import type { AppSettings, PiSettings, AdminConfig } from "../composables/useAdmin";
import AdminNav from "../components/admin/AdminNav.vue";
import AppSettingsTab from "../components/admin/AppSettingsTab.vue";
import PiSettingsTab from "../components/admin/PiSettingsTab.vue";
import SystemStatusTab from "../components/admin/SystemStatusTab.vue";

const { config, status, piSettings: piAgentSettings, loading, error, fetchConfig, saveConfig, fetchStatus, restartPi, stopPi, fetchPiAgentSettings } =
  useAdmin();

const activeTab = ref("settings");

onMounted(() => {
  document.documentElement.dataset.theme =
    window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  fetchConfig();
});

function handleTabSelect(tab: string) {
  activeTab.value = tab;
  if (tab === "pi") {
    fetchPiAgentSettings();
  }
}

const appSettings = computed<AppSettings>(() => {
  return (
    config.value?.app ?? {
      theme: "system",
      auto_start: true,
      start_minimized: true,
    }
  );
});

const piSettings = computed<PiSettings>(() => {
  return (
    config.value?.pi ?? {
      default_model: "",
      request_timeout_secs: 300,
      auto_restart_on_crash: true,
    }
  );
});

function handleAppUpdate(settings: AppSettings) {
  if (!config.value) return;
  const updated: AdminConfig = {
    ...config.value,
    app: settings,
  };
  saveConfig(updated);
}

function handlePiUpdate(settings: PiSettings) {
  if (!config.value) return;
  const updated: AdminConfig = {
    ...config.value,
    pi: settings,
  };
  saveConfig(updated);
}
</script>

<template>
  <div class="admin-view">
    <AdminNav :activeTab="activeTab" @select="handleTabSelect" />

    <main class="admin-main">
      <div v-if="error" class="admin-error">{{ error }}</div>

      <AppSettingsTab
        v-if="activeTab === 'settings'"
        :settings="appSettings"
        :disabled="loading.config"
        @update="handleAppUpdate"
      />

      <PiSettingsTab
        v-if="activeTab === 'pi'"
        :settings="piSettings"
        :piAgentSettings="piAgentSettings"
        :disabled="loading.config"
        @update="handlePiUpdate"
      />

      <SystemStatusTab
        v-if="activeTab === 'status'"
        :status="status"
        :loading="loading.status"
        @refresh="fetchStatus"
        @restart-pi="restartPi"
        @stop-pi="stopPi"
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
</style>
