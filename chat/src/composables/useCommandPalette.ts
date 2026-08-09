import { ref, computed } from "vue";
import type { Ref } from "vue";
import { i18n } from "../i18n";
import type {
  PaletteItem,
  SearchHit,
  SlashCommand,
  ProjectGroup,
  ModelRef,
  ImageContent,
} from "../types";

// ─── 命令面板（Ctrl+K）─────────────────────────────────────────────────
// "运行 pi 命令"分区复用 get_commands 数据源（与斜杠补全共享，最近使用置顶）；
// 跨会话搜索分区：面板输入即搜索词（≥3 字符防抖 250ms 调 /api/search），
// 结果转 PaletteItem（kind: "search"）注入面板。
// 依赖注入：会话动作/搜索跳转等由 App.vue 提供，本模块保持无业务耦合。

export interface CommandPaletteDeps {
  isTauri: boolean;
  openAdmin: () => Promise<void>;
  handleNewSession: () => void;
  handleSelectSession: (instanceId: string, keepScroll?: boolean) => void;
  slashCommands: Ref<SlashCommand[] | null>;
  wsSessions: Ref<ProjectGroup[]>;
  recordRecentCommand: (name: string) => void;
  reorderByRecent: <T extends { name: string }>(items: T[]) => T[];
  sendPrompt: (
    text: string,
    desiredModel?: ModelRef | null,
    behavior?: "steer",
    images?: ImageContent[],
    meta?: Record<string, unknown>,
  ) => void;
  fetchSlashCommands: () => void;
  handleSearchJump: (hit: SearchHit, query: string) => void;
  relativeTime: (ts?: number) => string;
}

export function useCommandPalette(deps: CommandPaletteDeps) {
  const paletteOpen = ref(false);
  // ── 跨会话搜索（面板搜索分区）─────────────────────────────────────────
  const paletteSearchResults = ref<PaletteItem[]>([]);
  const paletteSearching = ref(false);
  let paletteSearchTimer: ReturnType<typeof setTimeout> | null = null;
  /** 当前生效的搜索词（丢弃迟到的旧请求结果） */
  let paletteSearchQuery = "";

  function openPalette() {
    paletteOpen.value = true;
    // "运行 pi 命令"分区复用 get_commands 数据源：缓存为空时懒加载
    deps.fetchSlashCommands();
  }
  function closePalette() {
    paletteOpen.value = false;
    paletteSearchResults.value = [];
    paletteSearching.value = false;
  }

  function onPaletteQuery(q: string) {
    if (paletteSearchTimer) clearTimeout(paletteSearchTimer);
    const trimmed = q.trim();
    if (trimmed.length < 3) {
      paletteSearchQuery = "";
      paletteSearchResults.value = [];
      paletteSearching.value = false;
      return;
    }
    paletteSearchQuery = trimmed;
    paletteSearching.value = true;
    paletteSearchTimer = setTimeout(async () => {
      // 面板已关闭（Esc/点击外部）：结果不再注入
      if (!paletteOpen.value) return;
      try {
        const res = await fetch(`/api/search?q=${encodeURIComponent(trimmed)}&limit=50`);
        const data = await res.json();
        // 请求期间用户又改了词：丢弃这份过期结果
        if (paletteSearchQuery !== trimmed) return;
        const hits: SearchHit[] = data.results ?? [];
        paletteSearchResults.value = hits.map((hit) => {
          const hint = [
            hit.projectName,
            hit.label,
            deps.relativeTime(hit.timestamp),
          ].filter(Boolean).join(" · ");
          return {
            id: `search:${hit.sessionId}:${hit.timestamp ?? hit.entryId ?? "?"}`,
            title: hit.snippet || hit.label || hit.sessionId,
            keywords: hit.label ?? "",
            hint,
            kind: "search" as const,
            run: () => deps.handleSearchJump(hit, trimmed),
          };
        });
      } catch {
        if (paletteSearchQuery === trimmed) paletteSearchResults.value = [];
      } finally {
        if (paletteSearchQuery === trimmed) paletteSearching.value = false;
      }
    }, 250);
  }

  function handlePaletteRun(item: PaletteItem) {
    paletteOpen.value = false;
    paletteSearchResults.value = [];
    paletteSearching.value = false;
    item.run();
  }

  /** Ctrl/Cmd+K 全局开关（web 端与浏览器无默认冲突，可安全使用；输入框聚焦时也接管） */
  function onGlobalKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      paletteOpen.value = !paletteOpen.value;
    }
  }

  /**
   * 命令源（可扩展）：动作命令在前、会话切换目标在后。
   * "选择即完成"：选中即执行，无确认步骤；对象操作类（置顶/归档/删除/重命名）留在侧边栏，不进面板。
   * 后续新增命令（如跨会话搜索）只需向此列表 push 一条。
   */
  const paletteItems = computed<PaletteItem[]>(() => {
    const items: PaletteItem[] = [];
    if (deps.isTauri) {
      items.push({
        id: "open-settings",
        title: i18n.global.t("chat.cmdOpenSettings"),
        keywords: "settings admin providers 设置 管理 面板",
        kind: "action",
        run: () => void deps.openAdmin(),
      });
    }
    items.push({
      id: "new-session",
      title: i18n.global.t("chat.cmdNewSession"),
      keywords: "new chat project 新建 会话 项目",
      kind: "action",
      run: () => deps.handleNewSession(),
    });
    // "运行 pi 命令"分区：复用 get_commands 数据源（与斜杠补全共享），最近使用置顶
    for (const c of deps.reorderByRecent(deps.slashCommands.value ?? [])) {
      const srcLabel =
        c.source === "prompt"
          ? i18n.global.t("chat.slashSourcePrompt")
          : c.source === "skill"
            ? i18n.global.t("chat.slashSourceSkill")
            : i18n.global.t("chat.slashSourceExtension");
      items.push({
        id: `slash:${c.name}`,
        title: `/${c.name}`,
        keywords: `pi command ${c.source} ${c.description ?? ""}`,
        hint: c.description || srcLabel,
        kind: "slash",
        run: () => {
          deps.recordRecentCommand(c.name);
          // meta.slashCommand：时间线灰显"已执行命令"（扩展命令不产 agent turn，避免孤立空消息）
          deps.sendPrompt(`/${c.name}`, undefined, undefined, undefined, { slashCommand: true });
        },
      });
    }
    for (const project of deps.wsSessions.value) {
      for (const s of project.sessions) {
        const iid = s.instanceId ?? s.id;
        items.push({
          id: `session:${iid}`,
          title: s.label || s.id,
          keywords: `${s.id} ${s.cwd ?? ""} ${project.name}`,
          hint: project.name,
          kind: "session",
          run: () => deps.handleSelectSession(iid),
        });
      }
    }
    return items;
  });

  return {
    paletteOpen,
    paletteSearchResults,
    paletteSearching,
    paletteItems,
    openPalette,
    closePalette,
    onPaletteQuery,
    handlePaletteRun,
    onGlobalKeydown,
  };
}
