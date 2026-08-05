import type { ProjectGroup } from "../types";

/**
 * Map raw `/api/sessions` project rows to the frontend `ProjectGroup` shape.
 *
 * The backend returns the project display name directly as `name`; this
 * helper just normalizes the optional/defaulted fields so every place that
 * loads the sessions list (WS `sessions_list`, `useSessions`, sidebar
 * refresh) produces the same shape.
 */
export function mapProjectGroups(
  raw: Array<Record<string, unknown>>,
): ProjectGroup[] {
  return raw.map((p) => ({
    id: p.id as string | undefined,
    path: (p.path as string) || "",
    name: (p.name as string) || "",
    pinned: (p.pinned as number) || 0,
    archived: (p.archived as boolean) || false,
    sessions: (p.sessions as ProjectGroup["sessions"]) || [],
  }));
}
