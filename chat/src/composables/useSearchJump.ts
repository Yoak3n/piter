import { ref } from "vue";
import { i18n } from "../i18n";
import type { SearchHit } from "../types";

// ─── 跨会话搜索跳转：切会话 + 滚动定位 ────────────────────────────────
// 面板搜索分区命中后：先切会话（同步完成 activeInstanceId 切换），再设跳转目标，
// 等快照到达后由 MessageTimeline 消费（按 timestamp 精确匹配 + query 前缀兜底）。

export interface SearchJumpDeps {
  /** 切到目标会话（keepScroll=true 表示不清除跳转目标） */
  selectSession: (instanceId: string, keepScroll?: boolean) => void;
}

export function useSearchJump(deps: SearchJumpDeps) {
  /** 跳转目标：切到命中的会话后滚动到对应消息（等快照到达后由 MessageTimeline 消费） */
  const pendingScrollTarget = ref<{ sessionId: string; timestamp?: number; query: string } | null>(null);

  function relativeTime(ts?: number): string {
    if (!ts) return "";
    const diff = Date.now() - ts;
    if (diff < 60_000) return i18n.global.t("common.timeJustNow");
    const mins = Math.floor(diff / 60_000);
    if (mins < 60) return `${mins}m`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h`;
    return `${Math.floor(hrs / 24)}d`;
  }

  function handleSearchJump(hit: SearchHit, query: string) {
    // 先切会话（同步完成 activeInstanceId 切换），再设跳转目标——
    // 避免 watch 在旧会话的消息上误定位并提前清掉目标
    deps.selectSession(hit.sessionId, true);
    pendingScrollTarget.value = { sessionId: hit.sessionId, timestamp: hit.timestamp, query };
  }

  return { pendingScrollTarget, relativeTime, handleSearchJump };
}
