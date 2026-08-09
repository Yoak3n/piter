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
import {
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
import type { CostDashboard, CostToolStat, CostModelStat, CostProjectStat } from "./useAdmin";

// Overview stat card payload (icon 为 lucide 组件；tone 映射到主题 chart 色板)
interface StatCard {
  title: string;
  value: string;
  tone: string;
  icon: Component;
}

// ─── 用量统计（UsageTab）：数据拉取 + ECharts option + 格式化 ─────────
// 纯逻辑 composable：图表 option 全部按主题 CSS 变量实时读取（跟随明暗切换），
// 页面组件只负责把 option 交给 <VChart> 渲染。

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

export const RANGE_OPTIONS = ["7d", "30d", "90d"] as const;
export type RangeKey = (typeof RANGE_OPTIONS)[number];

export function useUsageStats() {
  const { t, locale } = useI18n();

  const range = ref<RangeKey>(loadRange());
  const payload = ref<CostDashboard | null>(null);
  const loading = ref(false);
  const error = ref("");

  function loadRange(): RangeKey {
    try {
      const saved = localStorage.getItem("piter-usage-range");
      if (saved && (RANGE_OPTIONS as readonly string[]).includes(saved)) {
        return saved as RangeKey;
      }
    } catch {
      // localStorage unavailable — default to 7d
    }
    return "7d";
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

  // ─── Formatting ───────────────────────────────────────────────────────

  function formatUsd(value: number): string {
    return `$${value.toFixed(2)}`;
  }

  function formatInt(value: number): string {
    return value.toLocaleString();
  }

  function formatCompact(value: number): string {
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
    return `${value}`;
  }

  function formatDayLabel(key: string): string {
    const [, m, d] = key.split("-");
    return `${m}/${d}`;
  }

  function formatSessionDate(time: string): string {
    const d = new Date(time);
    return d.toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  }

  // ─── Overview cards ───────────────────────────────────────────────────

  const OVERVIEW_TONE_KEYS: Record<string, number> = {
    blue: 1,
    green: 2,
    amber: 3,
    purple: 4,
    red: 5,
    teal: 6,
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

  // ─── Activity heatmap (ECharts calendar) ─────────────────────────────

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

  // ─── Models chart (stacked bars) ─────────────────────────────────────

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

  // ─── Tool cost doughnut ───────────────────────────────────────────────

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
    const r = parseInt(h.slice(0, 2), 16);
    const g = parseInt(h.slice(2, 4), 16);
    const b = parseInt(h.slice(4, 6), 16);
    return `rgba(${r},${g},${b},${alpha})`;
  }

  // ─── Projects ─────────────────────────────────────────────────────────

  const topProjects = computed<CostProjectStat[]>(() => (payload.value?.projects ?? []).slice(0, 6));

  const projectTotalCost = computed(() =>
    topProjects.value.reduce((s, p) => s + Number(p.cost || 0), 0)
  );

  function projectPercent(p: CostProjectStat): number {
    return projectTotalCost.value > 0
      ? Math.round((Number(p.cost || 0) / projectTotalCost.value) * 100)
      : 0;
  }

  // ─── Fun note ─────────────────────────────────────────────────────────

  const funNote = computed(() => {
    const total = payload.value?.overview.total_tokens ?? 0;
    const warAndPeace = 587000;
    const ratio = Math.max(1, Math.round(total / warAndPeace));
    return t("admin.funNote", { n: ratio });
  });

  const hasData = computed(() => (payload.value?.overview.sessions ?? 0) > 0);

  // ─── 生命周期：初始拉取 + 主题监听 + range 持久化 ───────────────────

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

  return {
    RANGE_OPTIONS,
    range,
    payload,
    loading,
    error,
    fetchData,
    formatUsd,
    formatInt,
    formatCompact,
    formatSessionDate,
    toneColor,
    statCards,
    heatmapOption,
    MODEL_PALETTE,
    topModels,
    modelsOption,
    TOOL_PALETTE,
    topTools,
    toolTotalCost,
    otherTools,
    otherToolCost,
    otherToolCalls,
    otherToolPercent,
    toolsOption,
    topProjects,
    projectPercent,
    funNote,
    hasData,
  };
}
