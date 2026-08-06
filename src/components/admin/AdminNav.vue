<script setup lang="ts">
import { ref, watch } from "vue";
import { Settings, Cpu, Activity, Puzzle, Globe, GitBranch, Store, ChevronDown, KeyRound, ChartColumn, Share2 } from "lucide-vue-next";

const props = defineProps<{
  activeTab: string;
  chatAvailable: boolean;
}>();

const emit = defineEmits<{
  (e: "select", tab: string): void;
}>();

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tabs = [
  { key: "status", labelKey: "admin.navStatus", icon: Activity },
  { key: "share", labelKey: "admin.navShare", icon: Share2 },
  { key: "usage", labelKey: "admin.navUsage", icon: ChartColumn },
];

// Pi is a parent group: everything here configures the Pi runtime itself.
const piTabs = [
  { key: "pi", labelKey: "admin.navConfig", icon: Cpu },
  { key: "providers", labelKey: "admin.navProviders", icon: KeyRound },
  { key: "versions", labelKey: "admin.navVersions", icon: GitBranch },
];
const piGroupKeys = ["pi", "providers", "versions"];

// Extensions is a parent group: "Installed" (enabled list) + "Market" (browse).
const extensionTabs = [
  { key: "extensions", labelKey: "admin.navInstalled", icon: Puzzle },
  { key: "market", labelKey: "admin.navMarket", icon: Store },
];

const bottomTabs = [
  { key: "settings", labelKey: "admin.navAppearance", icon: Settings },
];

const extensionGroupKeys = ["extensions", "market"];

const piOpen = ref(piGroupKeys.includes(props.activeTab));
const extensionsOpen = ref(extensionGroupKeys.includes(props.activeTab));
watch(
  () => props.activeTab,
  (tab) => {
    if (piGroupKeys.includes(tab)) piOpen.value = true;
    if (extensionGroupKeys.includes(tab)) extensionsOpen.value = true;
  }
);

function togglePi() {
  piOpen.value = !piOpen.value;
}

function toggleExtensions() {
  extensionsOpen.value = !extensionsOpen.value;
}

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
        <span>{{ $t(tab.labelKey) }}</span>
      </button>
    </div>

    <div class="admin-nav-group">
      <button
        class="admin-nav-item admin-nav-parent"
        :class="{ active: piGroupKeys.includes(activeTab) }"
        @click="togglePi"
      >
        <Cpu :size="15" />
        <span>{{ $t("admin.navPi") }}</span>
        <ChevronDown :size="13" class="nav-chevron" :class="{ open: piOpen }" />
      </button>
      <template v-if="piOpen">
        <button
          v-for="tab in piTabs"
          :key="tab.key"
          class="admin-nav-item admin-nav-child"
          :class="{ active: activeTab === tab.key }"
          @click="emit('select', tab.key)"
        >
          <component :is="tab.icon" :size="15" />
          <span>{{ $t(tab.labelKey) }}</span>
        </button>
      </template>
    </div>

    <div class="admin-nav-group">
      <button
        class="admin-nav-item admin-nav-parent"
        :class="{ active: extensionGroupKeys.includes(activeTab) }"
        @click="toggleExtensions"
      >
        <Puzzle :size="15" />
        <span>{{ $t("admin.navExtensions") }}</span>
        <ChevronDown :size="13" class="nav-chevron" :class="{ open: extensionsOpen }" />
      </button>
      <template v-if="extensionsOpen">
        <button
          v-for="tab in extensionTabs"
          :key="tab.key"
          class="admin-nav-item admin-nav-child"
          :class="{ active: activeTab === tab.key }"
          @click="emit('select', tab.key)"
        >
          <component :is="tab.icon" :size="15" />
          <span>{{ $t(tab.labelKey) }}</span>
        </button>
      </template>
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
        <span>{{ $t(tab.labelKey) }}</span>
      </button>

      <div class="admin-nav-divider"></div>

      <button
        v-if="isTauri"
        class="admin-nav-action"
        :class="{ ready: chatAvailable }"
        :disabled="!chatAvailable"
        :title="$t('admin.chatViewTitle')"
        @click="openWebApp"
      >
        <Globe :size="13" />
        <span>{{ $t("admin.openChatView") }}</span>
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
  background: var(--accent-soft);
  color: var(--accent-strong);
  font-weight: 500;
}

/* Parent group item (e.g. Extensions) */
.admin-nav-parent {
  color: var(--text-tertiary);
  margin-top: 2px;
}

.admin-nav-child {
  padding-left: calc(var(--space-lg) + var(--space-xs));
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
}

.nav-chevron {
  margin-left: auto;
  color: var(--text-tertiary);
  transition: transform var(--duration-fast) var(--ease);
  flex-shrink: 0;
}

.nav-chevron.open {
  transform: rotate(180deg);
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

/* Chat view reachable: make it stand out as the primary call-to-action —
   accent as soft fill + outline, no big blue block. */
.admin-nav-action.ready {
  background: var(--accent-soft);
  color: var(--accent-strong);
  font-weight: 600;
  border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
}
.admin-nav-action.ready:hover {
  background: var(--accent-glow);
  border-color: var(--accent);
  color: var(--accent-strong);
}
.admin-nav-action:disabled {
  cursor: not-allowed;
  opacity: 0.65;
}
.admin-nav-action:disabled:hover {
  background: transparent;
  color: var(--text-tertiary);
}
</style>
