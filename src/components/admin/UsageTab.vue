<script setup lang="ts">
import { ref, reactive, computed, onMounted, onUnmounted, watch } from "vue";
import type { Component } from "vue";
import { useI18n } from "vue-i18n";
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
import { StatCard, ChartCard } from "@piter/ui";
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

const { t, locale } = useI18n();

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
    error.value = t("admin.usageLoadFailed", { msg: `${e}` });
  } finally {
    loading.value = false;
  }
}

// Charts read design tokens from CSS so they follow the light/dark theme.
const themeColors = reactive({
  text: "#1d1d1f",
  textSecondary: "#8a8a8e",
  border: "#e4e4e2",
  accent: "#2f6fed",
  accentSoft: "#dfdfdd",
  panel: "#ffffff",
  chart: ["#4f8ff7", "#5fbf76", "#f3a64f", "#8c7cf7", "#ef6b73", "#38a89d"],
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
  themeColors.chart = [1, 2, 3, 4, 5, 6].map(
    (i) => read(`--chart-${i}`, themeColors.chart[i - 1]),
  );
}

onMounted(() => {
  fetchData();
  fetchBudget();
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
  return Number(value || 0).toLocaleString(locale.value);
}

function formatCompact(value: number): string {
  return new Intl.NumberFormat(locale.value, {
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
  return d.toLocaleDateString(locale.value, { month: "short", day: "numeric" });
}

// ─── Monthly budget card (0.2.0 P3) ──────────────────────────────────────
// 配置 + 状态走网关 REST（跨端可用）；金额单位为分（cents），输入框按美元显示。
// 进度条档位变色：<50 正常 / 50-80 黄 / 80-100 橙 / 100 红；未设置/未启用显示"未设置"。
const props = defineProps<{ brokerHttpUrl?: string }>();

const gatewayBase = computed(() => {
  const base = props.brokerHttpUrl ?? "";
  return base.endsWith("/") ? base : base ? `${base}/` : "";
});

interface BudgetConfig {
  budgetCents: number;
  resetDay: number;
  enabled: boolean;
}
interface BudgetStatus {
  used: number;
  budget: number;
  percent: number;
  tier: number;
  resetDay: number;
  cycleStart: string;
  cycleEnd: string;
}

const budgetConfig = ref<BudgetConfig | null>(null);
const budgetStatus = ref<BudgetStatus | null>(null);
const budgetLoading = ref(false);
const budgetSaving = ref(false);
const budgetError = ref("");
// 可编辑字段（金额以美元字符串存储，保存时转分）
const budgetDollars = ref("");
const budgetResetDay = ref(1);
const budgetEnabled = ref(false);

async function fetchBudget() {
  if (!gatewayBase.value) return;
  budgetLoading.value = true;
  budgetError.value = "";
  try {
    const [cfgRes, statusRes] = await Promise.all([
      fetch(`${gatewayBase.value}api/budget`),
      fetch(`${gatewayBase.value}api/budget/status`),
    ]);
    const cfg = await cfgRes.json();
    const status = await statusRes.json();
    budgetConfig.value = {
      budgetCents: Number(cfg.budgetCents) || 0,
      resetDay: Number(cfg.resetDay) || 1,
      enabled: !!cfg.enabled,
    };
    budgetDollars.value = String((budgetConfig.value.budgetCents / 100) || "");
    budgetResetDay.value = budgetConfig.value.resetDay;
    budgetEnabled.value = budgetConfig.value.enabled;
    budgetStatus.value = status;
  } catch (e) {
    budgetError.value = t("admin.budgetLoadFailed", { msg: `${e}` });
  } finally {
    budgetLoading.value = false;
  }
}

async function saveBudget() {
  if (!gatewayBase.value) return;
  budgetSaving.value = true;
  budgetError.value = "";
  try {
    const cents = Math.round((Number(budgetDollars.value) || 0) * 100);
    const res = await fetch(`${gatewayBase.value}api/budget`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        budget_cents: cents,
        reset_day: budgetResetDay.value,
        enabled: budgetEnabled.value,
      }),
    });
    const data = await res.json();
    if (data.success !== true) throw new Error(data.error ?? "save failed");
    await fetchBudget();
  } catch (e) {
    budgetError.value = t("admin.budgetSaveFailed", { msg: `${e}` });
  } finally {
    budgetSaving.value = false;
  }
}

/** 已启用且预算 > 0 → 显示进度条；否则"未设置" */
const budgetConfigured = computed(
  () => !!budgetConfig.value?.enabled && (budgetConfig.value?.budgetCents ?? 0) > 0,
);

/** 已用金额（分 → 美元，进度条显示用） */
const budgetUsed = computed(() => (budgetStatus.value?.used ?? 0) / 100);

const budgetPercent = computed(() => {
  const p = budgetStatus.value?.percent ?? 0;
  return Math.min(100, Math.max(0, p));
});

const budgetBarColor = computed(() => {
  const tier = budgetStatus.value?.tier ?? 0;
  if (tier >= 3) return "var(--danger)"; // 100%
  if (tier === 2) return "#f97316"; // 80-100%（橙）
  if (tier === 1) return "var(--warning)"; // 50-80%（黄）
  return "var(--accent)";
});

/** 距下一个周期起点（重置日）的天数 */
const budgetResetLabel = computed(() => {
  const end = budgetStatus.value?.cycleEnd;
  if (!end) return "";
  const days = Math.ceil((Date.parse(end) - Date.now()) / 86_400_000);
  return days <= 0 ? t("admin.budgetResetToday") : t("admin.budgetResetIn", { n: days });
});

// ─── Overview cards ───────────────────────────────────────────────────────

interface StatCard {
  title: string;
  value: string;
  tone: string;
  icon: Component;
}

// Tone → chart palette slot (index into themeColors.chart).
const OVERVIEW_TONE_KEYS: Record<string, number> = {
  green: 2,
  blue: 1,
  violet: 4,
  amber: 3,
  teal: 6,
  rose: 5,
};

const statCards = computed<StatCard[]>(() => {
  const o = payload.value?.overview;
  if (!o) return [];
  return [
    { title: t("admin.totalCost"), value: formatUsd(o.total_cost), tone: "green", icon: DollarSign },
    { title: t("admin.sessions"), value: formatInt(o.sessions), tone: "blue", icon: MessagesSquare },
    { title: t("admin.messages"), value: formatInt(o.messages), tone: "violet", icon: Zap },
    { title: t("admin.totalTokens"), value: formatCompact(o.total_tokens), tone: "teal", icon: Coins },
    { title: t("admin.activeDays"), value: formatInt(o.active_days), tone: "amber", icon: CalendarDays },
    { title: t("admin.currentStreak"), value: `${formatInt(o.current_streak)}d`, tone: "blue", icon: Flame },
    { title: t("admin.longestStreak"), value: `${formatInt(o.longest_streak)}d`, tone: "violet", icon: Trophy },
    { title: t("admin.input"), value: formatCompact(o.input_tokens), tone: "teal", icon: ArrowUpDown },
    { title: t("admin.output"), value: formatCompact(o.output_tokens), tone: "green", icon: TrendingUp },
    { title: t("admin.cacheRead"), value: formatCompact(o.cache_read), tone: "amber", icon: Database },
    { title: t("admin.cacheWrite"), value: formatCompact(o.cache_write), tone: "violet", icon: Layers },
    { title: t("admin.toolCalls"), value: formatInt(o.tool_calls), tone: "rose", icon: Wrench },
  ];
});

function toneColor(tone: string): string {
  const idx = OVERVIEW_TONE_KEYS[tone];
  return idx ? themeColors.chart[idx - 1] : themeColors.chart[0];
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
      formatter: (p: any) => `${p.data[0]}: ${formatCompact(p.data[1] ?? 0)} ${t("admin.tokens")}`,
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
      monthLabel: { color: c.textSecondary, fontSize: 10, nameMap: locale.value.startsWith("zh") ? "ZH" : "EN" },
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

const MODEL_PALETTE = computed(() => themeColors.chart.slice(0, 3));

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
      valueFormatter: (value: any) => `${formatCompact(value)} ${t("admin.tokens")}`,
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

const TOOL_PALETTE = computed(() => [...themeColors.chart]);

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
          itemStyle: { color: TOOL_PALETTE.value[i % TOOL_PALETTE.value.length] },
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
  return t("admin.funNote", { n: ratio });
});

