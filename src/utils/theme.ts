const darkMedia = window.matchMedia("(prefers-color-scheme: dark)");

/** Resolve the effective scheme for a saved theme preference. */
export function resolveTheme(theme: string): "light" | "dark" {
  return theme === "dark" || (theme === "system" && darkMedia.matches)
    ? "dark"
    : "light";
}

/** Apply the saved theme preference to the document root. */
export function applyTheme(theme: string): void {
  document.documentElement.dataset.theme = resolveTheme(theme);
}

export { darkMedia };
