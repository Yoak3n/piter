<script setup lang="ts">
import { ref, onMounted } from "vue";

// ─── 首启引导卡（三步上手）──────────────────────────────────────
// 一次性展示（localStorage 标记），直到关闭或创建首个会话；"配置 Provider"
// 步骤在桌面端可跳转设置面板。

defineProps<{ isTauri: boolean }>();

const ONBOARDING_KEY = "piter-onboarded";
const showGuide = ref(false);

function dismiss() {
  showGuide.value = false;
  try { localStorage.setItem(ONBOARDING_KEY, "1"); } catch { /* ignore */ }
}

// Onboarding step 1 → jump to the desktop settings (Providers tab area).
async function openSettings() {
  try {
    const { emit } = await import("@tauri-apps/api/event");
    await emit("navigate-to-admin");
  } catch { /* non-critical */ }
}

onMounted(() => {
  try {
    showGuide.value = !localStorage.getItem(ONBOARDING_KEY);
  } catch {
    showGuide.value = true;
  }
});

defineExpose({ dismiss });
</script>

<template>
  <div v-if="showGuide" class="onboarding">
    <button
      class="onboarding-close"
      :aria-label="$t('chat.dismiss')"
      :title="$t('chat.dismiss')"
      @click="dismiss"
    >
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
    </button>
    <div class="onboarding-emoji">👋</div>
    <h2 class="onboarding-title">{{ $t("chat.welcomeTitle") }}</h2>
    <ol class="onboarding-steps">
      <li class="onboarding-step">
        <span class="onboarding-num">1</span>
        <div class="onboarding-body">
          <strong>{{ $t("chat.stepAddProvider") }}</strong>
          <i18n-t keypath="chat.stepAddProviderDesc" tag="span">
            <template #settings>
              <button v-if="isTauri" class="step-link" @click="openSettings">{{ $t("chat.settingsLink") }}</button>
              <template v-else>{{ $t("chat.settingsLink") }}</template>
            </template>
          </i18n-t>
        </div>
      </li>
      <li class="onboarding-step">
        <span class="onboarding-num">2</span>
        <div class="onboarding-body">
          <strong>{{ $t("chat.stepCreateSession") }}</strong>
          <span>{{ $t("chat.stepCreateSessionDesc") }}</span>
        </div>
      </li>
      <li class="onboarding-step">
        <span class="onboarding-num">3</span>
        <div class="onboarding-body">
          <strong>{{ $t("chat.stepAsk") }}</strong>
          <span>{{ $t("chat.stepAskDesc") }}</span>
        </div>
      </li>
    </ol>
    <button class="btn btn-primary onboarding-done" @click="dismiss">{{ $t("common.gotIt") }}</button>
  </div>
</template>

<style scoped>
/* ── First-launch onboarding ── */
.onboarding {
  position: relative;
  padding: 1.75rem 2rem;
  border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
  border-radius: var(--radius-lg);
  background: var(--accent-soft);
  text-align: center;
  animation: onboard-pop var(--duration) var(--spring);
}

@keyframes onboard-pop {
  from { opacity: 0; transform: translateY(6px); }
  to { opacity: 1; transform: translateY(0); }
}

.onboarding-close {
  position: absolute;
  top: 10px;
  right: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease), color var(--duration-fast) var(--ease);
}

.onboarding-close:hover {
  background: var(--bg-panel);
  color: var(--text);
}

.onboarding-emoji {
  font-size: 2.5rem;
  line-height: 1;
}

.onboarding-title {
  margin: 0.75rem 0 0.5rem;
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text);
}

.onboarding-steps {
  list-style: none;
  margin: 1rem 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  text-align: left;
}

.onboarding-step {
  display: flex;
  align-items: flex-start;
  gap: 0.8rem;
}

.onboarding-num {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  flex-shrink: 0;
  margin-top: 1px;
  border-radius: 50%;
  background: var(--accent-soft);
  border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  color: var(--accent-strong);
  font-size: 0.8rem;
  font-weight: 600;
  line-height: 1;
}

.onboarding-body {
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: 0.95rem;
  color: var(--text-secondary);
}

.onboarding-body strong {
  color: var(--text);
  font-weight: 600;
  font-size: 1.05rem;
}

.step-link {
  border: none;
  background: none;
  padding: 0;
  color: var(--accent);
  font-weight: 600;
  font-size: inherit;
  cursor: pointer;
  text-decoration: underline;
}

.onboarding-done {
  margin-top: 1.25rem;
  height: 38px;
  padding: 0 26px;
  font-size: 0.9rem;
}
</style>
