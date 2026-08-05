import type { Attachment, ImageContent, ModelRef } from "../types";
import {
  AttachmentError,
  processImageFile,
  readTextFile,
  stripDataUriPrefix,
} from "./image";
import { supportsVision } from "./modelCapability";

/** 文本类文件：MIME 以 text/ 开头，或常见文本扩展名。 */
export function isTextFile(file: File): boolean {
  const name = file.name.toLowerCase();
  return file.type.startsWith("text/") || /\.(txt|md|json|csv|log)$/.test(name);
}

/**
 * 从粘贴事件中提取图片文件（QQ 截图 / 系统截图后直接 Ctrl+V 到输入框）。
 * 剪贴板里没有图片时返回空数组，调用方应让默认文本粘贴继续。
 */
export function clipboardImageFiles(e: ClipboardEvent): File[] {
  const items = e.clipboardData?.items;
  if (!items) return [];
  const files: File[] = [];
  for (const item of items) {
    if (item.kind === "file" && item.type.startsWith("image/")) {
      const f = item.getAsFile();
      if (f) files.push(f);
    }
  }
  return files;
}

export interface FilesToAttachmentsOptions {
  /** i18n 翻译函数 */
  t: (key: string) => string;
  /** 当前会话模型（多模态预检：模型不支持图片时弱提示） */
  currentModel?: ModelRef | null;
  /** 弱提示回调（图片过大 / 读取失败 / 文本截断等） */
  onHint: (msg: string) => void;
}

/** 把一组文件处理为附件（图片压缩 / 文本读取），按钮选择与拖拽共用。 */
export async function filesToAttachments(
  files: File[],
  opts: FilesToAttachmentsOptions,
): Promise<Attachment[]> {
  const added: Attachment[] = [];
  for (const file of files) {
    if (file.type.startsWith("image/")) {
      try {
        const img = await processImageFile(file);
        added.push({
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          type: "image",
          fileName: file.name,
          mimeType: img.mimeType,
          data: img.data,
          size: file.size,
        });
        // 多模态预检：当前模型可能不支持图片 → 弱提示（不硬拦截发送）
        if (!supportsVision(opts.currentModel)) {
          opts.onHint(opts.t("chat.imageUnsupported"));
        }
      } catch (err) {
        opts.onHint(
          err instanceof AttachmentError && err.code === "imageTooLarge"
            ? opts.t("chat.imageTooLarge")
            : opts.t("chat.attachFailed"),
        );
      }
    } else if (isTextFile(file)) {
      try {
        const { content, truncated } = await readTextFile(file);
        added.push({
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          type: "text",
          fileName: file.name,
          mimeType: file.type || "text/plain",
          content,
          size: file.size,
          truncated,
        });
        if (truncated) opts.onHint(opts.t("chat.fileTooLarge"));
      } catch {
        opts.onHint(opts.t("chat.attachFailed"));
      }
    }
  }
  return added;
}

/**
 * 把草稿 + 附件组装成发送载荷：文本文件内容拼进 prompt 文本，
 * 图片走 images 数组（纯 base64，pi 会自己补 data:<mime>;base64, 前缀）。
 */
export function buildPromptPayload(
  text: string,
  attachments: Attachment[] | undefined,
  t: (key: string) => string,
): { text: string; images: ImageContent[] } {
  const atts = attachments || [];
  const imageAtts = atts.filter((a) => a.type === "image" && a.data);
  const textAtts = atts.filter((a) => a.type === "text");
  let full = text;
  for (const att of textAtts) {
    // v1：文本文件内容直接拼进 prompt（不走 attachments 字段）
    full += `\n\n[${t("chat.attachment")} ${att.fileName}]:\n${att.content ?? ""}`;
  }
  return {
    text: full,
    images: imageAtts.map((a) => ({
      type: "image" as const,
      data: stripDataUriPrefix(a.data as string),
      mimeType: a.mimeType,
    })),
  };
}
