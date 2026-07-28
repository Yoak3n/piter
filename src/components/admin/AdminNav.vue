<script setup lang="ts">
import { Settings, Cpu, Activity } from "lucide-vue-next";

defineProps<{
  activeTab: string;
}>();

const emit = defineEmits<{
  (e: "select", tab: string): void;
}>();

const tabs = [
  { key: "settings", label: "Settings", icon: Settings },
  { key: "pi", label: "Pi Config", icon: Cpu },
  { key: "status", label: "Status", icon: Activity },
];
</script>

<template>
  <nav class="admin-nav">
    <div class="admin-nav-title">Admin</div>
    <button
      v-for="tab in tabs"
      :key="tab.key"
      class="admin-nav-item"
      :class="{ active: activeTab === tab.key }"
      @click="emit('select', tab.key)"
    >
      <component :is="tab.icon" :size="16" />
      <span>{{ tab.label }}</span>
    </button>
    <div class="admin-nav-spacer"></div>
    <router-link to="/chat" class="admin-nav-back">
      &larr; Back to Chat
    </router-link>
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

.admin-nav-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: var(--space-xs) var(--space-sm);
  margin-bottom: var(--space-xs);
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
  height: 32px;
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

.admin-nav-back {
  display: flex;
  align-items: center;
  padding: var(--space-xs) var(--space-sm);
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
  text-decoration: none;
  height: 28px;
  transition: color var(--duration-fast) var(--ease);
}

.admin-nav-back:hover {
  color: var(--text-secondary);
}
</style>
