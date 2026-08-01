<script setup lang="ts">
import { ref } from "vue";
import { ChevronRight } from "lucide-vue-next";
import type { ToolExecution } from "../../types";

defineProps<{
  tool: ToolExecution;
}>();

const expanded = ref(false);

function getArgsPreview(_toolName: string, args: Record<string, unknown>): string {
  if (!args || Object.keys(args).length === 0) return "";
  if (args.path) return String(args.path).substring(0, 80);
  if (args.command) return String(args.command).substring(0, 80);
  if (args.query) return String(args.query).substring(0, 60);
  if (args.url) return String(args.url);
  for (const val of Object.values(args)) {
    if (typeof val === "string" && val.length > 0) return val.substring(0, 60);
  }
  return "";
}

function formatArgs(args: Record<string, unknown>): string {
  try {
    if (Object.keys(args).length === 0) return "";
    return JSON.stringify(args, null, 2);
  } catch { return String(args); }
}
</script>

<template>
  <div class="tool-card" :class="tool.status">
    <div class="tool-card-header" @click="expanded = !expanded">
      <div class="tool-header-left">
        <ChevronRight :size="12" class="tool-chevron" :class="{ expanded }" />
        <span class="tool-name">{{ tool.toolName }}</span>
        <span v-if="getArgsPreview(tool.toolName, tool.args)" class="tool-args-preview">{{ getArgsPreview(tool.toolName, tool.args) }}</span>
      </div>
      <div class="tool-header-right">
        <span class="tool-status" :class="tool.status">{{ tool.status }}</span>
      </div>
    </div>
    <div v-if="expanded" class="tool-card-body">
      <div v-if="formatArgs(tool.args)" class="tool-args">{{ formatArgs(tool.args) }}</div>
      <div v-if="tool.output" class="tool-output">{{ tool.output }}</div>
    </div>
  </div>
</template>

<style scoped>
.tool-card {
  background:var(--color-bg-muted);
  border:1px solid var(--color-border-subtle);
  border-radius:10px;
  overflow:hidden;
  font-size:13px;
  transition:border-color 0.2s var(--ease);
}
.tool-card:hover { border-color:var(--color-border-strong); }
.tool-card-header {
  display:flex; justify-content:space-between; align-items:center;
  padding:8px 12px; cursor:pointer; user-select:none;
  transition:background 0.15s var(--ease);
}
.tool-card-header:hover { background:var(--color-bg-hover); }
.tool-header-left { display:flex; align-items:center; gap:8px; min-width:0; }
.tool-header-right { display:flex; align-items:center; gap:6px; flex-shrink:0; }
.tool-chevron { transition:transform 0.2s var(--ease); opacity:0.4; flex-shrink:0; }
.tool-chevron.expanded { transform:rotate(90deg); }
.tool-name { color:var(--color-accent); font-family:var(--font-family-mono); font-size:11px; }
.tool-args-preview { color:var(--color-text-tertiary); font-family:var(--font-family-mono); font-size:11px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:300px; }
.tool-status {
  font-size:10px; padding:2px 7px; border-radius:var(--radius-pill);
  text-transform:uppercase; letter-spacing:0.04em; flex-shrink:0;
  display:flex; align-items:center; gap:4px;
}
.tool-status.pending { background:var(--color-bg-panel); color:var(--color-text-tertiary); border:1px solid var(--color-border-subtle); }
.tool-status.pending::before { content:"○"; font-size:8px; }
.tool-status.streaming { background:var(--color-accent); color:#fff; animation:pulse 1.5s infinite; }
.tool-status.streaming::before { content:"●"; font-size:7px; }
.tool-status.complete { background:var(--success-soft, rgba(74,154,106,0.1)); color:var(--success); border:1px solid rgba(74,154,106,0.2); }
.tool-status.complete::before { content:"✓"; font-size:9px; }
.tool-status.error { background:var(--danger-soft, rgba(217,92,92,0.1)); color:var(--danger); border:1px solid rgba(217,92,92,0.2); }
.tool-status.error::before { content:"!"; font-size:9px; }
@keyframes pulse { 0%,100%{ opacity:1; } 50%{ opacity:0.7; } }
.tool-card-body { border-top:1px solid var(--color-border-subtle); }
.tool-args {
  background:rgba(0,0,0,0.06); padding:10px 12px;
  font-family:var(--font-family-mono); font-size:11px;
  overflow-x:auto; white-space:pre-wrap;
  border-bottom:1px solid var(--color-border-subtle);
}
[data-theme="dark"] .tool-args { background:rgba(0,0,0,0.2); }
.tool-output {
  padding:10px 12px; font-family:var(--font-family-mono); font-size:11px;
  white-space:pre-wrap; overflow-x:auto; max-height:300px; overflow-y:auto;
}

@media (max-width: 640px) {
  .tool-args-preview { max-width:140px; }
}
</style>
