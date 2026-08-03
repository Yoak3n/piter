<script setup lang="ts">
import { ref, watch, nextTick } from "vue";
import { Send, Maximize, Square, Zap, Clock, X } from "lucide-vue-next";
import type { PendingItem } from "../../composables/usePiConnection";

const props = defineProps<{
  modelValue: string;
  isRunning: boolean;
  isStreaming: boolean;
  /** 本地待投递队列（可取消 / 升级为插队） */
  outbox?: PendingItem[];
  /** pi 原生插队队列（只读展示） */
  steeringQueue?: string[];
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "send"): void;
  (e: "steer"): void;
  (e: "abort"): void;
  (e: "cancel-queued", id: number): void;
  (e: "upgrade-queued", id: number): void;
  (e: "expand"): void;
  (e: "restart-pi"): void;
}>();

const inputRef = ref<HTMLTextAreaElement | null>(null);

function autoGrow() {
  const el = inputRef.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = `${el.scrollHeight}px`;
}

function onInput(e: Event) {
  emit("update:modelValue", (e.target as HTMLTextAreaElement).value);
  nextTick(autoGrow);
}

watch(() => props.modelValue, () => nextTick(autoGrow));

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    emit("send");
  }
}
</script>

<template>
  <div class="composer">
    <div
      v-if="outbox?.length || steeringQueue?.length"
      class="composer-queue"
    >
      <span class="queue-label">Queue</span>
      <span
        v-for="(m, i) in steeringQueue"
        :key="`s${i}`"
        class="queue-chip queue-chip-steer"
        :title="m"
      >
        <Zap :size="10" />{{ m }}
      </span>
      <span
        v-for="item in outbox"
        :key="`o${item.id}`"
        class="queue-chip queue-chip-followup"
        :title="item.text"
      >
        <Clock :size="10" /><span class="queue-chip-text">{{ item.text }}</span>
        <button
          class="chip-btn chip-upgrade"
          title="Upgrade to insert now (steer)"
          aria-label="Upgrade to insert now"
          @click.stop="emit('upgrade-queued', item.id)"
        >
          <Zap :size="10" />
        </button>
        <button
          class="chip-btn chip-cancel"
          title="Cancel queued message"
          aria-label="Cancel queued message"
          @click.stop="emit('cancel-queued', item.id)"
        >
          <X :size="10" />
        </button>
      </span>
    </div>
    <div class="composer-box">
      <div class="composer-main">
        <textarea
          ref="inputRef"
          :value="modelValue"
          class="composer-input"
          :placeholder="isRunning ? 'Message Pi...' : 'Disconnected'"
          :disabled="!isRunning"
          rows="2"
          @input="onInput"
          @keydown="handleKeydown"
        />
        <div class="composer-btns">
          <button
            v-if="isRunning"
            class="composer-tool-btn composer-expand-btn"
            title="Expand editor"
            aria-label="Expand editor"
            @click="emit('expand')"
          >
            <Maximize :size="14" />
          </button>
          <button
            v-if="isStreaming"
            class="composer-tool-btn composer-stop-btn"
            title="Stop generation"
            aria-label="Stop generation"
            @click="emit('abort')"
          >
            <Square :size="14" />
          </button>
          <button
            v-if="isStreaming"
            class="composer-tool-btn composer-steer-btn"
            title="Insert now (steer)"
            aria-label="Insert now"
            :disabled="!modelValue.trim()"
            @click="emit('steer')"
          >
            <Zap :size="14" />
          </button>
          <button
            class="composer-send-btn"
            :disabled="!isRunning || !modelValue.trim()"
            :title="isStreaming ? 'Send after this run finishes' : 'Send'"
            aria-label="Send"
            @click="emit('send')"
          >
            <Clock v-if="isStreaming" :size="15" />
            <Send v-else :size="15" />
          </button>
        </div>
      </div>
      <div class="composer-hint">
        <template v-if="isRunning">
          <template v-if="isStreaming">Enter sends after this run finishes · ⚡ inserts now · ■ stops</template>
          <template v-else>Enter to send, Shift+Enter for newline</template>
        </template>
        <template v-else>
          <button class="btn-ghost-sm" @click="emit('restart-pi')">Reconnect</button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.composer { flex-shrink:0; border-top:1px solid var(--color-border-subtle); background:var(--color-bg-panel); }
