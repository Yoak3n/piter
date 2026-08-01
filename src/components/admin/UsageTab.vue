<script setup lang="ts">
import { ref, reactive, computed, onMounted, onUnmounted, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { use } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { BarChart, HeatmapChart, PieChart } from "echarts/charts";
import {
  CalendarComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  VisualMapComponent,
} from "echarts/components";
import VChart from "vue-echarts";
import {
  RefreshCw,
  DollarSign,
  MessagesSquare,
  Zap,
  Coins,
  CalendarDays,
  Flame,
  Trophy,
  ArrowUpDown,
  TrendingUp,
  Database,
  Layers,
  Wrench,
} from "lucide-vue-next";
import type { CostDashboard, CostToolStat, CostModelStat, CostProjectStat } from "../../composables/useAdmin";

// Register only the ECharts modules this page uses (tree-shaken bundle).
use([
  CanvasRenderer,
  BarChart,
  HeatmapChart,
  PieChart,
  CalendarComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  VisualMapComponent,
]);

const RANGE_OPTIONS = ["7d", "30d", "90d"] as const;
type RangeKey = (typeof RANGE_OPTIONS)[number];

// ─── State ────────────────────────────────────────────────────────────────

const range = ref<RangeKey>(loadRange());
const payload = ref<CostDashboard | null>(null);
const loading = ref(false);
const error = ref("");

function loadRange(): RangeKey {
  try {
    const saved = localStorage.getItem("piter-usage-range");
    if (saved === "7d" || saved === "90d") return saved;
  } catch {}
  return "30d";
}

async function fetchData() {
  loading.value = true;
  error.value = "";
  try {
    payload.value = await invoke<CostDashboard>("get_cost_dashboard", {
      range: range.value,
      granularity: "day",
      scope: "all",
    });
  } catch (e) {
    error.value = `Failed to load usage dashboard: ${e}`;
  } finally {
    loading.value = false;
  }
}

// Charts read design tokens from CSS so they follow the light/dark theme.
const themeColors = reactive({
  text: "#1d1d1f",
  textSecondary: "#8a8a8e",
  border: "#dfdeda",
  accent: "#6a7a8a",
  accentSoft: "#d8d7d2",
  panel: "#ffffff",
});
let themeObserver: MutationObserver | null = null;

function readThemeColors() {
  const cs = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) =>
    cs.getPropertyValue(name).trim() || fallback;
  themeColors.text = read("--text", themeColors.text);
  themeColors.textSecondary = read("--text-secondary", themeColors.textSecondary);
  themeColors.border = read("--border", themeColors.border);
  themeColors.accent = read("--accent", themeColors.accent);
  themeColors.accentSoft = read("--bg-active", themeColors.accentSoft);
  themeColors.panel = read("--bg-panel", themeColors.panel);
}

onMounted(() => {
  fetchData();
  readThemeColors();
  themeObserver = new MutationObserver(readThemeColors);
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
});
onUnmounted(() => themeObserver?.disconnect());

watch(range, () => {
  localStorage.setItem("piter-usage-range", range.value);
  fetchData();
});

// ─── Formatting ───────────────────────────────────────────────────────────

function formatUsd(value: number): string {
  return `$${(Number(value) || 0).toFixed(2)}`;
}

function formatInt(value: number): string {
  return Number(value || 0).toLocaleString();
}

function formatCompact(value: number): string {
  return new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(Number(value) || 0);
}

function formatDayLabel(key: string): string {
  const [, m, d] = key.split("-");
  return `${m}-${d}`;
}

