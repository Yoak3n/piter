<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Trash2 } from "lucide-vue-next";
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
  (e: "request-delete"): void;
  (e: "confirm-delete"): void;
  (e: "cancel-delete"): void;
}>();

const { t, locale } = useI18n();

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
    @click="emit('select')"
  >
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
    <button
      v-else
      class="session-delete-btn"
      :title="$t('chat.deleteSession')"
      @click.stop="emit('request-delete')"
    >
      <Trash2 :size="12" />
    </button>
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

.session-item:hover .session-delete-btn {
  opacity: 1;
}

.session-delete-btn:hover {
  background: var(--danger-soft);
  color: var(--danger);
}

/* Mobile: keep row actions visible without hover */
@media (max-width: 640px) {
  .session-delete-btn {
    opacity: 1;
  }
}
</style>
