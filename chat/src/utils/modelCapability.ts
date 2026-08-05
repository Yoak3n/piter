import type { ModelInfo, ModelRef } from "../types";

// Vision-capable model id patterns (方案 A —— 仅作为 pi 模型库缺失时的回退)。
// - OpenAI: gpt-4o / gpt-4.1 / gpt-5.x / o3 / o4 (gpt-3.5 及以下不支持，不匹配)
// - Anthropic: 全部 Claude 模型（3/4/5 系）均支持图片
// - Google: 全部 Gemini 模型（1.5/2/3 系）均支持图片
// - MiniMax mimo family (mimo / mimo-v2.5 支持图片；mimo-v2.5-pro 为纯文本，排除)
// - Generic suffix markers: `vl` / `vision` (e.g. qwen2.5-vl, mini-vision)
const VISION_PATTERNS: RegExp[] = [
  /^(gpt-4o|gpt-4\.1|gpt-5|o3|o4)/,
  /^claude-/,
  /^gemini-/i,
  /^mimo(?!.*-pro)/,
  /(^|[_-])(vl|vision)([_-]|$)/i,
];

/**
 * pi 模型库声明的视觉能力缓存（方案 B，权威来源）。
 * key = `provider/id` 或 `id`（currentModel 的 provider 可能缺失，两者都登记）。
 * 由 App.vue 启动时从 /api/pi/model-catalog 预热，ModelSelector 从
 * get_available_models（含自定义 provider）刷新。
 */
const visionRegistry = new Map<string, boolean>();

function capabilityKey(id: string, provider?: string): string {
  return provider ? `${provider}/${id}` : id;
}

/**
 * 登记模型模态声明。`input` 含 "image" 视为支持图片输入。
 * 只登记有明确声明的模型；未声明的模型保持"未知"（走正则回退）。
 */
export function registerModelCapabilities(
  models: Array<Pick<ModelInfo, "id" | "provider" | "input">> | undefined | null,
): void {
  if (!Array.isArray(models)) return;
  for (const m of models) {
    if (!m?.id || !Array.isArray(m.input)) continue;
    const vision = m.input.includes("image");
    visionRegistry.set(capabilityKey(m.id, m.provider), vision);
    visionRegistry.set(capabilityKey(m.id, undefined), vision);
  }
}

/**
 * Whether the current model is (definitely) capable of image input.
 *
 * 优先级：pi 模型库的模态声明（权威）→ 正则表回退（未收录/未知模型）。
 * 未命中任何规则视为"未知"→ 返回 false，UI 显示弱提示
 * （"当前模型可能不支持图片"），但不硬拦截发送——避免误伤新模型。
 * 无模型信息（尚未加载）时放行。
 */
export function supportsVision(model?: ModelRef | null): boolean {
  if (!model?.id) return true;
  const known = visionRegistry.get(capabilityKey(model.id, model.provider));
  if (known !== undefined) return known;
  return VISION_PATTERNS.some((re) => re.test(model.id));
}