function formatSessionDate(time: string): string {
  if (!time) return "";
  const d = new Date(time);
  if (!Number.isFinite(d.getTime())) return "";
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

// ─── Overview cards ───────────────────────────────────────────────────────

interface StatCard {
  title: string;
  value: string;
  tone: string;
  icon: unknown;
}

const OVERVIEW_TONES: Record<string, string> = {
  green: "#5fbf76",
  blue: "#5b8def",
  violet: "#8e6ad8",
  amber: "#d39a46",
  teal: "#38a89d",
  rose: "#cf6a7a",
};

const statCards = computed<StatCard[]>(() => {
  const o = payload.value?.overview;
  if (!o) return [];
  return [
    { title: "Total cost", value: formatUsd(o.total_cost), tone: "green", icon: DollarSign },
    { title: "Sessions", value: formatInt(o.sessions), tone: "blue", icon: MessagesSquare },
    { title: "Messages", value: formatInt(o.messages), tone: "violet", icon: Zap },
    { title: "Total tokens", value: formatCompact(o.total_tokens), tone: "teal", icon: Coins },
    { title: "Active days", value: formatInt(o.active_days), tone: "amber", icon: CalendarDays },
    { title: "Current streak", value: `${formatInt(o.current_streak)}d`, tone: "blue", icon: Flame },
    { title: "Longest streak", value: `${formatInt(o.longest_streak)}d`, tone: "violet", icon: Trophy },
    { title: "Input", value: formatCompact(o.input_tokens), tone: "teal", icon: ArrowUpDown },
    { title: "Output", value: formatCompact(o.output_tokens), tone: "green", icon: TrendingUp },
    { title: "Cache Read", value: formatCompact(o.cache_read), tone: "amber", icon: Database },
    { title: "Cache Write", value: formatCompact(o.cache_write), tone: "violet", icon: Layers },
    { title: "Tool Calls", value: formatInt(o.tool_calls), tone: "rose", icon: Wrench },
  ];
});

function toneColor(tone: string): string {
  return OVERVIEW_TONES[tone] ?? "#5b8def";
}

// ─── Activity heatmap (ECharts calendar) ─────────────────────────────────

const heatmapOption = computed(() => {
  const days = payload.value?.activity ?? [];
  const max = Math.max(...days.map((d) => d.value), 0);
  const c = themeColors;
  if (days.length === 0) return { series: [] };
  return {
    tooltip: {
      backgroundColor: c.panel,
      borderColor: c.border,
      textStyle: { color: c.text, fontSize: 12 },
      formatter: (p: any) => `${p.data[0]}: ${formatCompact(p.data[1] ?? 0)} tokens`,
    },
    visualMap: {
      min: 0,
      max: max || 1,
      show: false,
      inRange: { color: [c.accentSoft, hexToRgba(c.accent, 0.45), c.accent] },
    },
    calendar: {
      top: 10,
      left: 10,
      right: 10,
      bottom: 10,
      cellSize: ["auto", 13],
      range: [days[0].key, days[days.length - 1].key],
      itemStyle: { color: c.accentSoft, borderWidth: 3, borderColor: c.panel },
      splitLine: { show: false },
      yearLabel: { show: false },
      dayLabel: { show: false },
      monthLabel: { color: c.textSecondary, fontSize: 10, nameMap: "EN" },
    },
    series: [
      {
        type: "heatmap",
        coordinateSystem: "calendar",
        data: days.map((d) => [d.key, d.value]),
      },
    ],
  };
});

// ─── Models chart (stacked bars) ─────────────────────────────────────────

const MODEL_PALETTE = ["#5b8def", "#67c587", "#f3a64f"];

const topModels = computed<CostModelStat[]>(() => (payload.value?.models ?? []).slice(0, 3));

const dailySeries = computed(() => {
  const daily = payload.value?.daily ?? [];
  const names = topModels.value.map((m) => m.name);
  return {
    labels: daily.map((d) => d.key),
    series: names.map((name) => daily.map((d) => d.models?.[name] ?? 0)),
  };
});

const modelsOption = computed(() => {
  const { labels, series } = dailySeries.value;
  const names = topModels.value.map((m) => m.name);
  const c = themeColors;
  return {
    grid: { left: 56, right: 16, top: 14, bottom: 34 },
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow" },
      backgroundColor: c.panel,
      borderColor: c.border,
      textStyle: { color: c.text, fontSize: 12 },
      valueFormatter: (value: any) => `${formatCompact(value)} tokens`,
    },
    legend: {
      bottom: 0,
      left: "center",
      itemWidth: 10,
      itemHeight: 10,
      icon: "circle",
      textStyle: { color: c.textSecondary, fontSize: 12 },
      data: names,
    },
    xAxis: {
      type: "category",
      data: labels.map(formatDayLabel),
      boundaryGap: true,
      axisLine: { lineStyle: { color: c.border } },
      axisTick: { show: false },
      axisLabel: { color: c.textSecondary, fontSize: 10, hideOverlap: true },
    },
    yAxis: {
      type: "value",
      axisLabel: {
        color: c.textSecondary,
        fontSize: 10,
        formatter: (value: any) => formatCompact(value),
      },
      splitLine: { lineStyle: { color: c.border } },
    },
    series: names.map((name, i) => ({
      name,
      type: "bar",
      stack: "total",
      barMaxWidth: 26,
      data: series[i],
      emphasis: { focus: "series" },
    })),
  };
});

