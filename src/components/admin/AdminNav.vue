<script setup lang="ts">
import { Settings, Cpu, Activity, Puzzle, Globe, GitBranch } from "lucide-vue-next";

defineProps<{
  activeTab: string;
}>();

const emit = defineEmits<{
  (e: "select", tab: string): void;
}>();

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tabs = [
  { key: "status", label: "Status", icon: Activity },
  { key: "pi", label: "Pi Config", icon: Cpu },
  { key: "versions", label: "Versions", icon: GitBranch },
  { key: "extensions", label: "Extensions", icon: Puzzle },
];

const bottomTabs = [
  { key: "settings", label: "Appearance", icon: Settings },
];

async function openWebApp() {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("navigate_to_web");
  } catch {
    // ignore silently
  }
}
</script>

<template>
  <nav class="admin-nav">
    <div class="admin-nav-brand">
      <span class="admin-nav-title">Piter</span>
    </div>

    <div class="admin-nav-group">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="admin-nav-item"
        :class="{ active: activeTab === tab.key }"
        @click="emit('select', tab.key)"
      >
        <component :is="tab.icon" :size="15" />
        <span>{{ tab.label }}</span>
      </button>
    </div>

    <div class="admin-nav-spacer"></div>

    <div class="admin-nav-group admin-nav-bottom">
      <button
        v-for="tab in bottomTabs"
        :key="tab.key"
        class="admin-nav-item"
        :class="{ active: activeTab === tab.key }"
        @click="emit('select', tab.key)"
      >
        <component :is="tab.icon" :size="15" />
        <span>{{ tab.label }}</span>
      </button>

      <div class="admin-nav-divider"></div>

      <button
        v-if="isTauri"
        class="admin-nav-action"
        @click="openWebApp"
      >
        <Globe :size="13" />
        <span>Open Chat View</span>
      </button>
    </div>
  </nav>
</template>

<style scoped>
.admin-nav {
  display: flex;
  flex-direction: column;
  width: 200px;
  height: 100%;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border);
  padding: var(--space-md);
  gap: 2px;
  flex-shrink: 0;
}

.admin-nav-brand {
  padding: var(--space-xs) var(--space-sm);
  margin-bottom: var(--space-sm);
}

.admin-nav-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.admin-nav-group {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.admin-nav-bottom {
  margin-top: auto;
}

.admin-nav-divider {
  height: 1px;
  background: var(--border);
  margin: var(--space-xs) 0;
}

.admin-nav-item {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  width: 100%;
  padding: var(--space-xs) var(--space-sm);
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: var(--font-size-control);
  font-family: var(--font);
  cursor: pointer;
  height: 30px;
  text-align: left;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.admin-nav-item:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.admin-nav-item.active {
  background: var(--bg-active);
  color: var(--text);
  font-weight: 500;
}

.admin-nav-spacer {
  flex: 1;
}

.admin-nav-action {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  width: 100%;
  padding: var(--space-xs) var(--space-sm);
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
  font-family: var(--font);
  height: 28px;
  cursor: pointer;
  transition: color var(--duration-fast) var(--ease), background var(--duration-fast) var(--ease);
}

.admin-nav-action:hover {
  color: var(--text-secondary);
  background: var(--bg-hover);
}
</style>
