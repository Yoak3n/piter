<script setup lang="ts">
import { computed } from "vue";
import { RefreshCw } from "lucide-vue-next";
import { StatCard, ChartCard } from "@piter/ui";
import VChart from "vue-echarts";
import { useUsageStats } from "../../composables/useUsageStats";
import { useBudgetConfig } from "../../composables/useBudgetConfig";

// ─── 用量统计 Tab ─────────────────────────────────────────────────────
// 数据/图表 option/格式化在 useUsageStats；预算卡在 useBudgetConfig。
// 本组件只做模板组装（图表渲染 + 图例 + 会话表）。

const props = defineProps<{ brokerHttpUrl?: string }>();

const gatewayBase = computed(() => {
  const base = props.brokerHttpUrl ?? "";
  return base.endsWith("/") ? base : base ? `${base}/` : "";
});

const {
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
} = useUsageStats();

const {
  budgetStatus,
  budgetLoading,
  budgetSaving,
  budgetError,
  budgetDollars,
  budgetResetDay,
  budgetEnabled,
  saveBudget,
  budgetConfigured,
  budgetUsed,
  budgetPercent,
  budgetBarColor,
  budgetResetLabel,
} = useBudgetConfig(gatewayBase);
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