// ─── Tool cost doughnut ───────────────────────────────────────────────────

const TOOL_PALETTE = ["#4f8ff7", "#67c587", "#f3a64f", "#8c7cf7", "#ef6b73", "#4fc3d9"];

const topTools = computed<CostToolStat[]>(() => (payload.value?.usage.tools ?? []).slice(0, 6));

const toolTotalCost = computed(() =>
  (payload.value?.usage.tools ?? []).reduce((s, t) => s + Number(t.cost || 0), 0)
);

// Tools beyond the top 6 — they render as the grey doughnut remainder.
const otherTools = computed<CostToolStat[]>(() => (payload.value?.usage.tools ?? []).slice(6));
const otherToolCost = computed(() =>
  otherTools.value.reduce((s, t) => s + Number(t.cost || 0), 0)
);
const otherToolCalls = computed(() =>
  otherTools.value.reduce((s, t) => s + Number(t.count || 0), 0)
);
const otherToolPercent = computed(() =>
  toolTotalCost.value > 0
    ? Math.round((otherToolCost.value / toolTotalCost.value) * 100)
    : 0
);

const toolsOption = computed(() => {
  const c = themeColors;
  return {
    tooltip: {
      trigger: "item",
      backgroundColor: c.panel,
      borderColor: c.border,
      textStyle: { color: c.text, fontSize: 12 },
      formatter: (p: any) => `${p.name}: ${formatUsd(p.value)}`,
    },
    series: [
      {
        type: "pie",
        radius: ["55%", "74%"],
        center: ["50%", "50%"],
        data: topTools.value.map((t, i) => ({
          name: t.name,
          value: t.cost,
          itemStyle: { color: TOOL_PALETTE[i % TOOL_PALETTE.length] },
        })),
        label: { show: false },
        emphasis: { scaleSize: 5 },
      },
    ],
  };
});

