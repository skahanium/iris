import { lazy, Suspense, useMemo } from "react";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { useWorkspaceChromeActions } from "@/hooks/useWorkspaceChromeActions";
import type { AiDomain, ContextReference } from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { FileListItem } from "@/types/ipc";

const loadAssistantPanel = () =>
  import("@/components/ai/UnifiedAssistantPanel").then((m) => ({
    default: m.UnifiedAssistantPanel,
  }));

const UnifiedAssistantPanel = lazy(() => loadAssistantPanel());

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

  // 布局动作经 AppShell 的 WorkspaceChromeActions 通道显式下发，面板不自行切换 presentation。
  const chromeActions = useWorkspaceChromeActions();
  const assistantFocus =
    chromeActions.projection.primarySurface === "assistant_focus";

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
          onOpenWebVerificationSettings={onOpenWebVerificationSettings}
          onChromeChange={onChromeChange}
          onInsertToEditor={
            editorInteractionLocked ? undefined : handleInsertToEditor
          }
          assistantFocus={assistantFocus}
          onRequestFocusEnter={chromeActions.enterAssistantFocus}
          onRequestFocusExit={chromeActions.exitAssistantFocus}
        />
      </Suspense>
    </ErrorBoundary>
  );
}
