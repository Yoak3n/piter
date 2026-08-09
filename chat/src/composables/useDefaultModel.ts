import { ref } from "vue";
import type { ModelRef } from "../types";
import { registerModelCapabilities } from "../utils/modelCapability";

// ─── 默认模型 / 视觉能力注册表 / 多模态弱提示 ──────────────────────────
// 数据来源全部"不额外启动 pi 进程"：
//   1. 启动时：只读本地动态目录缓存 /api/pi/model-catalog（零成本，覆盖 opencode-go 等）；
//   2. 会话激活后：pi 已在运行，get_available_models 带上该会话 instanceId 复用实例
//      （补齐内置目录 DeepSeek/OpenAI… 与自定义 provider），且只取一次；
//   3. 打开模型下拉时 ModelSelector 也会登记（最新/自定义 provider）。
// 判定入口 supportsVision(注册表 → 正则回退)，用于附加图片/切换模型时的弱提示。

export function useDefaultModel() {
  // 全局默认模型缓存（/api/pi/settings）：该 instance 未指定 model 时回退用它
  const defaultModel = ref<ModelRef | null>(null);
  // 每个实例只拉取一次完整模型目录（会话激活时）
  const capabilitiesWarmed = ref(false);

  async function ensureDefaultModel(): Promise<ModelRef | null> {
    if (defaultModel.value) return defaultModel.value;
    try {
      const res = await fetch("/api/pi/settings");
      const data = await res.json();
      if (data.success && data.default_model) {
        defaultModel.value = {
          id: data.default_model,
          provider: data.default_provider,
        };
      }
    } catch {
      // non-critical
    }
    return defaultModel.value;
  }

  async function warmModelCapabilities() {
    try {
      const res = await fetch("/api/pi/model-catalog");
      const data = await res.json();
      if (data.success && Array.isArray(data.models)) {
        registerModelCapabilities(data.models);
      }
    } catch {
      // non-critical
    }
  }

  async function refreshModelCapabilities(instanceId: string) {
    try {
      const res = await fetch("/api/rpc", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ type: "get_available_models", instanceId }),
      });
      const data = await res.json();
      if (data.success && Array.isArray(data.data?.models)) {
        registerModelCapabilities(data.data.models);
      }
    } catch {
      // non-critical
    }
  }

  // 多模态弱提示（选图时模型不支持 / 切换模型后仍带附图），传给 Composer 提示条
  const visionHint = ref<{ text: string; key: number } | null>(null);
  let visionHintTimer: ReturnType<typeof setTimeout> | null = null;
  function showVisionHint(text: string) {
    visionHint.value = { text, key: (visionHint.value?.key ?? 0) + 1 };
    if (visionHintTimer) clearTimeout(visionHintTimer);
    visionHintTimer = setTimeout(() => {
      visionHint.value = null;
    }, 4000);
  }

  return {
    defaultModel,
    capabilitiesWarmed,
    ensureDefaultModel,
    warmModelCapabilities,
    refreshModelCapabilities,
    visionHint,
    showVisionHint,
  };
}
