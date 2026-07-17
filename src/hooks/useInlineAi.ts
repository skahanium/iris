import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useRef } from "react";

import {
  createEditorContextReference,
  EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
  type EditorContextReferenceResult,
} from "@/lib/context-reference";
import {
  buildSlashCommandMessage,
  parseSlashActionId,
  slashActionId,
} from "@/lib/slash-command-prompts";
import { buildInlineAiSelectionReferentPrompt } from "@/lib/inline-ai-prompts";
import {
  assistantRunControl,
  assistantRunStart,
  listenAssistantRunEvent,
} from "@/lib/ipc";
import type {
  AiDomain,
  AssistantSessionRef,
  ContextReference,
} from "@/types/ai";

export const INLINE_AI_INSERT_AFTER_SELECTION = "insert_after_selection";
export const INLINE_AI_REPLACE_SELECTION = "replace_selection";

export interface UseInlineAiOptions {
  domain?: AiDomain;
  isDocumentDirty?: () => boolean;
  isMutationBlocked?: () => boolean;
  onStatus?: (status: string) => void;
}

export interface AiStreamRequest {
  action: string;
  originalText: string;
  message: string;
}

export function getActiveAiStreamAttrs(
  editor: Editor,
): { originalText: string; action: string } | null {
  let result: { originalText: string; action: string } | null = null;
  editor.state.doc.descendants((node) => {
    if (node.type.name !== "aiStream" || result) return;
    const attrs = node.attrs as { originalText?: unknown; action?: unknown };
    if (typeof attrs.action === "string" && attrs.action) {
      result = {
        action: attrs.action,
        originalText:
          typeof attrs.originalText === "string" ? attrs.originalText : "",
      };
    }
  });
  return result;
}

export function buildRetryRequest(ctx: {
  originalText: string;
  action: string;
}): AiStreamRequest {
  const slash = parseSlashActionId(ctx.action);
  return {
    action: ctx.action,
    originalText: ctx.originalText,
    message: slash
      ? buildSlashCommandMessage(slash)
      : buildInlineAiSelectionReferentPrompt(ctx.action),
  };
}

/** Use the same disk-verified projection factory as the assistant sidecar. */
export async function buildInlineSelectionReference(
  editor: Editor,
  isDirty: () => boolean = () => false,
): Promise<EditorContextReferenceResult> {
  return createEditorContextReference({
    editor,
    kind: "selection",
    isDirty,
  });
}

interface ActiveInlineRun {
  runId: string;
  stateVersion: number;
  session: AssistantSessionRef;
  reference: ContextReference | null;
}

