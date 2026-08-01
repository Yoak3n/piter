<script setup lang="ts">
import { ref, computed } from "vue";
import type { Message, ToolExecution, ChatTurn } from "../../types";
import type { PendingItem } from "../../composables/usePiConnection";
import MessageTimeline from "./MessageTimeline.vue";
import Composer from "./Composer.vue";
import FullscreenEditor from "./FullscreenEditor.vue";

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
}>();

const emit = defineEmits<{
  (e: "send", text: string): void;
  (e: "steer", text: string): void;
  (e: "abort"): void;
  (e: "cancel-queued", id: number): void;
  (e: "upgrade-queued", id: number): void;
  (e: "update:draft", text: string): void;
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

function send() {
  const text = inputText.value.trim();
  if (!text || !props.isRunning) return;
  emit("send", text);
  inputText.value = "";
}

function steer() {
  const text = inputText.value.trim();
  if (!text || !props.isRunning) return;
  emit("steer", text);
  inputText.value = "";
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
      @send="send"
      @steer="steer"
      @abort="emit('abort')"
      @cancel-queued="emit('cancel-queued', $event)"
      @upgrade-queued="emit('upgrade-queued', $event)"
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
