import { lazy, Suspense, useMemo } from "react";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import type { AiDomain } from "@/types/ai";
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
  editorInteractionLocked?: boolean;
  runtimeDocumentCandidates?: FileListItem[];
  handleInsertToEditor: (content: string) => void;
  webSearch: boolean;
  webSearchProviderName?: string | null;
}

/** Lazily loads the Run-only side panel without passing implicit document state. */
export function AppAiPanelSlot({
  aiDomain,
  classifiedPath,
  editorInteractionLocked = false,
  runtimeDocumentCandidates = [],
  handleInsertToEditor,
  webSearch,
  webSearchProviderName = null,
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
          runtimeDocumentCandidates={mentionRuntimeCandidates}
          webSearch={webSearch}
          webSearchProviderName={webSearchProviderName}
          onInsertToEditor={
            editorInteractionLocked ? undefined : handleInsertToEditor
          }
        />
      </Suspense>
    </ErrorBoundary>
  );
}
