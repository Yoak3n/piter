<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { QrCode, Copy, Check, X } from "lucide-vue-next";

defineProps<{
  mobileMode: boolean;
}>();

const { t } = useI18n();

const showPopover = ref(false);
const qrSvg = ref("");
const qrData = ref("");
const loading = ref(false);
const error = ref("");
const copied = ref(false);

async function fetchQr() {
  loading.value = true;
  error.value = "";
  try {
    const resp = await fetch("/api/lan-qr");
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const svg = await resp.text();
    if (!svg.trim()) throw new Error("Empty QR response");
    qrSvg.value = svg;
    // Extract the URL from the SVG for display (it's embedded in the QR)
    // We also fetch /api/lan-info for the plain URL
  } catch (e) {
    error.value = t("chat.lanLoadError", { msg: e });
    qrSvg.value = "";
  } finally {
    loading.value = false;
  }
}

async function fetchLanInfo() {
  try {
    const resp = await fetch("/api/lan-info");
    if (!resp.ok) return;
    const info = await resp.json();
    qrData.value = (info.lan_urls?.[0]) || info.http_url || "";
  } catch {
    // non-critical
  }
}

function togglePopover() {
  showPopover.value = !showPopover.value;
  if (showPopover.value && !qrSvg.value && !loading.value) {
    fetchQr();
    fetchLanInfo();
  }
}

function closePopover() {
  showPopover.value = false;
}

async function copyUrl() {
  if (!qrData.value) return;
  try {
    await navigator.clipboard.writeText(qrData.value);
    copied.value = true;
    setTimeout(() => (copied.value = false), 2000);
  } catch {
    // fallback
    const ta = document.createElement("textarea");
    ta.value = qrData.value;
    ta.style.cssText = "position:fixed;left:-9999px";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    document.body.removeChild(ta);
    copied.value = true;
    setTimeout(() => (copied.value = false), 2000);
  }
}
</script>

<template>
  <div v-if="!mobileMode" class="lan-share">
    <button
      class="btn btn-ghost btn-icon btn-sm"
      :title="$t('chat.shareViaLan')"
      @click="togglePopover"
    >
      <QrCode :size="14" />
    </button>

    <!-- Popover -->
    <div v-if="showPopover" class="lan-popover-overlay" @click.self="closePopover">
      <div class="lan-popover">
        <div class="lan-popover-header">
          <span class="lan-popover-title">{{ $t("chat.scanToConnect") }}</span>
          <button class="btn-close" @click="closePopover">
            <X :size="14" />
          </button>
        </div>

        <div v-if="loading" class="lan-popover-loading">{{ $t("common.loading") }}</div>
        <div v-else-if="error" class="lan-popover-error">{{ error }}</div>
        <template v-else>
          <div v-if="qrSvg" class="lan-qr-wrapper" v-html="qrSvg" />
          <div v-if="qrData" class="lan-url-row">
            <code class="lan-url">{{ qrData }}</code>
            <button
              class="btn btn-ghost btn-icon btn-sm"
              :title="copied ? $t('common.copied') : $t('chat.copyUrl')"
              @click="copyUrl"
            >
              <Check v-if="copied" :size="14" />
              <Copy v-else :size="14" />
            </button>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.lan-share {
  position: relative;
  display: inline-flex;
  align-items: center;
}

.lan-popover-overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay-backdrop);
}

.lan-popover {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  min-width: 280px;
  max-width: 320px;
  box-shadow: var(--shadow-modal);
}

.lan-popover-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.lan-popover-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
}

.btn-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: none;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: var(--radius-sm);
}

.btn-close:hover {
  background: var(--bg-hover);
}

.lan-popover-loading,
.lan-popover-error {
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 20px 0;
}

.lan-popover-error {
  color: var(-danger);
}

.lan-qr-wrapper {
  display: flex;
  justify-content: center;
  margin-bottom: 12px;
}

.lan-qr-wrapper :deep(svg) {
  width: 200px;
  height: 200px;
  border-radius: var(--radius-sm);
}

.lan-url-row {
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--bg-muted);
  border-radius: var(--radius-sm);
  padding: 6px 8px;
}

.lan-url {
  flex: 1;
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  word-break: break-all;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
</style>