/** Inline AI presents the same persistent Run lifecycle as the assistant panel. */
export function useInlineAi({
  domain = "normal",
  isDocumentDirty = () => false,
  isMutationBlocked = () => false,
  onStatus,
}: UseInlineAiOptions = {}) {
  const activeRef = useRef<ActiveInlineRun | null>(null);
  const bufferRef = useRef("");
  const rafRef = useRef<number | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);
  const isMutationBlockedRef = useRef(isMutationBlocked);
  isMutationBlockedRef.current = isMutationBlocked;
  const isDocumentDirtyRef = useRef(isDocumentDirty);
  isDocumentDirtyRef.current = isDocumentDirty;

  const mutationBlocked = useCallback(() => isMutationBlockedRef.current(), []);

  const detach = useCallback(() => {
    if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    rafRef.current = null;
    unlistenRef.current?.();
    unlistenRef.current = null;
  }, []);

  const flush = useCallback(
    (editor: Editor) => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
      if (mutationBlocked()) return;
      editor.commands.updateAiStream(bufferRef.current);
    },
    [mutationBlocked],
  );

  const start = useCallback(
    async (
      editor: Editor,
      request: AiStreamRequest,
      reference?: ContextReference | null,
    ) => {
      if (mutationBlocked()) return;
      detach();
      bufferRef.current = "";
      editor.commands.clearAiStreamContent();
      editor.commands.setAiStreamStatus("streaming");
      onStatus?.("AI 正在处理…");

      try {
        const accepted = await assistantRunStart({
          clientRequestId: crypto.randomUUID(),
          turn: {
            message: request.message,
            explicitReferences: reference ? [reference] : [],
            retrievalScope: { paths: [], pathPrefixes: [] },
            displayMentions: [],
          },
          explicitAction: reference
            ? {
                effect: "draft",
                target: reference.contentHash
                  ? {
                      referenceId: reference.id,
                      contentHash: reference.contentHash,
                    }
                  : undefined,
              }
            : { effect: "draft" },
          webEnabled: false,
          securityDomain: domain,
        });
        activeRef.current = {
          runId: accepted.runId,
          stateVersion: accepted.stateVersion,
          session: accepted.session,
          reference: reference ?? null,
        };
        unlistenRef.current = await listenAssistantRunEvent((event) => {
          const active = activeRef.current;
          if (!active || event.runId !== active.runId) return;
          active.stateVersion = event.stateVersion;
          if (event.type === "content_delta") {
            bufferRef.current += event.payload.delta;
            if (rafRef.current === null) {
              rafRef.current = requestAnimationFrame(() => {
                rafRef.current = null;
                if (mutationBlocked()) return;
                editor.commands.updateAiStream(bufferRef.current);
              });
            }
            return;
          }
          if (event.type === "completed") {
            flush(editor);
            if (mutationBlocked()) return;
            editor.commands.setAiStreamStatus("ready");
            onStatus?.("AI 空闲");
            return;
          }
          if (event.type === "failed" || event.type === "cancelled") {
            flush(editor);
            if (mutationBlocked()) return;
            editor.commands.setAiStreamStatus("error");
            onStatus?.(
              event.type === "failed"
                ? `AI 错误: ${event.payload.message}`
                : "AI 已取消",
            );
          }
        });
      } catch (error) {
        if (mutationBlocked()) return;
        editor.commands.setAiStreamStatus("error");
        onStatus?.(
          `AI 错误: ${error instanceof Error ? error.message : "无法启动 Run"}`,
        );
      }
    },
    [detach, domain, flush, mutationBlocked, onStatus],
  );

  const run = useCallback(
    async (editor: Editor, action: string) => {
      if (mutationBlocked()) return;
      const { from, to } = editor.state.selection;
      const originalText = editor.state.doc.textBetween(from, to, "\n").trim();
      if (!originalText) return;
      const referenceResult = await buildInlineSelectionReference(editor, () =>
        isDocumentDirtyRef.current(),
      );
      if (!referenceResult.ok) {
        onStatus?.(referenceResult.message);
        return;
      }
      editor.commands.insertAiStreamForSelection({ originalText, action });
      await start(
        editor,
        {
          action,
          originalText,
          message: buildInlineAiSelectionReferentPrompt(action),
        },
        referenceResult.reference,
      );
    },
    [mutationBlocked, onStatus, start],
  );

  const runSlash = useCallback(
    async (editor: Editor, command: string) => {
      if (mutationBlocked()) return;
      const action = slashActionId(command);
      editor.commands.insertAiStreamAtCursor({ originalText: "", action });
      await start(editor, {
        action,
        originalText: "",
        message: buildSlashCommandMessage(command),
      });
    },
    [mutationBlocked, start],
  );

  const retry = useCallback(
    async (editor: Editor) => {
      if (mutationBlocked()) return;
      const context = getActiveAiStreamAttrs(editor);
      if (!context) return;
      const reference = activeRef.current?.reference ?? null;
      if (reference && isDocumentDirtyRef.current()) {
        onStatus?.(EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE);
        return;
      }
      await start(
        editor,
        buildRetryRequest(context),
        context.originalText ? reference : null,
      );
    },
    [mutationBlocked, onStatus, start],
  );

  const abort = useCallback(async () => {
    const active = activeRef.current;
    if (!active) return;
    await assistantRunControl({
      session: active.session,
      runId: active.runId,
      expectedStateVersion: active.stateVersion,
      action: { type: "cancel" },
    });
  }, []);

  const abortAndDetach = useCallback(() => {
    const active = activeRef.current;
    activeRef.current = null;
    bufferRef.current = "";
    detach();
    if (!active) return;
    void Promise.resolve(
      assistantRunControl({
        session: active.session,
        runId: active.runId,
        expectedStateVersion: active.stateVersion,
        action: { type: "cancel" },
      }),
    ).catch(() => undefined);
  }, [detach]);

  const dismiss = useCallback((_editor?: Editor) => void abort(), [abort]);

  const finish = useCallback(() => {
    activeRef.current = null;
    bufferRef.current = "";
    detach();
    onStatus?.("AI 空闲");
  }, [detach, onStatus]);

  useEffect(() => () => detach(), [detach]);

  return { run, runSlash, retry, abort, abortAndDetach, dismiss, finish };
}