const hasData = computed(() => (payload.value?.overview.sessions ?? 0) > 0);
</script>

<template>
  <div class="tab-content usage-tab">
    <div class="tab-header">
      <div class="tab-header-info">
        <h3 class="tab-title">{{ $t("admin.usage") }}</h3>
        <p class="tab-desc">{{ $t("admin.usageDesc") }}</p>
      </div>
      <div class="tab-header-actions">
        <div class="range-chips" role="group" :aria-label="$t('admin.timeRange')">
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
          {{ loading ? $t("common.loading") : $t("admin.refresh") }}
        </button>
      </div>
    </div>

    <div v-if="error" class="usage-error">{{ error }}</div>

    <!-- Monthly budget card -->
    <ChartCard class="usage-section" :title="$t('admin.budget')" :subtitle="$t('admin.budgetDesc')">
      <div v-if="!gatewayBase" class="usage-empty">{{ $t("admin.budgetGatewayHint") }}</div>
      <template v-else>
        <div v-if="budgetError" class="usage-error">{{ budgetError }}</div>
        <div class="budget-row">
          <!-- Editor -->
          <div class="budget-form">
            <div class="budget-field">
              <span class="budget-label">{{ $t("admin.budgetAmount") }}</span>
              <div class="budget-input-wrap">
                <span class="budget-currency">$</span>
                <input
                  v-model="budgetDollars"
                  type="number"
                  min="0"
                  step="0.01"
                  class="budget-input"
                  :placeholder="$t('admin.budgetAmountPlaceholder')"
                  :disabled="budgetSaving"
                />
              </div>
            </div>
            <div class="budget-field">
              <span class="budget-label">{{ $t("admin.budgetResetDay") }}</span>
              <input
                v-model.number="budgetResetDay"
                type="number"
                min="1"
                max="31"
                class="budget-input budget-input--day"
                :disabled="budgetSaving"
              />
              <span class="budget-hint">{{ $t("admin.budgetResetDayDesc") }}</span>
            </div>
            <label class="budget-toggle">
              <input v-model="budgetEnabled" type="checkbox" :disabled="budgetSaving" />
              <span>{{ $t("admin.budgetEnabled") }}</span>
            </label>
            <div class="budget-actions">
              <button class="btn btn-sm" :disabled="budgetSaving || budgetLoading" @click="saveBudget">
                {{ budgetSaving ? $t("common.saving") : $t("common.save") }}
              </button>
            </div>
          </div>

          <!-- Gauge -->
          <div class="budget-gauge">
            <template v-if="budgetConfigured">
              <div class="budget-gauge-head">
                <span class="budget-used">
                  {{ $t("admin.budgetUsed") }} {{ formatUsd(budgetUsed) }}
                  <span class="budget-of">{{ $t("admin.budgetOf") }} {{ formatUsd((budgetStatus?.budget ?? 0) / 100) }}</span>
                </span>
                <span class="budget-percent" :style="{ color: budgetBarColor }">
                  {{ Math.round(budgetStatus?.percent ?? 0) }}%
                </span>
              </div>
              <div class="budget-bar">
                <div
                  class="budget-bar-fill"
                  :style="{ width: budgetPercent + '%', background: budgetBarColor }"
                ></div>
              </div>
              <div class="budget-reset">{{ budgetResetLabel }}</div>
            </template>
            <div v-else-if="budgetLoading" class="usage-loading">{{ $t("admin.loadingUsage") }}</div>
            <div v-else class="usage-empty">{{ $t("admin.budgetUnset") }}</div>
          </div>
        </div>
      </template>
    </ChartCard>

    <template v-if="payload">
      <div v-if="!hasData" class="usage-empty">{{ $t("admin.noUsageData") }}</div>

      <template v-else>
        <!-- Overview cards -->
        <div class="overview-grid">
          <StatCard
            v-for="card in statCards"
            :key="card.title"
            :title="card.title"
            :value="card.value"
            :icon="card.icon"
            :tone="toneColor(card.tone)"
          />
        </div>

        <!-- Models (range-sensitive, placed up top for quick feedback) -->
        <ChartCard class="usage-section" :title="$t('admin.models')" :subtitle="$t('admin.dailyTokenSplit')">
          <VChart class="models-chart" :option="modelsOption" autoresize />
          <div class="models-legend">
            <div v-for="(m, index) in topModels" :key="m.name" class="legend-row">
              <div class="legend-main">
                <span class="legend-dot" :style="{ background: MODEL_PALETTE[index % MODEL_PALETTE.length] }"></span>
                <span class="legend-name">{{ m.name }}</span>
              </div>
              <div class="legend-meta">
                <span>{{ $t("admin.inOut", { input: formatCompact(m.input_tokens), output: formatCompact(m.output_tokens) }) }}</span>
                <span>{{ Math.round((m.fraction || 0) * 100) }}%</span>
              </div>
            </div>
          </div>
        </ChartCard>

        <!-- Activity heatmap -->
        <ChartCard class="usage-section" :title="$t('admin.activity')" :subtitle="$t('admin.activityDesc')">
          <div class="heatmap-wrap">
            <VChart class="heatmap-chart" :option="heatmapOption" autoresize />
          </div>
          <p class="overview-note">{{ funNote }}</p>
        </ChartCard>

        <!-- Tool cost / Projects -->
        <div class="usage-columns">
          <ChartCard class="usage-section" :title="$t('admin.toolCost')" :subtitle="$t('admin.tracked', { n: payload.usage.tools.length })">
            <template v-if="payload.usage.tools.length">
              <div class="tool-chart-layout">
                <VChart class="doughnut" :option="toolsOption" autoresize />
                <div class="tool-legend">
                  <div v-for="(t, index) in topTools" :key="t.name" class="legend-row tool-row">
                    <div class="legend-main">
                      <span class="legend-dot" :style="{ background: TOOL_PALETTE[index % TOOL_PALETTE.length] }"></span>
                      <div class="tool-legend-text">
                        <div class="tool-legend-title">{{ t.name }}</div>
                        <div class="tool-legend-subtitle">{{ $t("admin.calls", { n: formatInt(t.count) }) }}</div>
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
                        <div class="tool-legend-title">{{ $t("admin.otherTools", { n: otherTools.length }) }}</div>
                        <div class="tool-legend-subtitle">{{ $t("admin.calls", { n: formatInt(otherToolCalls) }) }}</div>
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
            <div v-else class="usage-empty">{{ $t("admin.noToolUsage") }}</div>
          </ChartCard>

          <ChartCard class="usage-section" :title="$t('admin.projects')" :subtitle="$t('admin.byCost')">
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
              <div v-if="!topProjects.length" class="usage-empty">{{ $t("admin.noProjectUsage") }}</div>
            </div>
          </ChartCard>
        </div>

        <!-- Sessions -->
        <ChartCard class="usage-section" :title="$t('admin.sessions')" :subtitle="$t('admin.recentSessions')">
          <div class="sessions-table-wrap">
            <table class="sessions-table">
              <thead>
                <tr>
                  <th>{{ $t("admin.colSession") }}</th>
                  <th>{{ $t("admin.colModel") }}</th>
                  <th class="num">{{ $t("admin.colTokens") }}</th>
                  <th class="num">{{ $t("admin.colTools") }}</th>
                  <th class="num">{{ $t("admin.colCost") }}</th>
                  <th>{{ $t("admin.colDate") }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(s, i) in payload.sessions" :key="i">
                  <td class="td-title">
                    <div class="session-title">{{ s.title || $t("common.untitled") }}</div>
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
        </ChartCard>
      </template>
    </template>

    <div v-else-if="loading" class="usage-loading">{{ $t("admin.loadingUsage") }}</div>
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

/* ── Sections ── */
.usage-section {
  margin-bottom: var(--space-lg);
}

/* ── Budget card ── */
.budget-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: var(--space-lg);
  align-items: start;
}

