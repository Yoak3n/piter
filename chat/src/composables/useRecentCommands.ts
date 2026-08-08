import { ref } from "vue";

/**
 * 最近使用的 pi 斜杠命令（本地记录，localStorage 持久化）。
 * 模块级单例：斜杠补全（Composer）与命令面板（App.vue）共享同一份记录。
 * 仅记录命令 name；渲染时按"当前会话命令列表中是否存在"过滤，避免置顶不存在的命令。
 */
const STORAGE_KEY = "piter:recentSlash";
const MAX_ITEMS = 8;

function loadRecent(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((n): n is string => typeof n === "string") : [];
  } catch {
    return [];
  }
}

const recent = ref<string[]>(loadRecent());

export function useRecentCommands() {
  /** 记录一次使用（去重置顶，超出上限裁剪） */
  function record(name: string) {
    if (!name) return;
    const next = [name, ...recent.value.filter((n) => n !== name)].slice(0, MAX_ITEMS);
    recent.value = next;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch {
      // localStorage 不可用（隐私模式等）时静默降级为内存态
    }
  }

  /** 把最近使用中、且存在于 base 的命令提到最前（保持最近使用顺序），其余保持原顺序 */
  function reorderByRecent<T extends { name: string }>(base: T[]): T[] {
    if (recent.value.length === 0 || base.length <= 1) return base;
    const hit = recent.value
      .map((n) => base.find((c) => c.name === n))
      .filter((c): c is T => Boolean(c));
    if (hit.length === 0) return base;
    const hitNames = new Set(hit.map((c) => c.name));
    return [...hit, ...base.filter((c) => !hitNames.has(c.name))];
  }

  return { recent, record, reorderByRecent };
}
