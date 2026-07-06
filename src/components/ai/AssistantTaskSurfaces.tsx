import type {
  CitationCheckResult,
  OrganizeSuggestion,
  PatchProposal,
  WritingState,
} from "@/types/ai";
import { artifactPassesValueGate } from "@/lib/assistant-artifact-tabs";
import {
  getAiPayloadStore,
  sanitizePayloadForUi,
} from "@/lib/ai-payload-store";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";

import { AssistantArtifactTagStrip } from "./AssistantArtifactTagStrip";

export type { ResearchProgressData } from "./AssistantProcessStatusBar";

interface AssistantTaskSurfacesProps {
  assistantArtifacts: AssistantArtifactDraft[];
  docSummary: string | null;
  docIssues: string[];
  citationResult: CitationCheckResult | null;
  organizeSuggestions: OrganizeSuggestion[];
  organizeSelection: Set<string>;
  evidenceRefreshNotice: string | null;
  writingPatches: PatchProposal[];
  writingState: WritingState | null;
  onOpenArtifact: (draft: AssistantArtifactDraft) => void;
}

export function AssistantTaskSurfaces({
  assistantArtifacts,
  docSummary,
  docIssues,
  citationResult,
  organizeSuggestions,
  organizeSelection,
  evidenceRefreshNotice,
  writingPatches,
  writingState,
  onOpenArtifact,
}: AssistantTaskSurfacesProps) {
  const artifacts: AssistantArtifactDraft[] = [];
  const pushArtifact = (draft: AssistantArtifactDraft) => {
    const safeDraft = {
      ...draft,
      payload: sanitizePayloadForUi(getAiPayloadStore(), draft.payload),
    };
    if (artifactPassesValueGate(safeDraft)) artifacts.push(safeDraft);
  };

  if (assistantArtifacts.length > 0) {
    return (
      <>
        <AssistantArtifactTagStrip
          artifacts={assistantArtifacts}
          onOpenArtifact={onOpenArtifact}
        />

        {evidenceRefreshNotice ? (
          <div className="px-3 pt-2 text-xs text-amber-700">
            {evidenceRefreshNotice}
          </div>
        ) : null}
      </>
    );
  }

  if (citationResult) {
    pushArtifact({
      kind: "structured_result",
      title: "引用检查",
      sourceRequestId: citationResult.request_id,
      payload: { schema: "citation_report", result: citationResult },
    });
  }
  if (organizeSuggestions.length > 0) {
    pushArtifact({
      kind: "structured_result",
      title: "鏁寸悊寤鸿",
      sourceRequestId: organizeSuggestions.map((item) => item.id).join("-"),
      payload: {
        schema: "organize_result",
        suggestions: organizeSuggestions,
        selectedIds: Array.from(organizeSelection),
      },
    });
  }
  if (writingPatches.length > 0) {
    pushArtifact({
      kind: "writing_change",
      title: "写作修改",
      sourceRequestId: writingPatches.map((item) => item.id).join("-"),
      payload: {
        schema: "writing_change",
        patches: writingPatches,
        writingState,
      },
    });
  }
  if (docSummary || docIssues.length > 0) {
    pushArtifact({
      kind: "structured_result",
      title: "文档检查",
      sourceRequestId: "document-check",
      payload: {
        resultKind: "document_issues",
        summary: docSummary,
        issues: docIssues,
      },
    });
  }

  return (
    <>
      <AssistantArtifactTagStrip
        artifacts={artifacts}
        onOpenArtifact={onOpenArtifact}
      />

      {evidenceRefreshNotice ? (
        <div className="px-3 pt-2 text-xs text-amber-700">
          {evidenceRefreshNotice}
        </div>
      ) : null}
    </>
  );
}
