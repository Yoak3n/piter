<script setup lang="ts">
import { useTauriWindow } from "./composables/useTauriWindow";

const { isTauri, isMaximized, minimize, toggleMaximize, close } =
  useTauriWindow();
</script>

<template>
  <!--
    Custom title bar shell shared by the chat and admin frontends.
    The whole bar is a window drag region (data-tauri-drag-region); any
    interactive child (buttons, selects…) must NOT carry the attribute, so it
    stays clickable. Window controls render only under Tauri.
  -->
  <header class="piter-titlebar" data-tauri-drag-region>
    <div class="piter-titlebar__side" data-tauri-drag-region>
      <slot name="left" />
    </div>
    <div class="piter-titlebar__center" data-tauri-drag-region>
      <slot name="center" />
    </div>
    <div class="piter-titlebar__spacer" data-tauri-drag-region />
    <div class="piter-titlebar__side" data-tauri-drag-region>
      <slot name="right" />
    </div>

    <div v-if="isTauri" class="piter-titlebar__controls">
      <button
        type="button"
        class="piter-titlebar__btn"
        title="Minimize"
        aria-label="Minimize"
        @click="minimize"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
      <button
        type="button"
        class="piter-titlebar__btn"
        :title="isMaximized ? 'Restore' : 'Maximize'"
        :aria-label="isMaximized ? 'Restore' : 'Maximize'"
        @click="toggleMaximize"
      >
        <svg v-if="isMaximized" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path
            d="M3.5 2.5h4a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1h-4a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1z"
            fill="none"
            stroke="currentColor"
            stroke-width="1"
          />
          <path d="M3.5 3.5h3.5v3.5" fill="none" stroke="currentColor" stroke-width="1" opacity="0.5" />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <rect x="1" y="1" width="8" height="8" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
      <button
        type="button"
        class="piter-titlebar__btn piter-titlebar__btn--close"
        title="Close"
        aria-label="Close"
        @click="close"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.piter-titlebar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: var(--titlebar-h, 44px);
  flex-shrink: 0;
  padding: 0 12px;
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border);
  user-select: none;
  -webkit-user-select: none;
}

.piter-titlebar__side,
.piter-titlebar__center {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.piter-titlebar__spacer {
  flex: 1;
  min-width: 0;
  align-self: stretch;
}

/* Window caption buttons — flush with the right edge of the window. */
.piter-titlebar__controls {
  display: flex;
  align-self: stretch;
  margin: 0 -12px 0 4px;
}

.piter-titlebar__btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 46px;
  height: 100%;
  border: none;
  background: none;
  color: var(--text-secondary);
  cursor: default;
  transition: background var(--duration-fast, 0.12s) var(--ease, ease);
}

.piter-titlebar__btn:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.piter-titlebar__btn--close:hover {
  background: #e81123;
  color: #fff;
}
</style>