.composer-box { display:flex; flex-direction:column; padding:10px 12px; gap:6px; }
.composer-main { position:relative; }
.composer-input {
  width:100%; min-height:56px; max-height:180px;
  padding:12px 52px 12px 14px;
  border:1px solid var(--color-border-subtle); border-radius:14px;
  background:var(--color-bg-app); color:var(--color-text-primary);
  font-size:13px; line-height:1.5; resize:none; outline:none;
  font-family:var(--font-family-base); overflow-y:auto;
  transition:border-color 0.2s var(--ease), box-shadow 0.2s var(--ease);
}
.composer-input:focus { border-color:var(--color-accent); box-shadow:var(--focus-ring); }
.composer-input:disabled { opacity:0.4; }
.composer-btns { position:absolute; right:8px; bottom:8px; display:flex; align-items:center; gap:6px; }
.composer-tool-btn {
  display:flex; align-items:center; justify-content:center;
  width:30px; height:30px; border-radius:8px; border:none;
  background:transparent; color:var(--color-text-tertiary); cursor:pointer;
  transition:background 0.15s var(--ease), color 0.15s var(--ease);
}
.composer-tool-btn:hover { background:var(--color-bg-hover); color:var(--color-text-primary); }
.composer-expand-btn { display:none; }
.composer-send-btn {
  display:flex; align-items:center; justify-content:center;
  width:34px; height:34px; border-radius:50%; border:none;
  background:var(--color-accent); color:#fff; cursor:pointer; flex-shrink:0;
  transition:background 0.15s var(--ease), transform 0.1s var(--ease), opacity 0.15s var(--ease);
}
.composer-send-btn:hover { background:var(--color-accent-strong); }
.composer-send-btn:active { transform:scale(0.95); }
.composer-send-btn:disabled { opacity:0.3; cursor:default; transform:none; }
.composer-hint { font-size:10px; color:var(--color-text-tertiary); }

/* ── Queue indicator ─────────────────────────────── */
.composer-queue {
  display:flex; align-items:center; gap:6px;
  padding:6px 12px 0; overflow-x:auto; flex-shrink:0;
}
.queue-label { font-size:10px; color:var(--color-text-tertiary); flex-shrink:0; }
.queue-chip {
  display:inline-flex; align-items:center; gap:4px;
  max-width:150px; padding:2px 8px; border-radius:999px;
  font-size:10px; line-height:1.4; white-space:nowrap; overflow:hidden;
  text-overflow:ellipsis; border:1px solid var(--color-border-subtle);
  background:var(--color-bg-app); color:var(--color-text-secondary); flex-shrink:0;
}
.queue-chip svg { flex-shrink:0; }
/* 文本包 span 可收缩省略（min-width:0），⚡/✕ 按钮 flex-shrink:0 不被挤走，始终可见 */
.queue-chip-text { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.queue-chip-followup { border-color:var(--color-border-subtle); color:var(--color-text-secondary); }
.queue-chip-steer { border-color:var(--color-accent); color:var(--color-accent); }
.chip-btn {
  display:inline-flex; align-items:center; justify-content:center;
  width:16px; height:16px; border:none; border-radius:4px; padding:0;
  background:transparent; color:inherit; cursor:pointer; opacity:0.6; flex-shrink:0;
  transition:opacity 0.15s var(--ease), background 0.15s var(--ease);
}
.chip-btn:hover { opacity:1; background:var(--color-bg-hover); }
.chip-upgrade:hover { color:var(--color-accent); }
.chip-cancel:hover { color:var(--color-danger, #ef4444); }

.composer-stop-btn:hover { background:rgba(239,68,68,0.12); color:var(--color-danger, #ef4444); }
.composer-steer-btn:hover { background:var(--color-accent-soft, var(--color-bg-hover)); color:var(--color-accent); }
.composer-steer-btn:disabled { opacity:0.3; cursor:default; }

/* 桌面端放宽队列 chip 宽度，长消息少省略 */
@media (min-width: 641px) {
  .queue-chip { max-width:400px; }
}

@media (max-width: 640px) {
  .composer-box { padding-bottom:calc(10px + env(safe-area-inset-bottom)); }
  .composer-input { font-size:16px; min-height:60px; }  /* prevent iOS zoom */
  .composer-expand-btn { display:flex; }
}
</style>
