import type { Editor } from "@tiptap/react";
import { useCallback, useMemo, type RefObject } from "react";

import { runEditorAction } from "@/lib/editor-action-executor";
import { insertAssistantMarkdownAtCursor } from "@/lib/editor-insert";

interface InlineAiPort {
  run: (editor: Editor, action: string) => Promise<void>;
  runSlash: (editor: Editor, command: string) => Promise<void>;
}

interface UseAppEditorActionsParams {
  activeNoteIsClassified: boolean;
  activePathRef: RefObject<string | null>;
  editorRef: RefObject<Editor | null>;
  getLiveMarkdown: () => string;
  inlineAi: InlineAiPort;
  isMutationBlocked: () => boolean;
  scheduleUndoRedoStateRefresh: (editor?: Editor | null) => void;
  sendSelectionToAi: (options?: { prefill?: string }) => void;
  setAiStatus: (message: string) => void;
}

export function useAppEditorActions({
  activeNoteIsClassified: _activeNoteIsClassified,
  activePathRef,
  editorRef,
  getLiveMarkdown: _getLiveMarkdown,
  inlineAi,
  isMutationBlocked,
  scheduleUndoRedoStateRefresh,
  sendSelectionToAi,
  setAiStatus,
}: UseAppEditorActionsParams) {
  const rejectWhenMutationBlocked = useCallback(() => {
    if (!isMutationBlocked()) return false;
    setAiStatus("文档正在保存，编辑操作已暂停");
    return true;
  }, [isMutationBlocked, setAiStatus]);

  const runInlineAi = useCallback(
    (action: string) => {
      if (rejectWhenMutationBlocked()) return;
      const ed = editorRef.current;
      if (!ed) return;
      void inlineAi.run(ed, action);
    },
    [editorRef, inlineAi, rejectWhenMutationBlocked],
  );

  const handleSlashCommand = useCallback(
    (command: string) => {
      if (rejectWhenMutationBlocked()) return;
      if (!editorRef.current) return;
      void inlineAi.runSlash(editorRef.current, command);
    },
    [editorRef, inlineAi, rejectWhenMutationBlocked],
  );

  const editorActionHandlers = useMemo(
    () => ({
      onInlineAi: (action: string) => runInlineAi(action),
      onSlashCommand: (command: string) => handleSlashCommand(command),
      onSendToAi: (options?: { prefill?: string }) =>
        sendSelectionToAi(options),
      onStatus: (message: string) => setAiStatus(message),
    }),
    [handleSlashCommand, runInlineAi, sendSelectionToAi, setAiStatus],
  );

  const runEditorActionById = useCallback(
    (actionId: string) => {
      if (rejectWhenMutationBlocked()) return;
      void runEditorAction(actionId, editorRef.current, editorActionHandlers);
    },
    [editorActionHandlers, editorRef, rejectWhenMutationBlocked],
  );

  const handleInsertToEditor = useCallback(
    (content: string) => {
      if (rejectWhenMutationBlocked()) return;
      const ed = editorRef.current;
      const path = activePathRef.current;
      if (!ed || !path) return;
      insertAssistantMarkdownAtCursor(ed, content);
    },
    [activePathRef, editorRef, rejectWhenMutationBlocked],
  );

  const handleUndo = useCallback(() => {
    if (rejectWhenMutationBlocked()) return;
    const ed = editorRef.current;
    if (!ed || !ed.can().undo()) return;
    ed.commands.undo();
    scheduleUndoRedoStateRefresh(ed);
  }, [editorRef, rejectWhenMutationBlocked, scheduleUndoRedoStateRefresh]);

  const handleRedo = useCallback(() => {
    if (rejectWhenMutationBlocked()) return;
    const ed = editorRef.current;
    if (!ed || !ed.can().redo()) return;
    ed.commands.redo();
    scheduleUndoRedoStateRefresh(ed);
  }, [editorRef, rejectWhenMutationBlocked, scheduleUndoRedoStateRefresh]);

  return {
    handleInsertToEditor,
    handleRedo,
    handleUndo,
    runEditorActionById,
  };
}
