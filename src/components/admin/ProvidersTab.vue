<script setup lang="ts">
import { ref, onMounted, nextTick } from "vue";
import { KeyRound, Loader2, RefreshCw, FileJson2, RotateCcw, CheckCircle2 } from "lucide-vue-next";
import {
  useAdmin,
  type PiProviderStatus,
} from "../../composables/useAdmin";

const props = defineProps<{
  brokerHttpUrl: string;
  piRunning: boolean;
}>();

const emit = defineEmits<{
  (e: "restart-pi"): void;
}>();

const { piAuthStatus, piModelsConfig, loading, error,
  fetchPiAuthStatus, setPiApiKey, removePiApiKey, fetchPiModelsConfig, savePiModelsConfig } = useAdmin();

// ─── API keys panel ─────────────────────────────────────────────────────────

const editingProvider = ref<string | null>(null);
const keyInput = ref("");
const keyError = ref("");
const keySaving = ref(false);

// ─── models.json editor ─────────────────────────────────────────────────────

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

const modelsText = ref("");
const modelsSaved = ref(false);
const modelsError = ref("");
const modelsSaving = ref(false);

// ─── model availability check ───────────────────────────────────────────────

const checkingModels = ref(false);
const modelCount = ref<number | null>(null);
const modelCheckError = ref("");

