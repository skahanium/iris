import { lazy, Suspense, useMemo } from "react";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import type { AiDomain, ContextReference } from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { FileListItem } from "@/types/ipc";

const UnifiedAssistantPanel = lazy(() =>
  import("@/components/ai/UnifiedAssistantPanel").then((m) => ({
    default: m.UnifiedAssistantPanel,
  })),
);

function AssistantPanelLoading() {
  return (
    <div
      className="ai-sidecar flex min-h-0 flex-1 flex-col gap-3 px-4 py-5"
      aria-live="polite"
      role="status"
      aria-label="AI 面板加载中"
    >
      <div className="h-3 w-24 animate-pulse rounded bg-muted/70" />
      <div className="space-y-2">
        <div className="h-3 w-full animate-pulse rounded bg-muted/50" />
        <div className="h-3 w-[88%] animate-pulse rounded bg-muted/45" />
        <div className="h-3 w-[72%] animate-pulse rounded bg-muted/40" />
      </div>
      <div className="mt-auto h-16 animate-pulse rounded-lg border border-border/50 bg-muted/30" />
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
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
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
  onChromeChange,
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
          onChromeChange={onChromeChange}
          onInsertToEditor={
            editorInteractionLocked ? undefined : handleInsertToEditor
          }
        />
      </Suspense>
    </ErrorBoundary>
  );
}