function hexToRgba(hex: string, alpha: number): string {
  const h = hex.replace("#", "");
  if (h.length !== 6) return hex;
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

// ─── Projects ─────────────────────────────────────────────────────────────

const topProjects = computed<CostProjectStat[]>(() => (payload.value?.projects ?? []).slice(0, 6));

const projectTotalCost = computed(() =>
  topProjects.value.reduce((s, p) => s + Number(p.cost || 0), 0)
);

function projectPercent(p: CostProjectStat): number {
  return projectTotalCost.value > 0
    ? Math.round((Number(p.cost || 0) / projectTotalCost.value) * 100)
    : 0;
}

// ─── Fun note ─────────────────────────────────────────────────────────────

const funNote = computed(() => {
  const total = payload.value?.overview.total_tokens ?? 0;
  const warAndPeace = 587000;
  const ratio = Math.max(1, Math.round(total / warAndPeace));
  return `You've used ~${ratio}× more tokens than War and Peace.`;
});

const hasData = computed(() => (payload.value?.overview.sessions ?? 0) > 0);
</script>

<template>
  <div class="tab-content usage-tab">
    <div class="tab-header">
      <div class="tab-header-info">
        <h3 class="tab-title">Usage</h3>
        <p class="tab-desc">Cost and activity across pi sessions</p>
      </div>
      <div class="tab-header-actions">
        <div class="range-chips" role="group" aria-label="Time range">
          <button
            v-for="r in RANGE_OPTIONS"
            :key="r"
            class="range-chip"
            :class="{ active: range === r }"
            :aria-pressed="range === r"
            @click="range = r"
          >
            {{ r }}
          </button>
        </div>
        <button class="btn btn-sm" :disabled="loading" @click="fetchData">
          <RefreshCw :size="12" :class="{ spin: loading }" />
          {{ loading ? "Loading..." : "Refresh" }}
        </button>
      </div>
    </div>

    <div v-if="error" class="usage-error">{{ error }}</div>

    <template v-if="payload">
      <div v-if="!hasData" class="usage-empty">No usage data in the selected range.</div>

      <template v-else>
        <!-- Overview cards -->
        <div class="overview-grid">
          <article
            v-for="card in statCards"
            :key="card.title"
            class="stat-card"
            :style="{ '--card-tone': toneColor(card.tone) }"
          >
            <div class="stat-title">
              <span class="stat-icon"><component :is="card.icon" :size="13" /></span>
              <span>{{ card.title }}</span>
            </div>
            <div class="stat-value">{{ card.value }}</div>
          </article>
        </div>

        <!-- Models (range-sensitive, placed up top for quick feedback) -->
        <section class="usage-section">
          <div class="usage-section-head">
            <h4>Models</h4>
            <span>Daily token split</span>
          </div>
          <div class="models-card">
            <VChart class="models-chart" :option="modelsOption" autoresize />
            <div class="models-legend">
              <div v-for="(m, index) in topModels" :key="m.name" class="legend-row">
                <div class="legend-main">
                  <span class="legend-dot" :style="{ background: MODEL_PALETTE[index % MODEL_PALETTE.length] }"></span>
                  <span class="legend-name">{{ m.name }}</span>
                </div>
                <div class="legend-meta">
                  <span>{{ formatCompact(m.input_tokens) }} in · {{ formatCompact(m.output_tokens) }} out</span>
                  <span>{{ Math.round((m.fraction || 0) * 100) }}%</span>
                </div>
              </div>
            </div>
          </div>
        </section>

        <!-- Activity heatmap -->
        <section class="usage-section">
          <div class="usage-section-head">
            <h4>Activity</h4>
            <span>Daily token intensity · last 365 days</span>
          </div>
          <div class="heatmap-wrap">
            <VChart class="heatmap-chart" :option="heatmapOption" autoresize />
          </div>
          <p class="overview-note">{{ funNote }}</p>
        </section>

        <!-- Tool cost / Projects -->
        <div class="usage-columns">
          <section class="usage-section">
            <div class="usage-section-head">
              <h4>Tool Cost</h4>
              <span>{{ payload.usage.tools.length }} tracked</span>
            </div>
            <div class="tool-cost-card">
              <template v-if="payload.usage.tools.length">
                <div class="tool-chart-layout">
                  <VChart class="doughnut" :option="toolsOption" autoresize />
                  <div class="tool-legend">
                    <div v-for="(t, index) in topTools" :key="t.name" class="legend-row tool-row">
                      <div class="legend-main">
                        <span class="legend-dot" :style="{ background: TOOL_PALETTE[index % TOOL_PALETTE.length] }"></span>
                        <div class="tool-legend-text">
                          <div class="tool-legend-title">{{ t.name }}</div>
                          <div class="tool-legend-subtitle">{{ formatInt(t.count) }} calls</div>
                        </div>
                      </div>
                      <div class="legend-values">
                        <span>{{ formatUsd(t.cost) }}</span>
                        <span>{{ Math.round((Number(t.cost || 0) / Math.max(toolTotalCost, 0.000001)) * 100) }}%</span>
                      </div>
                    </div>
                    <div v-if="otherTools.length" class="legend-row tool-row">
                      <div class="legend-main">
                        <span class="legend-dot" style="background: var(--bg-active)"></span>
                        <div class="tool-legend-text">
                          <div class="tool-legend-title">Other ({{ otherTools.length }} tools)</div>
                          <div class="tool-legend-subtitle">{{ formatInt(otherToolCalls) }} calls</div>
                        </div>
                      </div>
                      <div class="legend-values">
                        <span>{{ formatUsd(otherToolCost) }}</span>
                        <span>{{ otherToolPercent }}%</span>
                      </div>
                    </div>
                  </div>
                </div>
              </template>
              <div v-else class="usage-empty">No tool usage in range.</div>
            </div>
          </section>

          <section class="usage-section">
            <div class="usage-section-head">
              <h4>Projects</h4>
              <span>By cost</span>
            </div>
            <div class="projects-card">
              <div v-for="(p, index) in topProjects" :key="p.cwd" class="project-row">
                <div class="project-head">
                  <div class="project-main">
                    <span class="legend-dot" :style="{ background: TOOL_PALETTE[index % TOOL_PALETTE.length] }"></span>
                    <div class="project-text">
                      <div class="project-title">{{ p.name }}</div>
                      <div class="project-subtitle">{{ p.cwd }}</div>
                    </div>
                  </div>
                  <div class="project-values">
                    <span>{{ formatUsd(p.cost) }}</span>
                    <span>{{ projectPercent(p) }}%</span>
                  </div>
                </div>
                <div class="project-bar">
                  <div class="project-bar-fill" :style="{ width: projectPercent(p) + '%' }"></div>
                </div>
              </div>
              <div v-if="!topProjects.length" class="usage-empty">No project usage in range.</div>
            </div>
          </section>
        </div>

        <!-- Sessions -->
        <section class="usage-section">
          <div class="usage-section-head">
            <h4>Sessions</h4>
            <span>Recent sessions in range</span>
          </div>
          <div class="sessions-card">
            <div class="sessions-table-wrap">
              <table class="sessions-table">
                <thead>
                  <tr>
                    <th>Session</th>
                    <th>Model</th>
                    <th class="num">Tokens</th>
                    <th class="num">Tools</th>
                    <th class="num">Cost</th>
                    <th>Date</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="(s, i) in payload.sessions" :key="i">
                    <td class="td-title">
                      <div class="session-title">{{ s.title || "Untitled" }}</div>
                      <div v-if="s.workspace" class="session-workspace">{{ s.workspace }}</div>
                    </td>
                    <td class="td-model">{{ s.model }}</td>
                    <td class="num">{{ formatCompact(s.total_tokens) }}</td>
                    <td class="num">{{ formatInt(s.tool_calls) }}</td>
                    <td class="num td-cost">{{ formatUsd(s.total_cost) }}</td>
                    <td class="td-date">{{ formatSessionDate(s.time) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </section>
      </template>
    </template>

    <div v-else-if="loading" class="usage-loading">Loading usage data…</div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  max-width: 1080px;
}

.tab-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-md);
  margin-bottom: var(--space-lg);
  flex-wrap: wrap;
}

