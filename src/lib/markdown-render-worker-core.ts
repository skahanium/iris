import { contentHash64 } from "@/lib/content-hash";
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
  return contentHash64(content);
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
