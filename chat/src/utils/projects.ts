import type { ProjectGroup } from "../types";

/**
 * Map raw `/api/sessions` project rows to the frontend `ProjectGroup` shape.
 *
 * The backend returns the project display name directly as `name`; this
 * helper just normalizes the optional/defaulted fields so every place that
 * loads the sessions list (WS `sessions_list`, `useSessions`, sidebar
 * refresh) produces the same shape.
 *
 * Chat 视图不展示 workspace 类型的项目（工作空间属 work 视图范畴），
 * 在此统一过滤——侧栏 REST/WS、准备页 cwd 列表均不出现 ws_* 项目。
 */
export function mapProjectGroups(
  raw: Array<Record<string, unknown>>,
): ProjectGroup[] {
  return raw
    .filter((p) => p.projectType !== "workspace")
    .map((p) => ({
      id: p.id as string | undefined,
      path: (p.path as string) || "",
      name: (p.name as string) || "",
      projectType: p.projectType as string | undefined,
      pinned: (p.pinned as number) || 0,
      archived: (p.archived as boolean) || false,
      sessions: (p.sessions as ProjectGroup["sessions"]) || [],
    }));
}
