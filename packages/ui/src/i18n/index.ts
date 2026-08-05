import { createI18n } from "vue-i18n";
import { messages } from "./messages";

/**
 * Resolve "system" / "zh" / "en" to a concrete locale. Follows
 * `navigator.language` when the preference is unset or "system".
 */
export function resolveLocale(pref?: string | null): "zh" | "en" {
  if (pref === "zh" || pref === "en") return pref;
  const nav =
    typeof navigator !== "undefined" ? (navigator.language || "") : "";
  return nav.toLowerCase().startsWith("zh") ? "zh" : "en";
}

/**
 * Build a vue-i18n instance for the shared messages. Both frontends call
 * this with their language preference ("system" | "zh" | "en" | null).
 */
export function createAppI18n(localePref?: string | null) {
  return createI18n({
    legacy: false,
    locale: resolveLocale(localePref),
    fallbackLocale: "en",
    messages,
  });
}

/**
 * Apply a new language preference to an already-installed i18n instance.
 * Uses a structural type to avoid vue-i18n's invariant generic on messages.
 */
export function setLocale(
  i18n: { global: { locale: { value: string } } },
  pref?: string | null,
) {
  i18n.global.locale.value = resolveLocale(pref);
}

export { messages };
