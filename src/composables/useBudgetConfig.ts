import { ref, computed, onMounted } from "vue";
import type { Ref } from "vue";
import { useI18n } from "vue-i18n";

// ─── 月度预算卡（UsageTab，0.2.0 P3）：配置 + 状态 ─────────────────────
// 配置 + 状态走网关 REST（跨端可用）；金额单位为分（cents），输入框按美元显示。
// 进度条档位变色：<50 正常 / 50-80 黄 / 80-100 橙 / 100 红；未设置/未启用显示"未设置"。
// 纯逻辑 composable：`gatewayBase` 由父级传入（Tauri 状态携带的网关基址）。

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

export function useBudgetConfig(gatewayBase: Ref<string>) {
  const { t } = useI18n();

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

  // 挂载即拉取配置与状态（与 useUsageStats 的 fetchData 同挂载周期）
  onMounted(fetchBudget);

  return {
    budgetConfig,
    budgetStatus,
    budgetLoading,
    budgetSaving,
    budgetError,
    budgetDollars,
    budgetResetDay,
    budgetEnabled,
    fetchBudget,
    saveBudget,
    budgetConfigured,
    budgetUsed,
    budgetPercent,
    budgetBarColor,
    budgetResetLabel,
  };
}