.tab-header-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0;
}

.tab-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
}

.tab-header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-md);
}

.range-chips {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px;
  background: var(--bg-muted);
  border-radius: var(--radius-md);
}

.range-chip {
  height: 22px;
  min-width: 40px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-secondary);
  font-size: var(--font-size-control);
  line-height: 1;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.range-chip:hover {
  color: var(--text);
}

.range-chip.active {
  background: var(--bg-panel);
  border-color: var(--border);
  color: var(--text);
  font-weight: 500;
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

.usage-error {
  padding: var(--space-sm) var(--space-md);
  background: var(--danger-soft);
  color: var(--danger);
  border-radius: var(--radius-sm);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-md);
}

.usage-loading {
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
  padding: var(--space-lg) 0;
}

.usage-empty {
  padding: var(--space-lg) 0;
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
}

/* ── Overview cards ── */
.overview-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--space-sm);
  margin-bottom: var(--space-lg);
}

.stat-card {
  padding: var(--space-md);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
}

.stat-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  line-height: 1.2;
}

.stat-icon {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  color: var(--card-tone);
  opacity: 0.8;
}

.stat-value {
  margin-top: var(--space-sm);
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.02em;
  line-height: 1;
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

/* ── Sections ── */
.usage-section {
  margin-bottom: var(--space-lg);
}

.usage-section-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-md);
  margin-bottom: var(--space-sm);
}