function statusText(p: PiProviderStatus): string {
  if (!p.configured) return "Not configured";
  if (p.source === "stored") {
    return p.custom ? "Configured · OAuth" : "Configured · auth.json";
  }
  if (p.source === "environment") {
    return `From environment · ${p.env_var ?? "env var"}`;
  }
  return "Configured";
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
    keyError.value = "Key cannot be empty.";
    return;
  }
  keySaving.value = true;
  keyError.value = "";
  try {
    const ok = await setPiApiKey(provider, key);
    if (!ok) {
      keyError.value = error.value || "Failed to save key.";
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
  if (!confirm(`Remove stored credentials for ${label}?`)) return;
  const ok = await removePiApiKey(p.provider);
  if (!ok) {
    keyError.value = error.value || "Failed to remove key.";
  }
}

// ─── models.json ────────────────────────────────────────────────────────────

function insertOllamaExample() {
  const current = modelsText.value.trim();
  if (current && current !== "{}" && current !== "{\n  \"providers\": {}\n}") {
    if (!confirm("Replace current content with the Ollama example?")) return;
  }
  modelsText.value = MODELS_JSON_EXAMPLE;
  modelsError.value = "";
}

async function saveModels() {
  modelsError.value = "";
  try {
    JSON.parse(modelsText.value);
  } catch (e) {
    modelsError.value = `Invalid JSON: ${e instanceof Error ? e.message : String(e)}`;
    return;
  }
  modelsSaving.value = true;
  try {
    const ok = await savePiModelsConfig(modelsText.value);
    if (ok) {
      modelsSaved.value = true;
      setTimeout(() => (modelsSaved.value = false), 2000);
    } else {
      modelsError.value = error.value || "Failed to save models.json";
    }
  } finally {
    modelsSaving.value = false;
  }
}

// ─── model availability check ───────────────────────────────────────────────

async function checkModels() {
  if (!props.brokerHttpUrl) return;
  checkingModels.value = true;
  modelCount.value = null;
  modelCheckError.value = "";
  try {
    const res = await fetch(`${props.brokerHttpUrl}/api/rpc`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ type: "get_available_models" }),
    });
    const data = await res.json();
    if (data.success && Array.isArray(data.data?.models)) {
      modelCount.value = data.data.models.length;
    } else {
      modelCheckError.value = data.error || "Failed to fetch available models.";
    }
  } catch (e) {
    modelCheckError.value = `Gateway unreachable: ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    checkingModels.value = false;
  }
}

onMounted(loadAll);
</script>

<template>
  <div class="tab-content">
    <div class="tab-header">
      <div class="tab-header-info">
        <h3 class="tab-title">Providers</h3>
        <p class="tab-desc">
          Configure the model providers Pi can use — API keys, environment
          overrides, and custom providers. Changes apply to new sessions
          immediately; running sessions pick them up after a Pi restart.
        </p>
      </div>
      <button class="btn btn-sm" :disabled="loading.authStatus || loading.modelsConfig" @click="loadAll">
        <RefreshCw :size="12" :class="{ spin: loading.authStatus || loading.modelsConfig }" />
        {{ loading.authStatus ? "Loading..." : "Refresh" }}
      </button>
    </div>

    <!-- API Keys -->
    <div class="section-card">
      <div class="section-header">
        <div class="section-title">
          <KeyRound :size="14" class="section-icon" />
          <span>API Keys</span>
        </div>
        <p class="section-desc">
          Stored in <code>~/.pi/agent/auth.json</code> with 0600 permissions.
          When no key is stored, Pi falls back to the matching environment
          variable. OAuth subscriptions (e.g. ChatGPT/Claude plans) are managed
          with <code>pi /login</code> and appear below as read-only entries.
        </p>
      </div>

      <div v-if="loading.authStatus" class="loading-row">
        <Loader2 :size="12" class="spin" />
        <span>Loading providers…</span>
      </div>

      <template v-else>
        <div v-if="!piAuthStatus || piAuthStatus.length === 0" class="empty-row">
          <KeyRound :size="20" class="empty-icon" />
          <span>No providers known.</span>
        </div>

        <div v-else class="provider-list">
          <div v-for="p in piAuthStatus" :key="p.provider" class="provider-item">
            <template v-if="editingProvider === p.provider">
              <div class="provider-edit">
                <input
                  :id="`api-key-input-${p.provider}`"
                  v-model="keyInput"
                  type="password"
                  class="input key-input"
                  autocomplete="off"
                  spellcheck="false"
                  placeholder="Paste API key…"
                  @keydown.enter.prevent="saveKey"
                  @keydown.esc.prevent="cancelEdit"
                />
                <div v-if="keyError" class="inline-error">{{ keyError }}</div>
                <div class="provider-edit-actions">
                  <button class="btn btn-sm" :disabled="keySaving" @click="cancelEdit">Cancel</button>
                  <button class="btn btn-sm btn-primary" :disabled="keySaving" @click="saveKey">
                    {{ keySaving ? "Saving…" : "Save" }}
                  </button>
                </div>
              </div>
            </template>

            <template v-else>
              <div class="provider-info">
                <div class="provider-name-row">
                  <span class="provider-name">{{ p.display_name }}</span>
                  <span v-if="p.custom" class="provider-id">{{ p.provider }}</span>
                </div>
                <span class="provider-status">
                  <span class="status-dot" :class="dotClass(p)"></span>
                  <span class="status-text">{{ statusText(p) }}</span>
                </span>
              </div>
              <div class="provider-actions">
                <button v-if="!p.custom" class="btn btn-sm" @click="beginEdit(p)">
                  {{ p.configured ? "Update" : "Set key" }}
                </button>
                <button
                  v-if="p.configured && p.source === 'stored'"
                  class="btn btn-sm btn-remove"
                  @click="removeKey(p)"
                >
                  Remove
                </button>
              </div>
            </template>
          </div>
        </div>

        <div v-if="error" class="inline-error">{{ error }}</div>
      </template>

      <!-- Apply / verify -->
      <div v-if="piAuthStatus && !loading.authStatus" class="section-footer apply-row">
        <div class="apply-note">
          <button
            class="btn btn-sm"
            :disabled="checkingModels || !brokerHttpUrl"
            @click="checkModels"
          >
            <Loader2 v-if="checkingModels" :size="12" class="spin" />
            <CheckCircle2 v-else :size="12" />
            {{ checkingModels ? "Checking…" : "Refresh model list" }}
          </button>
          <button
            class="btn btn-sm"
            :disabled="!piRunning"
            title="Restart Pi to apply key changes to running sessions"
            @click="emit('restart-pi')"
          >
            <RotateCcw :size="12" />
            Restart Pi to apply
          </button>
        </div>
        <div v-if="modelCount !== null" class="model-check-ok">
          {{ modelCount }} model{{ modelCount === 1 ? "" : "s" }} available.
        </div>
        <div v-if="modelCheckError" class="inline-error">{{ modelCheckError }}</div>
      </div>
    </div>

    <!-- Custom providers (models.json) -->
    <div class="section-card">
      <div class="section-header">
        <div class="section-title">
          <FileJson2 :size="14" class="section-icon" />
          <span>Custom Providers</span>
        </div>
        <p class="section-desc">
          <code>~/.pi/agent/models.json</code> lets Pi talk to services that
          aren't in the built-in list above — local models, company gateways,
          or proxies.
        </p>
      </div>

      <div class="info-box">
        <div class="info-box-title">When do I need this?</div>
        <ul class="info-list">
          <li>
            <strong>Use local models.</strong> Run models on your own machine
            (Ollama, LM Studio, vLLM) instead of paying for a cloud API.
          </li>
          <li>
            <strong>Use a proxy / gateway.</strong> Point a built-in provider
            (Anthropic, OpenAI, …) at a custom <code>baseUrl</code> — common
            with corporate proxies or API aggregators.
          </li>
          <li>
            <strong>Custom key per endpoint.</strong> Some gateways require a
            key that only works with their <code>baseUrl</code>; store it here
            instead of in the API Keys list above.
          </li>
        </ul>

        <div class="info-box-title">What each field means</div>
        <dl class="field-list">
          <div class="field-row">
            <dt>baseUrl</dt>
            <dd>The API endpoint. For Ollama it's usually <code>http://localhost:11434/v1</code>.</dd>
          </div>
          <div class="field-row">
            <dt>api</dt>
            <dd>API format Pi uses to talk to it. <code>openai-completions</code> works for most tools (including Ollama).</dd>
          </div>
          <div class="field-row">
            <dt>apiKey</dt>
            <dd>Optional. Only needed if the service requires one (Ollama accepts any value).</dd>
          </div>
          <div class="field-row">
            <dt>models</dt>
            <dd>The models to show in chat, each as <code>{ "id": "..." }</code>. Use the exact id from your service.</dd>
          </div>
        </dl>

        <div class="info-box-title">Quick start with Ollama</div>
        <ol class="info-list">
          <li>Install <a href="https://ollama.com" target="_blank" rel="noreferrer">Ollama</a> and pull a model, e.g. <code>ollama pull llama3.1:8b</code>.</li>
          <li>Click <strong>Insert Ollama example</strong> below and adjust the model ids.</li>
          <li>Click <strong>Save models.json</strong>, then <strong>Restart Pi</strong>.</li>
          <li>Open a chat and pick your local model from the model selector.</li>
        </ol>

        <p class="info-note">
          Keys stored here are separate from the API Keys list above. Pi reads
          this file when a session starts.
        </p>
      </div>

      <textarea
        v-model="modelsText"
        class="input models-textarea"
        spellcheck="false"
        :disabled="loading.modelsConfig"
        placeholder="{ &quot;providers&quot;: {} }"
      ></textarea>

      <div class="editor-actions">
        <button class="btn btn-sm" @click="insertOllamaExample">Insert Ollama example</button>
        <button class="btn btn-sm btn-primary" :disabled="modelsSaving" @click="saveModels">
          {{ modelsSaving ? "Saving…" : modelsSaved ? "Saved" : "Save models.json" }}
        </button>
      </div>
      <div v-if="modelsError" class="inline-error">{{ modelsError }}</div>
    </div>
  </div>
</template>

<style scoped>
.tab-content {
  padding: var(--space-xl);
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.tab-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
}

.tab-header-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tab-title {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0;
}

.tab-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
  line-height: var(--line-height-caption);
}

.section-card {
  background: var(--bg-muted);
  border-radius: var(--radius-md);
  padding: var(--space-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.section-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.section-title {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  font-size: var(--font-size-body);
  font-weight: 600;
  color: var(--text);
}

.section-icon {
  color: var(--accent);
}

.section-desc {
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  margin: 0;
  line-height: var(--line-height-caption);
}

code {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 0 4px;
}

.loading-row,
.empty-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
  padding: var(--space-sm) 0;
}

.empty-icon {
  opacity: 0.4;
  flex-shrink: 0;
}

.provider-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: var(--space-sm);
}

.provider-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-sm);
  padding: var(--space-sm) var(--space-md);
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  min-height: 46px;
  transition:
    border-color var(--duration-fast) var(--ease),
    background var(--duration-fast) var(--ease);
}
.provider-item:hover {
  border-color: var(--border-hover);
  background: var(--bg-hover);
}

.provider-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  flex: 1;
}

.provider-name-row {
  display: flex;
  align-items: baseline;
  gap: var(--space-sm);
  min-width: 0;
}

.provider-name {
  font-size: var(--font-size-body);
  font-weight: 500;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.provider-id {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.provider-status {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}
.status-dot.ok {
  background: var(--success);
  box-shadow: 0 0 0 3px var(--success-soft);
}
.status-dot.env {
  background: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.status-dot.none {
  background: var(--border-strong);
}

.status-text {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.provider-actions {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  flex-shrink: 0;
}

.btn-remove {
  background: transparent;
  border-color: transparent;
  color: var(--danger);
}
.btn-remove:hover {
  background: var(--danger-soft);
  border-color: transparent;
}

.provider-edit {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-sm);
  width: 100%;
}

.provider-edit-actions {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  flex-shrink: 0;
}

.key-input {
  flex: 1;
  min-width: 140px;
  font-family: var(--font-mono);
  font-size: 12px;
}

.inline-error {
  font-size: var(--font-size-caption);
  color: var(--danger);
  width: 100%;
}

.section-footer {
  margin-top: var(--space-sm);
  padding-top: var(--space-md);
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.apply-row {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  flex-direction: row;
  flex-wrap: wrap;
}

.apply-note {
  display: flex;
  gap: var(--space-sm);
  align-items: center;
}

.model-check-ok {
  font-size: var(--font-size-caption);
  color: var(--success);
}

.models-textarea {
  height: 220px;
  width: 100%;
  resize: vertical;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
  padding: var(--space-sm);
}

.info-box {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-md);
}

.info-box-title {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  margin-top: var(--space-sm);
}
.info-box-title:first-child {
  margin-top: 0;
}

.info-list {
  margin: 0;
  padding-left: var(--space-lg);
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: var(--font-size-caption);
  color: var(--text-secondary);
  line-height: var(--line-height-caption);
}

.info-list a {
  color: var(--accent);
  text-decoration: none;
}
.info-list a:hover {
  text-decoration: underline;
}

.field-list {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field-row {
  display: grid;
  grid-template-columns: 90px 1fr;
  gap: var(--space-sm);
  align-items: baseline;
  font-size: var(--font-size-caption);
}

.field-row dt {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--accent);
}

.field-row dd {
  margin: 0;
  color: var(--text-secondary);
  line-height: var(--line-height-caption);
}

.info-note {
  margin: var(--space-xs) 0 0;
  font-size: var(--font-size-caption);
  color: var(--text-tertiary);
  line-height: var(--line-height-caption);
  border-top: 1px solid var(--border);
  padding-top: var(--space-sm);
}

.editor-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-sm);
}

.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
