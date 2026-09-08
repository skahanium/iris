import { assistantContentHash } from "@/lib/assistant-stream-buffer";

export const MARKDOWN_WORKER_RENDER_CHARS = 36_000;

export interface RenderableAssistantContent {
  content: string;
  fullHash: string;
  fullLength: number;
  omittedChars: number;
  streaming: boolean;
  truncated: boolean;
}

/** The assistant renderer windows DOM while retaining the full source. */
export function createStreamingRenderableContent(content: string): string {
  return content;
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

  return {
    content,
    fullHash,
    fullLength,
    omittedChars: 0,
    streaming,
    truncated: false,
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