.budget-form {
  display: grid;
  gap: var(--space-sm);
}

.budget-field {
  display: grid;
  gap: 4px;
}

.budget-label {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

.budget-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.budget-currency {
  position: absolute;
  left: 10px;
  font-size: var(--font-size-control);
  color: var(--text-tertiary);
  pointer-events: none;
}

.budget-input {
  width: 100%;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-panel);
  color: var(--text);
  font-size: var(--font-size-control);
  font-variant-numeric: tabular-nums;
}

.budget-input-wrap .budget-input {
  padding-left: 22px;
}

.budget-input--day {
  width: 72px;
}

.budget-input:focus {
  outline: none;
  border-color: var(--accent);
}

.budget-hint {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
  line-height: 1.4;
}

.budget-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: var(--font-size-control);
  color: var(--text);
  cursor: pointer;
}

.budget-actions {
  margin-top: var(--space-xs);
}

.budget-gauge {
  display: grid;
  gap: var(--space-sm);
  min-width: 0;
}

.budget-gauge-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-md);
}

.budget-used {
  font-size: var(--font-size-control);
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.budget-of {
  font-size: var(--font-size-micro);
  color: var(--text-tertiary);
}

.budget-percent {
  font-size: var(--font-size-title);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.budget-bar {
  height: 8px;
  border-radius: 999px;
  background: var(--bg-muted);
  overflow: hidden;
}

.budget-bar-fill {
  height: 100%;
  border-radius: 999px;
  transition: width 0.3s var(--ease);
}

.budget-reset {
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
  .budget-row {
    grid-template-columns: 1fr;
  }
}
</style>
