// Getting an image ready to send: shrunk to fit what the agent takes, then
// kept by the core under the chat's next number. See PLAN.md, "Images".

import { saveImage, type ImageRef } from "../ipc";
import type { Chat } from "../state/chat.svelte";

/** Types the agent takes as they are. Anything else is converted. */
const TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];
/** Claude Code's limits: the longest side in pixels, and the size once base64-encoded. */
const MAX_SIDE = 2000;
const MAX_BASE64 = 5 * 1024 * 1024;
/** Largest original kept for looking at, once base64-encoded. */
const MAX_ORIGINAL = 60 * 1024 * 1024;

const base64Size = (bytes: number) => Math.ceil(bytes / 3) * 4;

function toBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).replace(/^data:[^,]*,/, ""));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

function encode(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob | null> {
  return new Promise((resolve) => canvas.toBlob(resolve, type, quality));
}

/**
 * The image as base64, as is when it already fits. Otherwise redrawn with its
 * longest side at most 2000px, as PNG when it was a PNG and that fits, else as
 * JPEG at falling quality. Throws when it can't be read or made small enough.
 */
export async function prepare(
  blob: Blob,
): Promise<{ mediaType: string; data: string; original?: { media_type: string; data: string } }> {
  const bitmap = await createImageBitmap(blob).catch(() => {
    throw new Error("That image couldn't be read");
  });
  try {
    const { width, height } = bitmap;
    if (TYPES.includes(blob.type) && Math.max(width, height) <= MAX_SIDE && base64Size(blob.size) <= MAX_BASE64) {
      return { mediaType: blob.type, data: await toBase64(blob) };
    }
    const scale = Math.min(1, MAX_SIDE / Math.max(width, height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(width * scale));
    canvas.height = Math.max(1, Math.round(height * scale));
    const draw = (background: string | null) => {
      const ctx = canvas.getContext("2d")!;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      // JPEG has no transparency; see-through parts would turn black.
      if (background) {
        ctx.fillStyle = background;
        ctx.fillRect(0, 0, canvas.width, canvas.height);
      }
      ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    };
    const tries: [string, number?][] = [
      ...(blob.type === "image/png" ? [["image/png"] as [string]] : []),
      ["image/jpeg", 0.85],
      ["image/jpeg", 0.7],
      ["image/jpeg", 0.55],
    ];
    for (const [type, quality] of tries) {
      draw(type === "image/jpeg" ? "#ffffff" : null);
      const out = await encode(canvas, type, quality);
      if (out && out.type === type && base64Size(out.size) <= MAX_BASE64) {
        // The full-size one is kept too, to look at, when it can be shown.
        const keep = TYPES.includes(blob.type) && base64Size(blob.size) <= MAX_ORIGINAL;
        const original = keep ? { media_type: blob.type, data: await toBase64(blob) } : undefined;
        return { mediaType: type, data: await toBase64(out), original };
      }
    }
    throw new Error("That image is too large to send");
  } finally {
    bitmap.close();
  }
}

/** Keeps an image for the chat's draft. Returns its `[image n]` word. */
export async function attach(chat: Chat, blob: Blob): Promise<string> {
  const { mediaType, data, original } = await prepare(blob);
  const image: ImageRef = await saveImage(chat.sessionId, mediaType, data, original);
  chat.draftImages.push(image);
  return `[image ${image.n}]`;
}

/** The image files among dropped or pasted ones. */
export function imageFiles(files: Iterable<File | null>): File[] {
  return [...files].filter((f): f is File => !!f && f.type.startsWith("image/"));
}
