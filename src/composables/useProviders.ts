import { ref, onMounted, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { useAdmin, type PiProviderStatus } from "./useAdmin";

// ─── Providers 配置（ProvidersTab）：API keys + models.json + 模型检查 ──
// 逻辑集中在 composable（依赖 useAdmin + Tauri 状态注入），页面组件只做模板组装。
// deps.onRestartPi 由父级接到 AdminView 的 restart-pi 事件。

const MODELS_JSON_EXAMPLE = `{
  "providers": {
    "ollama": {
      "baseUrl": "http://localhost:11434/v1",
      "api": "openai-completions",
      "apiKey": "ollama",
      "compat": {
        "supportsDeveloperRole": false,
        "supportsReasoningEffort": false
      },
      "models": [
        { "id": "llama3.1:8b" },
        { "id": "qwen2.5-coder:7b" }
      ]
    }
  }
}`;

export function useProviders(deps: {
  brokerHttpUrl: string;
  piRunning: boolean;
  onRestartPi: () => void;
}) {
  const { t } = useI18n();
  const {
    piAuthStatus,
    piModelsConfig,
    loading,
    error,
    fetchPiAuthStatus,
    setPiApiKey,
    removePiApiKey,
    fetchPiModelsConfig,
    savePiModelsConfig,
  } = useAdmin();

  // ─── API keys panel ──────────────────────────────────────────────────
  const editingProvider = ref<string | null>(null);
  const keyInput = ref("");
  const keyError = ref("");
  const keySaving = ref(false);

  // ─── models.json editor ──────────────────────────────────────────────
  const modelsText = ref("");
  const modelsSaved = ref(false);
  const modelsError = ref("");
  const modelsSaving = ref(false);

  // ─── model availability check ────────────────────────────────────────
  const checkingModels = ref(false);
  const modelCount = ref<number | null>(null);
  const modelCheckError = ref("");

  function statusText(p: PiProviderStatus): string {
    if (!p.configured) return t("admin.providerNotConfigured");
    if (p.source === "stored") {
      return p.custom ? t("admin.providerConfiguredOAuth") : t("admin.providerConfiguredAuth");
    }
    if (p.source === "environment") {
      return t("admin.providerConfiguredEnv", { var: p.env_var ?? "env" });
    }
    return t("admin.providerConfigured");
  }

  function dotClass(p: PiProviderStatus): string {
    if (!p.configured) return "none";
    return p.source === "stored" ? "ok" : "env";
  }

  async function loadAll() {
    await fetchPiAuthStatus();
    await fetchPiModelsConfig();
    modelsText.value = piModelsConfig.value;
    modelsError.value = "";
  }

  function beginEdit(p: PiProviderStatus) {
    keyError.value = "";
    keyInput.value = "";
    editingProvider.value = p.provider;
    nextTick(() => {
      (document.getElementById(`api-key-input-${p.provider}`) as HTMLInputElement | null)?.focus();
    });
  }

  function cancelEdit() {
    editingProvider.value = null;
    keyInput.value = "";
    keyError.value = "";
  }

  async function saveKey() {
    const provider = editingProvider.value;
    if (!provider) return;
    const key = keyInput.value.trim();
    if (!key) {
      keyError.value = t("admin.keyErrorEmpty");
      return;
    }
    keySaving.value = true;
    keyError.value = "";
    try {
      const ok = await setPiApiKey(provider, key);
      if (!ok) {
        keyError.value = error.value || t("admin.keySaveFailed");
        return;
      }
      editingProvider.value = null;
      keyInput.value = "";
    } finally {
      keySaving.value = false;
    }
  }

  async function removeKey(p: PiProviderStatus) {
    const label = p.custom ? p.display_name : `${p.display_name} (${p.provider})`;
    if (!confirm(t("admin.removeKeyConfirm", { label }))) return;
    const ok = await removePiApiKey(p.provider);
    if (!ok) {
      keyError.value = error.value || t("admin.keyRemoveFailed");
    }
  }

  // ─── models.json ─────────────────────────────────────────────────────
  function insertOllamaExample() {
    const current = modelsText.value.trim();
    if (current && current !== "{}" && current !== "{\n  \"providers\": {}\n}") {
      if (!confirm(t("admin.replaceModelsConfirm"))) return;
    }
    modelsText.value = MODELS_JSON_EXAMPLE;
    modelsError.value = "";
  }

  async function saveModels() {
    modelsError.value = "";
    try {
      JSON.parse(modelsText.value);
    } catch (e) {
      modelsError.value = t("admin.modelsInvalidJson", { msg: e instanceof Error ? e.message : String(e) });
      return;
    }
    modelsSaving.value = true;
    try {
      const ok = await savePiModelsConfig(modelsText.value);
      if (ok) {
        modelsSaved.value = true;
        setTimeout(() => (modelsSaved.value = false), 2000);
      } else {
        modelsError.value = error.value || t("admin.modelsSaveFailed");
      }
    } finally {
      modelsSaving.value = false;
    }
  }

  // ─── model availability check ────────────────────────────────────────
  async function checkModels() {
    if (!deps.brokerHttpUrl) return;
    checkingModels.value = true;
    modelCount.value = null;
    modelCheckError.value = "";
    try {
      const res = await fetch(`${deps.brokerHttpUrl}/api/rpc`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ type: "get_available_models" }),
      });
      const data = await res.json();
      if (data.success && Array.isArray(data.data?.models)) {
        modelCount.value = data.data.models.length;
      } else {
        modelCheckError.value = data.error || t("admin.modelsFetchFailed");
      }
    } catch (e) {
      modelCheckError.value = t("admin.gatewayUnreachable", { msg: e instanceof Error ? e.message : String(e) });
    } finally {
      checkingModels.value = false;
    }
  }

  onMounted(loadAll);

  return {
    piAuthStatus,
    loading,
    error,
    editingProvider,
    keyInput,
    keyError,
    keySaving,
    modelsText,
    modelsSaved,
    modelsError,
    modelsSaving,
    checkingModels,
    modelCount,
    modelCheckError,
    loadAll,
    statusText,
    dotClass,
    beginEdit,
    cancelEdit,
    saveKey,
    removeKey,
    insertOllamaExample,
    saveModels,
    checkModels,
  };
}
