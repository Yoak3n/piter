import { ref } from "vue";
import type { ProjectGroup } from "../types";

export function useSessions() {
  const sessions = ref<ProjectGroup[]>([]);
  const loading = ref(false);

  async function fetchSessions() {
    loading.value = true;
    try {
      const res = await fetch("/api/sessions");
      const data = await res.json();
      const raw = data.projects || [];
      sessions.value = raw.map((p: Record<string, unknown>) => ({
        id: p.id as string | undefined,
        path: p.path as string || "",
        name: (p.name ?? p.dirName) as string || "",
        pinned: (p.pinned as number) || 0,
        archived: (p.archived as boolean) || false,
        sessions: p.sessions as any[] || [],
      }));
    } catch (e) {
      console.error("Failed to load sessions:", e);
    } finally {
      loading.value = false;
    }
  }

  async function deleteSession(instanceId: string): Promise<boolean> {
    try {
      const res = await fetch(
        `/api/delete-session?instanceId=${encodeURIComponent(instanceId)}`,
      );
      const data = await res.json();
      return data.success === true;
    } catch (e) {
      console.error("Failed to delete session:", e);
      return false;
    }
  }

  async function createSession(
    cwd?: string,
    name?: string,
  ): Promise<{ id: string; file_path: string } | null> {
    try {
      const res = await fetch("/api/sessions/create", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          cwd: cwd || ".",
          name: name || "New Session",
        }),
      });
      const data = await res.json();
      if (data.success) {
        return { id: data.id, file_path: data.file_path };
      }
    } catch (e) {
      console.error("Failed to create session:", e);
    }
    return null;
  }

  return {
    sessions,
    loading,
    fetchSessions,
    deleteSession,
    createSession,
  };
}
