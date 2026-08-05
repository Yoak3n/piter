<script setup lang="ts">
import { Pin, PinOff, Archive, ArchiveRestore, Plus } from "lucide-vue-next";
import { InlineConfirm } from "@piter/ui";
import SessionItem from "./SessionItem.vue";
import type { ProjectGroup as ProjectGroupType } from "../../types";

defineProps<{
  project: ProjectGroupType;
  collapsed: boolean;
  activeSessionId: string | null;
  sessionStatus: "running" | "idle" | null;
  /** This project is awaiting archive confirm. */
  archiveConfirm: boolean;
  actionLoading: boolean;
  /** Session id currently awaiting delete confirm. */
  deleteConfirmId: string | null;
  deleteLoading: boolean;
}>();

const emit = defineEmits<{
  (e: "toggle"): void;
  (e: "pin"): void;
  (e: "archive"): void;
  (e: "confirm-archive"): void;
  (e: "cancel-archive"): void;
  (e: "restore"): void;
  (e: "new-session"): void;
  (e: "select-session", instanceId: string): void;
  (e: "request-delete", instanceId: string): void;
  (e: "confirm-delete", instanceId: string): void;
  (e: "cancel-delete"): void;
}>();

// Delete confirm + delete action use the same identity as session selection
// (instanceId with a fallback to the DB id), keeping the whole flow consistent.
function sessionKey(s: { instanceId?: string; id: string }): string {
  return s.instanceId ?? s.id;
}
</script>

<template>
  <div class="project-group">
    <div
      class="project-header"
      role="button"
      :title="project.path"
      @click="emit('toggle')"
    >
      <span
        class="project-chevron"
        :class="{ collapsed }"
      >&#9660;</span>
      <span class="project-name">{{ project.name }}</span>
      <Pin
        v-if="project.pinned"
        :size="11"
        class="project-pinned-icon"
        fill="currentColor"
      />
      <Archive
        v-if="project.archived"
        :size="11"
        class="project-archived-icon"
      />
      <span class="project-count">{{ project.sessions.length }}</span>
      <template v-if="project.id">
        <InlineConfirm
          v-if="archiveConfirm"
          :prompt="$t('chat.archivePrompt')"
          :busy="actionLoading"
          @confirm="emit('confirm-archive')"
          @cancel="emit('cancel-archive')"
        />
        <button
          v-else-if="project.archived"
          class="project-action-btn"
          :title="$t('chat.restoreProject')"
          :disabled="actionLoading"
          @click.stop="emit('restore')"
        >
          <ArchiveRestore :size="12" />
        </button>
        <template v-else>
          <button
            class="project-action-btn"
            :title="project.pinned ? $t('chat.unpinProject') : $t('chat.pinProject')"
            :disabled="actionLoading"
            @click.stop="emit('pin')"
          >
            <PinOff v-if="project.pinned" :size="12" />
            <Pin v-else :size="12" />
          </button>
          <button
            class="project-action-btn"
            :title="$t('chat.archiveProject')"
            :disabled="actionLoading"
            @click.stop="emit('archive')"
          >
            <Archive :size="12" />
          </button>
        </template>
      </template>
      <button
        class="project-new-btn"
        :title="$t('chat.newChat')"
        @click.stop="emit('new-session')"
      >
        <Plus :size="12" />
      </button>
    </div>

    <div v-if="!collapsed" class="project-sessions">
      <SessionItem
        v-for="session in project.sessions"
        :key="sessionKey(session)"
        :session="session"
        :active="sessionKey(session) === activeSessionId"
        :running="sessionKey(session) === activeSessionId && sessionStatus === 'running'"
        :confirming="deleteConfirmId === sessionKey(session)"
        :delete-loading="deleteLoading"
        @select="emit('select-session', sessionKey(session))"
        @request-delete="emit('request-delete', sessionKey(session))"
        @confirm-delete="emit('confirm-delete', sessionKey(session))"
        @cancel-delete="emit('cancel-delete')"
      />
    </div>
  </div>
</template>

<style scoped>
.project-group {
  border-bottom: 1px solid var(--border);
}

.project-group:last-child {
  border-bottom: none;
}

.project-header {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 8px 10px;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  cursor: pointer;
  border: none;
  background: none;
  text-align: left;
}

.project-header:hover {
  background: var(--bg-hover);
}

.project-chevron {
  font-size: 8px;
  transition: transform var(--duration-fast) var(--ease);
  flex-shrink: 0;
}

.project-chevron.collapsed {
  transform: rotate(-90deg);
}

.project-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.project-count {
  font-size: 10px;
  color: var(--text-tertiary);
  font-weight: 400;
}

.project-new-btn,
.project-action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity var(--duration-fast) var(--ease);
}

.project-header:hover .project-new-btn,
.project-header:hover .project-action-btn {
  opacity: 1;
}

.project-new-btn:hover,
.project-action-btn:hover {
  background: var(--bg-active);
  color: var(--text);
}

.project-action-btn:disabled {
  opacity: 0.3;
  cursor: default;
}

.project-pinned-icon {
  color: var(--accent);
  flex-shrink: 0;
}

.project-archived-icon {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.project-sessions {
  display: flex;
  flex-direction: column;
}

/* Mobile: keep row actions visible without hover */
@media (max-width: 640px) {
  .project-new-btn,
  .project-action-btn {
    opacity: 1;
  }
}
</style>
