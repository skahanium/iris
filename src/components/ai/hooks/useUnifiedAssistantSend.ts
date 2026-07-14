import { useCallback, useState } from "react";

import type { ImageAttachment } from "../AiMessageList";
import type {
  AssistantRunAccepted,
  AgentModelOverride,
  AssistantRunStartRequest,
  AssistantSessionRef,
  ContextReference,
  SecurityDomain,
} from "@/types/ai";

export interface UnifiedAssistantSendOptions {
  aiDomain: SecurityDomain;
  input: string;
  images: ImageAttachment[];
  composerDisabled: boolean;
  session: AssistantSessionRef | null;
  contextReferences: ContextReference[];
  webSearch: boolean;
  modelOverride?: AgentModelOverride | null;
  start: (request: AssistantRunStartRequest) => Promise<AssistantRunAccepted>;
  appendUserMessage: (message: string, images?: ImageAttachment[]) => void;
  ensureAssistantStreamSlot: () => void;
  clearContextReferences: () => void;
  setInput: (value: string) => void;
  setImages: (images: ImageAttachment[]) => void;
  setSession: (session: AssistantSessionRef) => void;
  setStreaming: (streaming: boolean) => void;
  setActivityHint: (hint: string | null) => void;
  setError: (message: string | null) => void;
}

function contentPartsForImages(
  message: string,
  images: ImageAttachment[],
): AssistantRunStartRequest["contentParts"] | undefined {
  if (images.length === 0) return undefined;
  return [
    { type: "text", text: message },
    ...images.map((image) => ({
      type: "image_url" as const,
      image_url: {
        url: `data:${image.mimeType};base64,${image.dataBase64}`,
        detail: "auto" as const,
      },
    })),
  ];
}

/** Starts the single production Run path from a user-authored prompt. */
export function useUnifiedAssistantSend({
  aiDomain,
  input,
  images,
  composerDisabled,
  session,
  contextReferences,
  webSearch,
  modelOverride,
  start,
  appendUserMessage,
  ensureAssistantStreamSlot,
  clearContextReferences,
  setInput,
  setImages,
  setSession,
  setStreaming,
  setActivityHint,
  setError,
}: UnifiedAssistantSendOptions) {
  const [isStarting, setIsStarting] = useState(false);

  const send = useCallback(async () => {
    const message = input.trim();
    if ((!message && images.length === 0) || composerDisabled || isStarting) {
      return;
    }
    if (!message) {
      setError("图片请求需要附带文字说明。");
      return;
    }

    const explicitReferences = contextReferences.filter(
      (reference) => !reference.stale && !reference.invalidReason,
    );
    const currentImages = images;
    setIsStarting(true);
    setError(null);
    setStreaming(true);
    setActivityHint("正在提交请求…");
    appendUserMessage(message, currentImages);
    ensureAssistantStreamSlot();

    try {
      const accepted = await start({
        clientRequestId: crypto.randomUUID(),
        ...(session ? { session } : {}),
        message,
        ...(currentImages.length > 0
          ? { contentParts: contentPartsForImages(message, currentImages) }
          : {}),
        explicitReferences,
        webEnabled: webSearch,
        securityDomain: aiDomain,
        ...(modelOverride ? { modelOverride } : {}),
      });
      setSession(accepted.session);
      setInput("");
      setImages([]);
      clearContextReferences();
      setActivityHint("正在准备回答…");
    } catch {
      setStreaming(false);
      setActivityHint(null);
      setError("请求未能提交，请稍后重试。");
    } finally {
      setIsStarting(false);
    }
  }, [
    aiDomain,
    appendUserMessage,
    clearContextReferences,
    composerDisabled,
    contextReferences,
    ensureAssistantStreamSlot,
    images,
    input,
    isStarting,
    session,
    setActivityHint,
    setError,
    setImages,
    setInput,
    setSession,
    setStreaming,
    start,
    webSearch,
    modelOverride,
  ]);

  return { isStarting, send };
}
