<script setup lang="ts">
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Message, ToolExecution, ChatTurn, Attachment, ImageContent, ModelRef } from "../../types";
import type { PendingItem } from "../../composables/usePiConnection";
import { buildPromptPayload } from "../../utils/attachments";
import MessageTimeline from "./MessageTimeline.vue";
import Composer from "./Composer.vue";
import FullscreenEditor from "./FullscreenEditor.vue";

const { t } = useI18n();

const props = defineProps<{
  messages: Message[];
  isRunning: boolean;
  isStreaming: boolean;
  currentAssistantContent: string;
  currentThinking?: string;
  toolExecutions?: ToolExecution[];
  draft?: string;
  outbox?: PendingItem[];
  steeringQueue?: string[];
  attachments?: Attachment[];
  currentModel?: ModelRef | null;
  visionHint?: { text: string; key: number } | null;
}>();

const emit = defineEmits<{
  (e: "send", payload: { text: string; images: ImageContent[] }): void;
  (e: "steer", payload: { text: string; images: ImageContent[] }): void;
  (e: "abort"): void;
  (e: "cancel-queued", id: number): void;
  (e: "upgrade-queued", id: number): void;
  (e: "update:draft", text: string): void;
  (e: "update:attachments", attachments: Attachment[]): void;
  (e: "restart-pi"): void;
}>();

// Draft is owned by the parent (keyed by session), so switching sessions
// preserves each session's own unsent text.
const inputText = computed<string>({
  get: () => props.draft ?? "",
  set: (v) => emit("update:draft", v),
});

const expanded = ref(false);

const turns = computed<ChatTurn[]>(() => {
  const result: ChatTurn[] = [];
  let current: ChatTurn | null = null;
  for (const msg of props.messages) {
    if (msg.role === "user") {
      if (current) result.push(current);
      current = { id: msg.id, user: msg, assistants: [], tools: [], system: null };
    } else if (msg.role === "assistant") {
      if (!current) current = { id: msg.id, user: null, assistants: [], tools: [], system: null };
      current.assistants.push(msg);
    } else if (msg.role === "tool") {
      if (!current) current = { id: msg.id, user: null, assistants: [], tools: [], system: null };
      current.tools.push(msg);
    } else if (msg.role === "system") {
      if (!current) current = { id: msg.id, user: null, assistants: [], tools: [], system: null };
      current.system = msg;
    }
  }
  if (current) result.push(current);
  return result;
});

/** 把当前草稿 + 附件组装成发送载荷（与首条消息共用于 utils/attachments） */
function buildPayload(text: string): { text: string; images: ImageContent[] } {
  return buildPromptPayload(text, props.attachments, (k) => t(k));
}

function send() {
  if (!props.isRunning) return;
  const payload = buildPayload(inputText.value);
  if (!payload.text.trim() && payload.images.length === 0) return;
  emit("send", payload);
  inputText.value = "";
  emit("update:attachments", []);
}

function steer() {
  if (!props.isRunning) return;
  const payload = buildPayload(inputText.value);
  if (!payload.text.trim() && payload.images.length === 0) return;
  emit("steer", payload);
  inputText.value = "";
  emit("update:attachments", []);
}

function sendAndClose() {
  send();
  expanded.value = false;
}
</script>

<template>
  <div class="chat">
    <MessageTimeline
      :turns="turns"
      :is-streaming="isStreaming"
      :current-assistant-content="currentAssistantContent"
      :current-thinking="currentThinking"
      :tool-executions="toolExecutions"
    />
    <Composer
      v-model="inputText"
      :is-running="isRunning"
      :is-streaming="isStreaming"
      :outbox="outbox"
      :steering-queue="steeringQueue"
      :attachments="attachments"
      :current-model="currentModel"
      :vision-hint="visionHint"
      @send="send"
      @steer="steer"
      @abort="emit('abort')"
      @cancel-queued="emit('cancel-queued', $event)"
      @upgrade-queued="emit('upgrade-queued', $event)"
      @update:attachments="emit('update:attachments', $event)"
      @expand="expanded = true"
      @restart-pi="emit('restart-pi')"
    />
    <FullscreenEditor
      v-model="inputText"
      :open="expanded"
      :is-running="isRunning"
      :is-streaming="isStreaming"
      @send="sendAndClose"
      @abort="emit('abort')"
      @close="expanded = false"
    />
  </div>
</template>

<style scoped>
.chat { display:flex; flex-direction:column; height:100%; overflow:hidden; }
</style>
