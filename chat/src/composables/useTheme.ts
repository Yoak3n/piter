import { onMounted } from "vue";

// ─── 主题（明暗）───────────────────────────────────────────────────────
// 本应用由 gateway 以普通网页方式提供，不能依赖 Tauri 运行时。桌面端把保存的主题
// 以 `theme` query 参数注入导航；否则跟随系统偏好。
// （跨端共享：admin 侧 src/utils/theme.ts 的 resolveTheme/applyTheme 已同类，待 B4
//   应用收敛时并入 @piter/ui，本 composable 保持 chat 端行为不变。）

export function useTheme() {
  const darkMedia = window.matchMedia("(prefers-color-scheme: dark)");
  let currentTheme = "system";

  function applyTheme() {
    const dark =
      currentTheme === "dark" || (currentTheme === "system" && darkMedia.matches);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
  }

  function applySavedTheme() {
    const urlTheme = new URLSearchParams(window.location.search).get("theme");
    if (urlTheme === "light" || urlTheme === "dark" || urlTheme === "system") {
      currentTheme = urlTheme;
    }
    applyTheme();
  }

  onMounted(() => {
    darkMedia.addEventListener("change", applyTheme);
    applySavedTheme();
  });

  return { applyTheme, applySavedTheme };
}
