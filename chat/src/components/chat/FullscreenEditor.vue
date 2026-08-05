<script setup lang="ts">
import { ref, watch, nextTick, onMounted, onUnmounted } from "vue";
import { X, Square } from "lucide-vue-next";

const props = defineProps<{
  modelValue: string;
  open: boolean;
  isRunning: boolean;
  isStreaming: boolean;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "send"): void;
  (e: "abort"): void;
  (e: "close"): void;
}>();

const fsInputRef = ref<HTMLTextAreaElement | null>(null);
const fsViewportRef = ref<HTMLDivElement | null>(null);

// Keep the fullscreen editor inside the *visual* viewport so the footer
// (send button) is not covered by the on-screen keyboard on mobile.
function syncFsViewport() {
  const el = fsViewportRef.value;
  const vv = window.visualViewport;
  if (!el || !vv) return;
  el.style.height = `${vv.height}px`;
  el.style.transform = `translateY(${vv.offsetTop}px)`;
}

watch(() => props.open, (open) => {
  if (open) {
    nextTick(() => {
      syncFsViewport();
      fsInputRef.value?.focus();
    });
  }
});

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  window.visualViewport?.addEventListener("resize", syncFsViewport);
  window.visualViewport?.addEventListener("scroll", syncFsViewport);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
  window.visualViewport?.removeEventListener("resize", syncFsViewport);
  window.visualViewport?.removeEventListener("scroll", syncFsViewport);
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" ref="fsViewportRef" class="composer-fullscreen">
      <div class="fs-header">
        <span class="fs-title">{{ $t("chat.editPrompt") }}</span>
        <button class="fs-close" :aria-label="$t('common.close')" @click="emit('close')">
          <X :size="18" />
        </button>
      </div>
      <textarea
        ref="fsInputRef"
        :value="modelValue"
        class="fs-input"
        :placeholder="isRunning ? $t('chat.fsPlaceholder') : $t('chat.disconnected')"
        :disabled="!isRunning"
        @input="emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)"
      />
      <div class="fs-footer">
        <span class="fs-hint">{{ isStreaming ? $t("chat.fsHintStreaming") : $t("chat.fsHintIdle") }}</span>
        <div class="fs-actions">
          <button
            v-if="isStreaming"
            class="fs-stop"
            :title="$t('chat.stopGeneration')"
            :aria-label="$t('chat.stopGeneration')"
            @click="emit('abort')"
          >
            <Square :size="14" />
          </button>
          <button
            class="btn btn-primary btn-sm fs-send"
            :disabled="!isRunning || !modelValue.trim()"
            @click="emit('send')"
          >
            {{ isStreaming ? $t("chat.sendAfterRun") : $t("chat.send") }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.composer-fullscreen {
  position:fixed; left:0; top:0; width:100%; height:100vh; z-index:100;
  display:flex; flex-direction:column;
  background:var(--bg);
  padding-top:env(safe-area-inset-top);
  padding-bottom:env(safe-area-inset-bottom);
}
.fs-header {
  display:flex; align-items:center; justify-content:space-between;
  padding:8px 12px; flex-shrink:0;
  border-bottom:1px solid var(--border);
  background:var(--bg-panel);
}
.fs-title { font-size:13px; font-weight:600; color:var(--text); }
.fs-close {
  display:flex; align-items:center; justify-content:center;
  width:30px; height:30px; border-radius:var(--radius-sm); border:none;
  background:transparent; color:var(--text-secondary); cursor:pointer;
  transition:background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}
.fs-close:hover { background:var(--bg-hover); color:var(--text); }
.fs-input {
  flex:1; width:100%; min-height:0;
  padding:14px 16px;
  border:none; background:transparent; color:var(--text);
  font-size:15px; line-height:1.6; resize:none; outline:none;
  font-family:var(--font);
}
.fs-input:disabled { opacity:0.4; }
.fs-footer {
  display:flex; align-items:center; justify-content:space-between; gap:12px;
  padding:10px 16px; flex-shrink:0;
  border-top:1px solid var(--border);
  background:var(--bg-panel);
}
.fs-hint { font-size:10px; color:var(--text-tertiary); }
.fs-actions { display:flex; align-items:center; gap:8px; }
.fs-stop {
  display:flex; align-items:center; justify-content:center;
  width:32px; height:32px; border-radius:var(--radius-sm); border:1px solid var(--border);
  background:var(--bg); color:var(--danger); cursor:pointer;
  transition:background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}
.fs-stop:hover { background:var(--danger-soft); color:var(--danger); }
.fs-send { height:32px; padding:0 18px; font-size:13px; }
</style>