.usage-section-head h4 {
  margin: 0;
  font-size: var(--font-size-control);
  font-weight: 600;
  color: var(--text);
  letter-spacing: 0.01em;
}

.usage-section-head span {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

.usage-columns {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-md);
  align-items: start;
}

.usage-columns .usage-section {
  margin-bottom: 0;
}

/* ── Heatmap ── */
.heatmap-wrap {
  padding: var(--space-sm);
  background: var(--bg-muted);
  border-radius: var(--radius-md);
}

.heatmap-chart {
  width: 100%;
  height: 160px;
}

.overview-note {
  margin: var(--space-sm) 0 0;
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
}

/* ── Models chart ── */
.models-card,
.tool-cost-card,
.projects-card,
.sessions-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-panel);
  padding: var(--space-md);
}

.models-chart {
  width: 100%;
  height: 300px;
}

.models-legend {
  display: grid;
  gap: var(--space-sm);
  margin-top: var(--space-sm);
}

.legend-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-md);
  align-items: center;
}

.legend-main {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  min-width: 0;
}

.legend-dot {
  width: 9px;
  height: 9px;
  border-radius: 999px;
  flex-shrink: 0;
}

.legend-name {
  font-size: var(--font-size-control);
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.legend-meta,
.legend-values {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  font-size: var(--font-size-micro);
  color: var(--text-secondary);
  white-space: nowrap;
}

/* ── Tool cost ── */
.tool-chart-layout {
  display: flex;
  align-items: center;
  gap: var(--space-md);
}

.doughnut {
  width: 108px;
  height: 108px;
  flex-shrink: 0;
}

.tool-legend {
  flex: 1;
  min-width: 0;
  display: grid;
  gap: var(--space-sm);
}

.tool-legend-title {
  font-size: var(--font-size-control);
  color: var(--text);
  line-height: 1.15;
}

.tool-legend-subtitle {
  margin-top: 1px;
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

/* ── Projects ── */
.projects-card {
  display: grid;
  gap: var(--space-md);
}

.project-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-sm);
}

.project-main {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  min-width: 0;
}

.project-text {
  min-width: 0;
}

.project-title {
  font-size: var(--font-size-control);
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.project-subtitle {
  margin-top: 1px;
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 180px;
}

.project-values {
  display: flex;
  gap: var(--space-sm);
  font-size: var(--font-size-micro);
  color: var(--text-secondary);
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.project-bar {
  height: 4px;
  border-radius: 999px;
  background: var(--bg-muted);
  overflow: hidden;
}

.project-bar-fill {
  height: 100%;
  border-radius: 999px;
  background: var(--accent);
  opacity: 0.85;
}

/* ── Sessions table ── */
.sessions-table-wrap {
  overflow-x: auto;
}

.sessions-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--font-size-control);
}

.sessions-table th {
  padding: var(--space-xs) var(--space-sm);
  text-align: left;
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
  font-weight: 500;
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}

.sessions-table th.num,
.sessions-table td.num {
  text-align: right;
}

.sessions-table td {
  padding: var(--space-sm);
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  vertical-align: middle;
  white-space: nowrap;
}

.sessions-table tbody tr:last-child td {
  border-bottom: none;
}

.sessions-table tbody tr:hover {
  background: var(--bg-hover);
}

.td-title {
  min-width: 180px;
  max-width: 320px;
}

.session-title {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--text);
}

.session-workspace {
  margin-top: 2px;
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.td-model {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--text-secondary);
}

.td-cost {
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.td-date {
  color: var(--text-tertiary);
}

/* ── Responsive ── */
@media (max-width: 1100px) {
  .overview-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  .usage-columns {
    grid-template-columns: 1fr 1fr;
  }
}

@media (max-width: 760px) {
  .overview-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .usage-columns {
    grid-template-columns: 1fr;
  }
}
</style>
