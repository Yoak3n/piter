<script setup lang="ts">
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Message, ToolExecution, ChatTurn, Attachment, ImageContent, ModelRef, SlashCommand } from "../../types";
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
  slashCommands?: SlashCommand[] | null;
  /** 跨会话搜索跳转目标：切到该会话后滚动定位到对应消息 */
  scrollTarget?: { sessionId: string; timestamp?: number; query: string } | null;
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
  (e: "respond-extension", payload: { id: string; answer: { value?: string; confirmed?: boolean; cancelled?: boolean } }): void;
  (e: "fetch-slash-commands"): void;
  (e: "scroll-handled"): void;
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
  const fresh = (id: number): ChatTurn => ({ id, user: null, assistants: [], tools: [], system: [] });
  for (const msg of props.messages) {
    if (msg.role === "user") {
      if (current) result.push(current);
      current = { id: msg.id, user: msg, assistants: [], tools: [], system: [] };
    } else if (msg.role === "assistant") {
      if (!current) current = fresh(msg.id);
      current.assistants.push(msg);
    } else if (msg.role === "tool") {
      if (!current) current = fresh(msg.id);
      current.tools.push(msg);
    } else if (msg.role === "system") {
      if (!current) current = fresh(msg.id);
      // 多条 system 提示 / 扩展 UI 卡片并存，不再覆盖式赋值
      current.system.push(msg);
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
      :scroll-to="scrollTarget"
      @respond-extension="emit('respond-extension', $event)"
      @scroll-handled="emit('scroll-handled')"
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
      :slash-commands="slashCommands"
      @send="send"
      @steer="steer"
      @abort="emit('abort')"
      @cancel-queued="emit('cancel-queued', $event)"
      @upgrade-queued="emit('upgrade-queued', $event)"
      @update:attachments="emit('update:attachments', $event)"
      @expand="expanded = true"
      @restart-pi="emit('restart-pi')"
      @fetch-slash-commands="emit('fetch-slash-commands')"
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
