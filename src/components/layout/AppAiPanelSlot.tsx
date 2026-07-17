import { lazy, Suspense, useMemo } from "react";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import type { AiDomain, ContextReference } from "@/types/ai";
import type { FileListItem } from "@/types/ipc";

const UnifiedAssistantPanel = lazy(() =>
  import("@/components/ai/UnifiedAssistantPanel").then((m) => ({
    default: m.UnifiedAssistantPanel,
  })),
);

function AssistantPanelLoading() {
  return (
    <div
      className="ai-sidecar flex min-h-0 flex-1 items-center justify-center px-4 text-xs text-muted-foreground"
      aria-live="polite"
      role="status"
    >
      AI 面板加载中…
    </div>
  );
}

interface AppAiPanelSlotProps {
  aiDomain: AiDomain;
  classifiedPath: string | null;
  consumeEditorSelectionReference?: () => void;
  editorSelectionReference?: ContextReference | null;
  editorInteractionLocked?: boolean;
  runtimeDocumentCandidates?: FileListItem[];
  handleInsertToEditor: (content: string) => void;
  webSearch: boolean;
  webSearchProviderName?: string | null;
  onOpenWebVerificationSettings?: () => void;
}

/** Lazily loads the Run-only side panel without passing implicit document state. */
export function AppAiPanelSlot({
  aiDomain,
  classifiedPath,
  consumeEditorSelectionReference,
  editorSelectionReference = null,
  editorInteractionLocked = false,
  runtimeDocumentCandidates = [],
  handleInsertToEditor,
  webSearch,
  webSearchProviderName = null,
  onOpenWebVerificationSettings,
}: AppAiPanelSlotProps) {
  const mentionRuntimeCandidates = useMemo(
    () =>
      runtimeDocumentCandidates.filter((candidate) =>
        candidate.path.trim().endsWith(".md"),
      ),
    [runtimeDocumentCandidates],
  );

  return (
    <ErrorBoundary scope="AI面板">
      <Suspense fallback={<AssistantPanelLoading />}>
        <UnifiedAssistantPanel
          aiDomain={aiDomain}
          classifiedPath={classifiedPath}
          oneShotContextReference={editorSelectionReference}
          consumeOneShotContextReference={consumeEditorSelectionReference}
          runtimeDocumentCandidates={mentionRuntimeCandidates}
          webSearch={webSearch}
          webSearchProviderName={webSearchProviderName}
          onOpenWebVerificationSettings={onOpenWebVerificationSettings}
          onInsertToEditor={
            editorInteractionLocked ? undefined : handleInsertToEditor
          }
        />
      </Suspense>
    </ErrorBoundary>
  );
}
