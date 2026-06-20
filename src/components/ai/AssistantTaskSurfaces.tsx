import type { RefObject } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { AgentTaskDto } from "@/types/ipc";
import type {
  CitationCheckResult,
  OrganizeSuggestion,
  PatchProposal,
  ResearchFocusPayload,
  WritingState,
} from "@/types/ai";

import {
  AssistantProcessStatusBar,
  type ResearchProgressData,
} from "./AssistantProcessStatusBar";
import { DocumentCheckArtifacts } from "./assistant/DocumentCheckArtifacts";
import { ResearchFocusView } from "./assistant/ResearchFocusView";
import { WritingStatePanel } from "./assistant/WritingStatePanel";
import { CitationCheckView } from "./CitationCheckView";
import { PatchPreview } from "./PatchPreview";

export type { ResearchProgressData } from "./AssistantProcessStatusBar";

interface AssistantTaskSurfacesProps {
  activityHint: string | null;
  agentTask: AgentTaskDto | null;
  researchProgress: ResearchProgressData | null;
  researchRunning: boolean;
  onAbortProcess: () => void;
  researchResult: ResearchFocusPayload | null;
  researchPanelExpanded: boolean;
  researchDetailRef: RefObject<HTMLDivElement | null>;
  generatingResearchNote: boolean;
  onGenerateResearchNote: () => void;
  docSummary: string | null;
  docIssues: string[];
  citationResult: CitationCheckResult | null;
  organizeSuggestions: OrganizeSuggestion[];
  organizeSelection: Set<string>;
  onClearOrganizeSelection: () => void;
  onToggleOrganizeSuggestion: (id: string) => void;
  onAcceptOrganize: () => void;
  evidenceRefreshNotice: string | null;
  writingPatches: PatchProposal[];
  writingState: WritingState | null;
  onAcceptPatch: (patch: PatchProposal) => void;
  onRejectPatch: (patch: PatchProposal) => void;
  onCopyPatch: (patch: PatchProposal) => void;
  onRegenerateWriting: () => void;
}

export function AssistantTaskSurfaces({
  activityHint,
  agentTask,
  researchProgress,
  researchRunning,
  onAbortProcess,
  researchResult,
  researchPanelExpanded,
  researchDetailRef,
  generatingResearchNote,
  onGenerateResearchNote,
  docSummary,
  docIssues,
  citationResult,
  organizeSuggestions,
  organizeSelection,
  onClearOrganizeSelection,
  onToggleOrganizeSuggestion,
  onAcceptOrganize,
  evidenceRefreshNotice,
  writingPatches,
  writingState,
  onAcceptPatch,
  onRejectPatch,
  onCopyPatch,
  onRegenerateWriting,
}: AssistantTaskSurfacesProps) {
  return (
    <>
      <AssistantProcessStatusBar
        activityHint={activityHint}
        agentTask={agentTask}
        researchProgress={researchProgress}
        researchRunning={researchRunning}
        onAbort={onAbortProcess}
      />

      {researchResult && researchPanelExpanded ? (
        <div
          ref={researchDetailRef}
          className="min-h-0 flex-1 overflow-y-auto px-3 pt-3"
          data-testid="research-detail-panel"
        >
          <ResearchFocusView
            result={researchResult}
            generatingNote={generatingResearchNote}
            onGenerateNote={onGenerateResearchNote}
          />
        </div>
      ) : null}

      {docSummary || docIssues.length > 0 ? (
        <div className="ai-task-surface px-3 pt-3">
          <DocumentCheckArtifacts summary={docSummary} issues={docIssues} />
        </div>
      ) : null}

      {citationResult ? (
        <div className="ai-task-surface px-3 pt-3">
          <CitationCheckView result={citationResult} />
        </div>
      ) : null}

      {organizeSuggestions.length > 0 ? (
        <div className="ai-task-surface px-3 pt-3">
          <Card className="border-border/60">
            <CardHeader className="pb-2">
              <div className="flex items-center justify-between gap-3">
                <CardTitle className="text-sm font-medium">整理建议</CardTitle>
                <div className="flex items-center gap-1.5">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={onClearOrganizeSelection}
                  >
                    清空选择
                  </Button>
                  <Button type="button" size="sm" onClick={onAcceptOrganize}>
                    应用已选
                  </Button>
                </div>
              </div>
            </CardHeader>
            <CardContent className="space-y-2">
              {organizeSuggestions.map((suggestion) => (
                <label
                  key={suggestion.id}
                  className="flex items-start gap-2 rounded-md border border-border/60 px-3 py-2 text-xs"
                >
                  <input
                    type="checkbox"
                    checked={organizeSelection.has(suggestion.id)}
                    onChange={() => onToggleOrganizeSuggestion(suggestion.id)}
                    className="mt-0.5 h-3.5 w-3.5"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="outline" className="text-[10px]">
                        {suggestion.suggestion_type}
                      </Badge>
                      <span className="truncate font-medium">
                        {suggestion.target_path}
                      </span>
                    </div>
                    <p className="mt-1 text-muted-foreground">
                      {suggestion.reason}
                    </p>
                    <p className="mt-1 text-foreground/80">
                      建议值：{suggestion.suggested_value}
                    </p>
                  </div>
                </label>
              ))}
            </CardContent>
          </Card>
        </div>
      ) : null}

      {evidenceRefreshNotice ? (
        <div className="px-3 pt-2 text-xs text-amber-700">
          {evidenceRefreshNotice}
        </div>
      ) : null}

      {writingPatches.length > 0 ? (
        <div
          className="ai-task-surface space-y-2 px-3 pt-3"
          data-testid="patch-preview"
        >
          {writingPatches.map((patch) => (
            <PatchPreview
              key={patch.id}
              patch={patch}
              onAccept={onAcceptPatch}
              onReject={onRejectPatch}
              onCopy={onCopyPatch}
              onRegenerate={onRegenerateWriting}
            />
          ))}
        </div>
      ) : null}

      <WritingStatePanel state={writingState} />
    </>
  );
}
