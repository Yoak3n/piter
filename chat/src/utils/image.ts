import type { ImageContent } from "../types";

// ─── Limits ────────────────────────────────────────────────────────────────
/** 原图超过该字节数直接拒绝（不压缩，提示用户） */
export const IMAGE_MAX_BYTES = 8 * 1024 * 1024;
/** 压缩后的目标上限（超过则再降一档） */
export const IMAGE_COMPRESS_MAX_BYTES = 2 * 1024 * 1024;
/** 首轮压缩：最长边上限 */
export const IMAGE_MAX_EDGE = 1024;
/** 降级压缩：最长边上限 */
export const IMAGE_MAX_EDGE_FALLBACK = 800;
/** toDataURL 质量参数 */
export const IMAGE_QUALITY = 0.8;
/** 文本附件读取上限（超过截断并提示） */
export const TEXT_FILE_MAX_BYTES = 200 * 1024;

/** Attachment processing error with a stable machine-readable code. */
export class AttachmentError extends Error {
  readonly code: "imageTooLarge" | "fileReadFailed";
  constructor(code: "imageTooLarge" | "fileReadFailed") {
    super(code);
    this.name = "AttachmentError";
    this.code = code;
  }
}

export function readFileAsDataURL(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(new AttachmentError("fileReadFailed"));
    reader.readAsDataURL(file);
  });
}

/** Extract the mime type from a `data:<mime>;base64,...` URI header. */
export function mimeFromDataUrl(dataUrl: string): string {
  const m = /^data:([^;,]+)/.exec(dataUrl);
  return m ? m[1] : "image/png";
}

/**
 * Strip the `data:<mime>;base64,` prefix, returning the raw base64 payload.
 * The pi protocol's `ImageContent.data` is pure base64 — pi re-adds the
 * `data:<mime>;base64,` prefix itself when building provider requests, so
 * sending a full data URI would produce a doubled prefix and break upstream.
 */
export function stripDataUriPrefix(data: string): string {
  if (/^data:[^,]*;base64/i.test(data)) {
    const comma = data.indexOf(",");
    if (comma >= 0) return data.slice(comma + 1);
  }
  return data;
}

/**
 * Build a displayable `<img src>` from an ImageContent: passthrough for data
 * URIs (legacy snapshots), otherwise re-prefix the pure base64 with the mime.
 */
export function imageContentToSrc(img: { data?: string; mimeType?: string }): string {
  if (!img.data) return "";
  if (img.data.startsWith("data:")) return img.data;
  return `data:${img.mimeType || "image/png"};base64,${img.data}`;
}

function base64Bytes(dataUrl: string): number {
  const comma = dataUrl.indexOf(",");
  const b64 = comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
  return Math.floor((b64.length * 3) / 4);
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new AttachmentError("fileReadFailed"));
    img.src = src;
  });
}

/** Re-encode an image via canvas so the longest edge ≤ maxEdge. */
function scaleImage(img: HTMLImageElement, dataUrl: string, maxEdge: number): string {
  const scale = Math.min(1, maxEdge / Math.max(img.naturalWidth, img.naturalHeight));
  const w = Math.max(1, Math.round(img.naturalWidth * scale));
  const h = Math.max(1, Math.round(img.naturalHeight * scale));
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return dataUrl;
  ctx.drawImage(img, 0, 0, w, h);
  return canvas.toDataURL(mimeFromDataUrl(dataUrl), IMAGE_QUALITY);
}

/**
 * Compress an image data URI: scale the longest edge to ≤1024px at quality
 * 0.8; if the result is still over 2MB, scale down to ≤800px. Small images
 * that already fit the byte budget are returned unchanged.
 */
export async function compressImage(dataUrl: string): Promise<string> {
  const img = await loadImage(dataUrl);
  const longest = Math.max(img.naturalWidth, img.naturalHeight);
  if (longest <= IMAGE_MAX_EDGE && base64Bytes(dataUrl) <= IMAGE_COMPRESS_MAX_BYTES) {
    return dataUrl;
  }
  let out = scaleImage(img, dataUrl, IMAGE_MAX_EDGE);
  if (base64Bytes(out) > IMAGE_COMPRESS_MAX_BYTES) {
    out = scaleImage(img, dataUrl, IMAGE_MAX_EDGE_FALLBACK);
  }
  return out;
}

/**
 * Turn an image File into an ImageContent for the prompt payload.
 * Rejects with `AttachmentError("imageTooLarge")` when the original is >8MB.
 * The returned `data` is pure base64 (no `data:` prefix), matching the pi
 * protocol — pi re-adds the `data:<mime>;base64,` prefix per provider.
 */
export async function processImageFile(file: File): Promise<ImageContent> {
  if (file.size > IMAGE_MAX_BYTES) throw new AttachmentError("imageTooLarge");
  const dataUrl = await readFileAsDataURL(file);
  const data = await compressImage(dataUrl);
  return { type: "image", data: stripDataUriPrefix(data), mimeType: mimeFromDataUrl(data) };
}

/** Read a text file, truncating content past `TEXT_FILE_MAX_BYTES`. */
export async function readTextFile(
  file: File,
): Promise<{ content: string; truncated: boolean }> {
  const raw = await file.text();
  if (raw.length > TEXT_FILE_MAX_BYTES) {
    return { content: raw.slice(0, TEXT_FILE_MAX_BYTES), truncated: true };
  }
  return { content: raw, truncated: false };
}

/** Human-readable byte size (e.g. "1.5 MB"). */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
