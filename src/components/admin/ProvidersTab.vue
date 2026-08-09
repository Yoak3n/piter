<script setup lang="ts">
import { toRefs } from "vue";
import { KeyRound, Loader2, RefreshCw, FileJson2, RotateCcw, CheckCircle2 } from "lucide-vue-next";
import { useProviders } from "../../composables/useProviders";

// ─── Providers 配置 Tab ─────────────────────────────────────────────────
// 全部逻辑（API keys 编辑 / models.json / 模型检查）在 useProviders；
// 本组件只做模板组装，样式原样保留。

const props = defineProps<{
  brokerHttpUrl: string;
  piRunning: boolean;
}>();

const emit = defineEmits<{
  (e: "restart-pi"): void;
}>();

const { brokerHttpUrl, piRunning } = toRefs(props);

const {
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
} = useProviders({
  brokerHttpUrl: props.brokerHttpUrl,
  piRunning: props.piRunning,
  onRestartPi: () => emit("restart-pi"),
});
</script>

<template>
  <div class="tab-content">
    <div class="tab-header">
      <div class="tab-header-info">
        <h3 class="tab-title">{{ $t("admin.providersTitle") }}</h3>
        <p class="tab-desc">{{ $t("admin.providersDesc") }}</p>
      </div>
      <button class="btn btn-sm" :disabled="loading.authStatus || loading.modelsConfig" @click="loadAll">
        <RefreshCw :size="12" :class="{ spin: loading.authStatus || loading.modelsConfig }" />
        {{ loading.authStatus ? $t("common.loading") : $t("admin.refresh") }}
      </button>
    </div>

    <!-- API Keys -->
    <div class="section-card">
      <div class="section-header">
        <div class="section-title">
          <KeyRound :size="14" class="section-icon" />
          <span>{{ $t("admin.apiKeys") }}</span>
        </div>
        <p class="section-desc">
          <i18n-t keypath="admin.apiKeysDesc" tag="span">
            <template #path><code>~/.pi/agent/auth.json</code></template>
            <template #login><code>pi /login</code></template>
          </i18n-t>
        </p>
      </div>

      <div v-if="loading.authStatus" class="loading-row">
        <Loader2 :size="12" class="spin" />
        <span>{{ $t("admin.loadingProviders") }}</span>
      </div>

      <template v-else>
        <EmptyState v-if="!piAuthStatus || piAuthStatus.length === 0" compact :title="$t('admin.noProviders')">
          <template #icon><KeyRound :size="20" /></template>
        </EmptyState>

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
                  :placeholder="$t('admin.keyPlaceholder')"
                  @keydown.enter.prevent="saveKey"
                  @keydown.esc.prevent="cancelEdit"
                />
                <div v-if="keyError" class="inline-error">{{ keyError }}</div>
                <div class="provider-edit-actions">
                  <button class="btn btn-sm" :disabled="keySaving" @click="cancelEdit">{{ $t("common.cancel") }}</button>
                  <button class="btn btn-sm btn-primary" :disabled="keySaving" @click="saveKey">
                    {{ keySaving ? $t("admin.saving") : $t("common.save") }}
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
                  {{ p.configured ? $t("admin.updateKey") : $t("admin.setKey") }}
                </button>
                <button
                  v-if="p.configured && p.source === 'stored'"
                  class="btn btn-sm btn-remove"
                  @click="removeKey(p)"
                >
                  {{ $t("admin.removeKey") }}
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
            {{ checkingModels ? $t("admin.checkingModels") : $t("admin.refreshModelList") }}
          </button>
          <button
            class="btn btn-sm"
            :disabled="!piRunning"
            :title="$t('admin.restartToApplyTitle')"
            @click="emit('restart-pi')"
          >
            <RotateCcw :size="12" />
            {{ $t("admin.restartToApply") }}
          </button>
        </div>
        <div v-if="modelCount !== null" class="model-check-ok">
          {{ $t("admin.modelsAvailable", modelCount) }}
        </div>
        <div v-if="modelCheckError" class="inline-error">{{ modelCheckError }}</div>
      </div>
    </div>

    <!-- Custom providers (models.json) -->
    <div class="section-card">
      <div class="section-header">
        <div class="section-title">
          <FileJson2 :size="14" class="section-icon" />
          <span>{{ $t("admin.customProviders") }}</span>
        </div>
        <p class="section-desc">
          <i18n-t keypath="admin.customProvidersDesc" tag="span">
            <template #path><code>~/.pi/agent/models.json</code></template>
          </i18n-t>
        </p>
      </div>

      <div class="info-box">
        <div class="info-box-title">{{ $t("admin.whenNeedTitle") }}</div>
        <ul class="info-list">
          <li>
            <strong>{{ $t("admin.needLocal") }}</strong> {{ $t("admin.needLocalDesc") }}
          </li>
          <li>
            <strong>{{ $t("admin.needProxy") }}</strong> {{ $t("admin.needProxyDesc") }}
          </li>
          <li>
            <strong>{{ $t("admin.needCustomKey") }}</strong> {{ $t("admin.needCustomKeyDesc") }}
          </li>
        </ul>

        <div class="info-box-title">{{ $t("admin.whatFieldsTitle") }}</div>
        <dl class="field-list">
          <div class="field-row">
            <dt>{{ $t("admin.fieldBaseUrl") }}</dt>
            <dd>{{ $t("admin.fieldBaseUrlDesc") }}</dd>
          </div>
          <div class="field-row">
            <dt>{{ $t("admin.fieldApi") }}</dt>
            <dd>{{ $t("admin.fieldApiDesc") }}</dd>
          </div>
          <div class="field-row">
            <dt>{{ $t("admin.fieldApiKey") }}</dt>
            <dd>{{ $t("admin.fieldApiKeyDesc") }}</dd>
          </div>
          <div class="field-row">
            <dt>{{ $t("admin.fieldModels") }}</dt>
            <dd>
              <i18n-t keypath="admin.fieldModelsDesc" tag="span">
                <template #code><code>{ "id": "..." }</code></template>
              </i18n-t>
            </dd>
          </div>
        </dl>

        <div class="info-box-title">{{ $t("admin.ollamaTitle") }}</div>
        <ol class="info-list">
          <li>
            <i18n-t keypath="admin.ollamaStep1" tag="span">
              <template #link>
                <a href="https://ollama.com" target="_blank" rel="noreferrer">Ollama</a>
              </template>
              <template #code><code>ollama pull llama3.1:8b</code></template>
            </i18n-t>
          </li>
          <li>
            <i18n-t keypath="admin.ollamaStep2" tag="span">
              <template #strong><strong>{{ $t("admin.insertOllamaExample") }}</strong></template>
            </i18n-t>
          </li>
          <li>
            <i18n-t keypath="admin.ollamaStep3" tag="span">
              <template #strong1><strong>{{ $t("admin.saveModelsJson") }}</strong></template>
              <template #strong2><strong>{{ $t("admin.restartToApply") }}</strong></template>
            </i18n-t>
          </li>
          <li>{{ $t("admin.ollamaStep4") }}</li>
        </ol>

        <p class="info-note">{{ $t("admin.providerInfoNote") }}</p>
      </div>

      <textarea
        v-model="modelsText"
        class="input models-textarea"
        spellcheck="false"
        :disabled="loading.modelsConfig"
        placeholder="{ &quot;providers&quot;: {} }"
      ></textarea>

      <div class="editor-actions">
        <button class="btn btn-sm" @click="insertOllamaExample">{{ $t("admin.insertOllamaExample") }}</button>
        <button class="btn btn-sm btn-primary" :disabled="modelsSaving" @click="saveModels">
          {{ modelsSaving ? $t("admin.saving") : modelsSaved ? $t("common.saved") : $t("admin.saveModelsJson") }}
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

.loading-row {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  color: var(--text-tertiary);
  font-size: var(--font-size-caption);
  padding: var(--space-sm) 0;
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
