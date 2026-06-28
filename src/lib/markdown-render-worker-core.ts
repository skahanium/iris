import { renderMarkdownWithProfile } from "@/lib/markdown-contract";

export interface MarkdownRenderRequest {
  type: "render";
  id: number;
  profile: "chat_assistant";
  content: string;
  streaming: boolean;
}

export interface MarkdownAbortRequest {
  type: "abort";
  id: number;
}

export type MarkdownRenderWorkerRequest =
  | MarkdownRenderRequest
  | MarkdownAbortRequest;

export type MarkdownRenderWorkerResponse =
  | {
      type: "rendered";
      id: number;
      html: string;
      contentHash: string;
      renderedLength: number;
    }
  | {
      type: "skipped";
      id: number;
      reason: "duplicate" | "aborted";
    }
  | {
      type: "error";
      id: number;
      message: string;
    };

export function markdownContentHash(content: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < content.length; i += 1) {
    hash ^= content.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

export function renderMarkdownForWorker(
  request: MarkdownRenderRequest,
): MarkdownRenderWorkerResponse {
  try {
    const result = renderMarkdownWithProfile(request.content, request.profile, {
      streaming: request.streaming,
    });
    return {
      type: "rendered",
      id: request.id,
      html: result.output,
      contentHash: markdownContentHash(request.content),
      renderedLength: request.content.length,
    };
  } catch (error: unknown) {
    return {
      type: "error",
      id: request.id,
      message: error instanceof Error ? error.message : String(error),
    };
  }
}
