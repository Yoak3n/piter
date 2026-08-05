import { createAppI18n } from "@piter/ui";

// Shared i18n instance for the admin panel. The locale follows the saved
// config (app.language) once it loads — AdminView syncs it via setLocale().
export const i18n = createAppI18n("system");
