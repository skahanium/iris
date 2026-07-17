import { useCallback, useState } from "react";

import { invokeErrorMessage } from "@/lib/credentials";
import {
  mentionsToContextScope,
  trimMentionDraft,
} from "@/lib/ai-context-scope";
import { fileSignature } from "@/lib/ipc";

import type { ImageAttachment } from "../AiMessageList";
import type {
  AssistantRunAccepted,
  AgentModelOverride,
  AssistantRunStartRequest,
  AssistantTurnDraft,
  AssistantSessionRef,
  ContextReference,
  ContextScope,
  DisplayMention,
  SecurityDomain,
} from "@/types/ai";
import type { FileSignatureResult } from "@/types/ipc";

export interface UnifiedAssistantSendOptions {
  aiDomain: SecurityDomain;
  classifiedContextRef?: string | null;
  includeCurrentClassifiedDocument?: boolean;
  clearClassifiedDocumentConsent?: () => void;
  input: string;
  images: ImageAttachment[];
  composerDisabled: boolean;
  session: AssistantSessionRef | null;
  contextReferences: ContextReference[];
  displayMentions: DisplayMention[];
  retrievalScope: ContextScope;
  webSearch: boolean;
  modelOverride?: AgentModelOverride | null;
  start: (request: AssistantRunStartRequest) => Promise<AssistantRunAccepted>;
  getFileSignature?: (path: string) => Promise<FileSignatureResult>;
  appendUserMessage: (
    message: string,
    images?: ImageAttachment[],
    displayMentions?: DisplayMention[],
  ) => void;
  ensureAssistantStreamSlot: () => void;
  clearContextReferences: () => void;
  setInput: (value: string) => void;
  setImages: (images: ImageAttachment[]) => void;
  setSession: (session: AssistantSessionRef | null) => void;
  setStreaming: (streaming: boolean) => void;
  setActivityHint: (hint: string | null) => void;
  setError: (message: string | null) => void;
}

async function referencesForFileMentions(
  mentions: readonly DisplayMention[],
  getFileSignature: (path: string) => Promise<FileSignatureResult>,
): Promise<ContextReference[]> {
  const paths = [
    ...new Set(
      mentions
        .filter((mention) => mention.kind === "file")
        .map((mention) => mention.value),
    ),
  ];
  return Promise.all(
    paths.map(async (path) => {
      const signature = await getFileSignature(path);
      return {
        id: crypto.randomUUID(),
        kind: "note" as const,
        filePath: path,
        contentHash: signature.contentHash,
        utf8Range: null,
        editorRange: null,
        excerpt: "",
        stale: false,
      };
    }),
  );
}

function hasRetrievalScope(scope: ContextScope): boolean {
  return (
    scope.paths.length > 0 ||
    scope.pathPrefixes.length > 0 ||
    (scope.corpusIds?.length ?? 0) > 0 ||
    (scope.requiredTags?.length ?? 0) > 0
  );
}

function contentPartsForImages(
  message: string,
  images: ImageAttachment[],
): AssistantTurnDraft["contentParts"] | undefined {
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

function classifiedSubmissionError(reason: unknown): string {
  const message = invokeErrorMessage(reason);
  if (message.includes("agent_run_classified_context_required"))
    return "请先明确附带当前打开的涉密文档。";
  if (message.includes("agent_run_classified_context_expired"))
    return "当前涉密文档上下文已失效，请重新附带。";
  if (message.includes("agent_run_classified_vault_locked"))
    return "涉密保险库已锁定，请解锁后重试。";
  if (message.includes("agent_run_permission_denied"))
    return "当前涉密文档未获授权读取或发送给模型。";
  return "请求未能提交，请稍后重试。";
}

/** Starts the single production Run path from a user-authored prompt. */
export function useUnifiedAssistantSend({
  aiDomain,
  classifiedContextRef,
  includeCurrentClassifiedDocument = false,
  clearClassifiedDocumentConsent,
  input,
  images,
  composerDisabled,
  session,
  contextReferences,
  displayMentions,
  retrievalScope,
  webSearch,
  modelOverride,
  start,
  getFileSignature = fileSignature,
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
    const draft = trimMentionDraft(input, displayMentions);
    const message = draft.message;
    if ((!message && images.length === 0) || composerDisabled || isStarting) {
      return;
    }
    if (!message) {
      setError("图片请求需要附带文字说明。");
      return;
    }
    if (aiDomain === "classified") {
      if (!includeCurrentClassifiedDocument || !classifiedContextRef) {
        setError("请先点击“引用当前涉密文档”，该授权仅对本次提问生效。");
        return;
      }
      if (
        images.length > 0 ||
        contextReferences.length > 0 ||
        draft.displayMentions.length > 0 ||
        hasRetrievalScope(retrievalScope) ||
        webSearch
      ) {
        setError("涉密分析仅支持当前文档文本，不支持图片、其他引用或联网。");
        return;
      }
    }

    const explicitReferences = contextReferences.filter(
      (reference) => !reference.stale && !reference.invalidReason,
    );
    const currentImages = images;
    setIsStarting(true);
    setError(null);

    try {
      const mentionReferences =
        aiDomain === "classified"
          ? []
          : await referencesForFileMentions(
              draft.displayMentions,
              getFileSignature,
            );
      const turnScope =
        aiDomain === "classified"
          ? { paths: [], pathPrefixes: [], requiredTags: [] }
          : mentionsToContextScope(draft.displayMentions);
      setStreaming(true);
      setActivityHint("正在提交请求…");
      appendUserMessage(message, currentImages, draft.displayMentions);
      ensureAssistantStreamSlot();
      const accepted = await start({
        clientRequestId: crypto.randomUUID(),
        ...(session ? { session } : {}),
        turn: {
          message,
          ...(currentImages.length > 0
            ? { contentParts: contentPartsForImages(message, currentImages) }
            : {}),
          explicitReferences:
            aiDomain === "classified"
              ? []
              : [...explicitReferences, ...mentionReferences],
          retrievalScope: turnScope,
          displayMentions:
            aiDomain === "classified" ? [] : draft.displayMentions,
        },
        webEnabled: aiDomain === "classified" ? false : webSearch,
        securityDomain: aiDomain,
        ...(aiDomain === "classified" && classifiedContextRef
          ? { classifiedContextRef }
          : {}),
        ...(modelOverride ? { modelOverride } : {}),
      });
      setSession(aiDomain === "classified" ? null : accepted.session);
      setInput("");
      setImages([]);
      clearContextReferences();
      if (aiDomain === "classified") clearClassifiedDocumentConsent?.();
      setActivityHint("正在准备回答…");
    } catch (reason) {
      setStreaming(false);
      setActivityHint(null);
      if (aiDomain === "classified") {
        setError(classifiedSubmissionError(reason));
        return;
      }
      setError("请求未能提交，请稍后重试。");
    } finally {
      setIsStarting(false);
    }
  }, [
    aiDomain,
    classifiedContextRef,
    appendUserMessage,
    clearContextReferences,
    composerDisabled,
    contextReferences,
    displayMentions,
    ensureAssistantStreamSlot,
    images,
    input,
    includeCurrentClassifiedDocument,
    getFileSignature,
    clearClassifiedDocumentConsent,
    isStarting,
    session,
    setActivityHint,
    setError,
    setImages,
    setInput,
    setSession,
    setStreaming,
    start,
    retrievalScope,
    webSearch,
    modelOverride,
  ]);

  return { isStarting, send };
}
