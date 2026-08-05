import { createAppI18n } from "@piter/ui";

// The desktop window injects the saved language as ?lang= when navigating to
// the chat page (see nav.rs web_url_with_prefs); falls back to the system
// locale in a plain browser.
const urlLang = new URLSearchParams(window.location.search).get("lang");
export const i18n = createAppI18n(urlLang);
