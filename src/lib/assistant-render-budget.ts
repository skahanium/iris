import { assistantContentHash } from "@/lib/assistant-stream-buffer";

export const ASSISTANT_RENDER_FULL_LIMIT = 80_000;
export const ASSISTANT_RENDER_PREFIX_CHARS = 12_000;
export const ASSISTANT_RENDER_SUFFIX_CHARS = 26_000;
export const ASSISTANT_STREAM_RENDER_TAIL_CHARS = 32_000;
export const MARKDOWN_WORKER_RENDER_CHARS = 36_000;

export interface RenderableAssistantContent {
  content: string;
  fullHash: string;
  fullLength: number;
  omittedChars: number;
  streaming: boolean;
  truncated: boolean;
}

function alignForwardToLine(value: string, index: number): number {
  const nextBreak = value.indexOf("\n", index);
  if (nextBreak < 0 || nextBreak - index > 800) return index;
  return nextBreak + 1;
}

function alignBackwardToLine(value: string, index: number): number {
  const previousBreak = value.lastIndexOf("\n", index);
  if (previousBreak < 0 || index - previousBreak > 800) return index;
  return previousBreak + 1;
}

function truncationNotice(omittedChars: number, streaming: boolean): string {
  const phase = streaming ? "streaming" : "render";
  return [
    "",
    `> [!note] truncated for memory safety (${phase}); ${omittedChars} characters hidden from this view.`,
    "",
  ].join("\n");
}

export function createRenderableAssistantContent(
  content: string,
  options?: { streaming?: boolean; maxChars?: number },
): RenderableAssistantContent {
  const streaming = options?.streaming ?? false;
  const fullLength = content.length;
  const fullHash = assistantContentHash(content);
  const maxChars = options?.maxChars;

  if (typeof maxChars === "number") {
    const budget = Math.max(0, Math.floor(maxChars));
    if (fullLength <= budget) {
      return {
        content,
        fullHash,
        fullLength,
        omittedChars: 0,
        streaming,
        truncated: false,
      };
    }
    const windowContent = content.slice(Math.max(0, fullLength - budget));
    return {
      content: windowContent,
      fullHash,
      fullLength,
      omittedChars: fullLength - windowContent.length,
      streaming,
      truncated: true,
    };
  }

  if (!streaming && fullLength <= ASSISTANT_RENDER_FULL_LIMIT) {
    return {
      content,
      fullHash,
      fullLength,
      omittedChars: 0,
      streaming,
      truncated: false,
    };
  }

  if (streaming) {
    if (fullLength <= ASSISTANT_STREAM_RENDER_TAIL_CHARS) {
      return {
        content,
        fullHash,
        fullLength,
        omittedChars: 0,
        streaming,
        truncated: false,
      };
    }

    const suffixStart = alignBackwardToLine(
      content,
      Math.max(0, fullLength - ASSISTANT_STREAM_RENDER_TAIL_CHARS),
    );
    const suffix = content.slice(suffixStart);
    const omittedChars = fullLength - suffix.length;
    return {
      content: `${truncationNotice(omittedChars, true)}${suffix}`,
      fullHash,
      fullLength,
      omittedChars,
      streaming,
      truncated: true,
    };
  }

  const prefixEnd = alignForwardToLine(
    content,
    Math.min(fullLength, ASSISTANT_RENDER_PREFIX_CHARS),
  );
  const suffixStart = alignBackwardToLine(
    content,
    Math.max(prefixEnd, fullLength - ASSISTANT_RENDER_SUFFIX_CHARS),
  );
  const prefix = content.slice(0, prefixEnd);
  const suffix = content.slice(suffixStart);
  const omittedChars = Math.max(0, fullLength - prefix.length - suffix.length);
  return {
    content: `${prefix}${truncationNotice(omittedChars, false)}${suffix}`,
    fullHash,
    fullLength,
    omittedChars,
    streaming,
    truncated: true,
  };
}

export function createWorkerRenderableContent(
  content: string,
): RenderableAssistantContent {
  return createRenderableAssistantContent(content, {
    maxChars: MARKDOWN_WORKER_RENDER_CHARS,
    streaming: true,
  });
}
