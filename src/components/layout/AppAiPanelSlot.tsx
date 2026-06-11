import type { MutableRefObject } from "react";

import { UnifiedAssistantPanel } from "@/components/ai/UnifiedAssistantPanel";
import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { resolveNoteDisplayTitle } from "@/lib/note-display";
import type { WritingEditorContext } from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

interface AppAiPanelSlotProps {
  activeDocumentTitle: string | null;
  activeNoteIsClassified: boolean;
  activePathRef: MutableRefObject<string | null>;
  assistantDocumentTitle: string | null;
  assistantNotePath: string | null;
  assistantPrefill: string | null;
  bumpVaultIndex: () => void;
  dirtyRef: MutableRefObject<boolean>;
  getLiveMarkdown: () => string;
  getParagraphText: () => string | null;
  getWritingContext: () => WritingEditorContext | null;
  handleInsertToEditor: (content: string) => void;
  markClean: (path: string, title: string) => void;
  markdownRef: MutableRefObject<string>;
  selectionQuote: AssistantSelectionQuote | null;
  setAiStatus: (status: string) => void;
  setAssistantChrome: (snapshot: AssistantChromeSnapshot) => void;
  syncTabMarkdownCache: (path: string, markdown: string) => void;
  webSearch: boolean;
  applyMarkdownToEditor: (content: string) => void;
}

export function AppAiPanelSlot({
  activeDocumentTitle,
  activeNoteIsClassified,
  activePathRef,
  assistantDocumentTitle,
  assistantNotePath,
  assistantPrefill,
  bumpVaultIndex,
  dirtyRef,
  getLiveMarkdown,
  getParagraphText,
  getWritingContext,
  handleInsertToEditor,
  markClean,
  markdownRef,
  selectionQuote,
  setAiStatus,
  setAssistantChrome,
  syncTabMarkdownCache,
  webSearch,
  applyMarkdownToEditor,
}: AppAiPanelSlotProps) {
  return (
    <ErrorBoundary scope="AI面板">
      <UnifiedAssistantPanel
        notePath={assistantNotePath}
        noteDisplayTitle={assistantDocumentTitle}
        getNoteContent={getLiveMarkdown}
        webSearch={webSearch}
        getWritingContext={getWritingContext}
        getParagraphText={getParagraphText}
        selectionQuote={activeNoteIsClassified ? null : selectionQuote}
        prefillMessage={assistantPrefill}
        onChromeChange={setAssistantChrome}
        onVaultRefresh={bumpVaultIndex}
        onInsertToEditor={handleInsertToEditor}
        onPatchApplied={(newContent: string) => {
          if (activeNoteIsClassified) {
            setAiStatus("涉密笔记不能接收 AI 改写");
            return;
          }
          applyMarkdownToEditor(newContent);
          markdownRef.current = newContent;
          dirtyRef.current = false;
          const path = activePathRef.current;
          if (path) {
            syncTabMarkdownCache(path, newContent);
            markClean(
              path,
              resolveNoteDisplayTitle({
                path,
                title: activeDocumentTitle ?? undefined,
              }),
            );
          }
        }}
      />
    </ErrorBoundary>
  );
}
