<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Pencil, Pin, PinOff, Trash2 } from "lucide-vue-next";
import { StatusDot, InlineConfirm } from "@piter/ui";
import type { SessionInfo } from "../../types";

const props = defineProps<{
  session: SessionInfo;
  /** This session is the active one. */
  active: boolean;
  /** Pi is processing in this session. */
  running: boolean;
  /** Whether the inline "Delete? Yes/No" confirm is open for this row. */
  confirming: boolean;
  deleteLoading: boolean;
}>();

const emit = defineEmits<{
  (e: "select"): void;
  (e: "pin"): void;
  (e: "request-delete"): void;
  (e: "confirm-delete"): void;
  (e: "cancel-delete"): void;
  (e: "renamed"): void;
}>();

const { t, locale } = useI18n();

// ─── Inline rename ────────────────────────────────────────────────
const editing = ref(false);
const saving = ref(false);
const draft = ref("");
const inputEl = ref<HTMLInputElement | null>(null);

function startEdit() {
  draft.value = props.session.label;
  editing.value = true;
  nextTick(() => {
    inputEl.value?.focus();
    inputEl.value?.select();
  });
}

function cancelEdit() {
  if (saving.value) return;
  editing.value = false;
}

async function saveEdit() {
  if (saving.value) return;
  const name = draft.value.trim();
  // Empty or unchanged name → close without an API call.
  if (!name || name === props.session.label) {
    editing.value = false;
    return;
  }
  saving.value = true;
  try {
    const res = await fetch("/api/sessions/rename", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path: props.session.filePath, name }),
    });
    const data = await res.json();
    if (!data.success) throw new Error(data.error || `HTTP ${res.status}`);
    editing.value = false;
    emit("renamed");
  } catch (e) {
    console.error("Rename failed:", e);
    // Keep the input open so the user can retry.
  } finally {
    saving.value = false;
  }
}

const statusState = computed<"idle" | "busy" | "review" | "unloaded">(() => {
  switch (props.session.state) {
    case "idle":
      return "idle";
    case "busy":
      return "busy";
    case "waiting_review":
      return "review";
    default:
      return "unloaded";
  }
});

function formatTime(updatedAt: number): string {
  const diffSecs = Math.floor((Date.now() - updatedAt * 1000) / 1000);
  if (diffSecs < 60) return t("common.timeJustNow");
  const rtf = new Intl.RelativeTimeFormat(locale.value, { numeric: "auto" });
  const mins = Math.round(diffSecs / 60);
  if (mins < 60) return rtf.format(-mins, "minute");
  const hours = Math.round(mins / 60);
  if (hours < 24) return rtf.format(-hours, "hour");
  const days = Math.round(hours / 24);
  if (days < 7) return rtf.format(-days, "day");
  return new Date(updatedAt * 1000).toLocaleDateString(locale.value, {
    month: "short",
    day: "numeric",
  });
}
</script>

<template>
  <button
    class="session-item"
    :class="{ active }"
    @click="!editing && emit('select')"
  >
    <template v-if="editing">
      <div class="session-rename-wrap" @click.stop>
        <input
          ref="inputEl"
          v-model="draft"
          class="session-rename-input"
          :title="$t('chat.renameSession')"
          @keydown.enter="saveEdit"
          @keydown.esc="cancelEdit"
          @blur="cancelEdit"
        />
      </div>
    </template>
    <template v-else>
      <div class="session-item-main">
        <div class="session-title">
          <StatusDot
            :state="statusState"
            :title="session.state || 'unloaded'"
          />
          <span
            v-if="running"
            class="session-running-indicator"
            :title="$t('chat.piProcessing')"
          />
          <Pin
            v-if="session.pinned"
            :size="10"
            class="session-pinned-marker"
            fill="currentColor"
          />
          {{ session.label || $t("common.untitled") }}
        </div>
        <div class="session-meta">
          <span class="session-time">{{ formatTime(session.updatedAt) }}</span>
        </div>
      </div>

      <InlineConfirm
        v-if="confirming"
        :prompt="$t('chat.deletePrompt')"
        :busy="deleteLoading"
        @confirm="emit('confirm-delete')"
        @cancel="emit('cancel-delete')"
      />
      <template v-else>
        <button
          class="session-pin-btn"
          :title="session.pinned ? $t('chat.unpinSession') : $t('chat.pinSession')"
          @click.stop="emit('pin')"
        >
          <PinOff v-if="session.pinned" :size="12" />
          <Pin v-else :size="12" />
        </button>
        <button
          class="session-edit-btn"
          :title="$t('chat.renameSession')"
          @click.stop="startEdit"
        >
          <Pencil :size="12" />
        </button>
        <button
          class="session-delete-btn"
          :title="$t('chat.deleteSession')"
          @click.stop="emit('request-delete')"
        >
          <Trash2 :size="12" />
        </button>
      </template>
    </template>
  </button>
</template>

<style scoped>
.session-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px 8px 22px;
  cursor: pointer;
  border: none;
  background: none;
  width: 100%;
  text-align: left;
  font-size: 12px;
  color: var(--text);
  gap: 6px;
}

.session-item:hover {
  background: var(--bg-hover);
}

.session-item.active {
  background: var(--accent-soft);
  border-left: 2px solid var(--accent);
  padding-left: 20px;
}

.session-item-main {
  flex: 1;
  min-width: 0;
}

.session-title {
  font-size: 12px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  display: flex;
  align-items: center;
  gap: 5px;
}

.session-running-indicator {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
  flex-shrink: 0;
  animation: session-pulse 1.2s ease-in-out infinite;
}

/* 会话置顶标记：与项目置顶（accent 蓝）区分，用 warning 琥珀色 */
.session-pinned-marker {
  color: var(--warning);
  flex-shrink: 0;
}

@keyframes session-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.3;
  }
}

.session-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 2px;
}

.session-time {
  font-size: 10px;
  color: var(--text-tertiary);
}

.session-pin-btn,
.session-edit-btn,
.session-delete-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity var(--duration-fast) var(--ease);
}

.session-item:hover .session-pin-btn,
.session-item:hover .session-edit-btn,
.session-item:hover .session-delete-btn {
  opacity: 1;
}

.session-pin-btn:hover {
  background: var(--warning-soft);
  color: var(--warning);
}

.session-edit-btn:hover {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.session-delete-btn:hover {
  background: var(--danger-soft);
  color: var(--danger);
}

.session-rename-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
}

.session-rename-input {
  flex: 1;
  min-width: 0;
  padding: 4px 8px;
  font-size: 12px;
  font-weight: 500;
  color: var(--text);
  background: var(--bg-panel);
  border: 1px solid var(--accent);
  border-radius: var(--radius-sm);
  outline: none;
}

/* Mobile: keep row actions visible without hover */
@media (max-width: 640px) {
  .session-pin-btn,
  .session-edit-btn,
  .session-delete-btn {
    opacity: 1;
  }
}
</style>
