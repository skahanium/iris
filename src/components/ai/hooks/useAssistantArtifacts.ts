import {
  useCallback,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";

import { invokeErrorMessage } from "@/lib/credentials";
import {
  organizeApply as defaultOrganizeApply,
  patchApply as defaultPatchApply,
} from "@/lib/ipc";
import { utf8ByteRangeToStringRange } from "@/lib/utf8-range";
import type {
  CitationCheckResult,
  OrganizeSuggestion,
  PatchApplyResult,
  PatchProposal,
  ResearchFocusPayload,
  ResearchState,
  WritingState,
} from "@/types/ai";

interface UseAssistantArtifactsParams {
  getNoteContent: () => string;
  onPatchApplied?: (newContent: string) => void;
  onVaultRefresh?: () => void;
  deps?: {
    patchApply?: (patch: PatchProposal) => Promise<PatchApplyResult>;
    organizeApply?: (
      suggestions: OrganizeSuggestion[],
    ) => Promise<{ applied: string[]; skipped: string[]; errors: string[] }>;
  };
}

interface UseAssistantArtifactsResult {
  writingPatches: PatchProposal[];
  setWritingPatches: Dispatch<SetStateAction<PatchProposal[]>>;
  citationResult: CitationCheckResult | null;
  setCitationResult: Dispatch<SetStateAction<CitationCheckResult | null>>;
  organizeSuggestions: OrganizeSuggestion[];
  setOrganizeSuggestions: Dispatch<SetStateAction<OrganizeSuggestion[]>>;
  organizeSelection: Set<string>;
  setOrganizeSelection: Dispatch<SetStateAction<Set<string>>>;
  researchResult: ResearchFocusPayload | null;
  setResearchResult: Dispatch<SetStateAction<ResearchFocusPayload | null>>;
  researchState: ResearchState | null;
  setResearchState: Dispatch<SetStateAction<ResearchState | null>>;
  writingState: WritingState | null;
  setWritingState: Dispatch<SetStateAction<WritingState | null>>;
  docSummary: string | null;
  setDocSummary: Dispatch<SetStateAction<string | null>>;
  docIssues: string[];
  setDocIssues: Dispatch<SetStateAction<string[]>>;
  lastError: string | null;
  setLastError: Dispatch<SetStateAction<string | null>>;
  clearTaskSurfaces: () => void;
  handleAcceptPatch: (patch: PatchProposal) => Promise<void>;
  handleRejectPatch: (patch: PatchProposal) => void;
  handleCopyPatch: (patch: PatchProposal) => Promise<void>;
  handleClearOrganizeSelection: () => void;
  handleToggleOrganizeSuggestion: (id: string) => void;
  handleAcceptOrganize: () => Promise<void>;
}

export function useAssistantArtifacts({
  getNoteContent,
  onPatchApplied,
  onVaultRefresh,
  deps,
}: UseAssistantArtifactsParams): UseAssistantArtifactsResult {
  const [writingPatches, setWritingPatches] = useState<PatchProposal[]>([]);
  const [citationResult, setCitationResult] =
    useState<CitationCheckResult | null>(null);
  const [organizeSuggestions, setOrganizeSuggestions] = useState<
    OrganizeSuggestion[]
  >([]);
  const [organizeSelection, setOrganizeSelection] = useState<Set<string>>(
    new Set(),
  );
  const [researchResult, setResearchResult] =
    useState<ResearchFocusPayload | null>(null);
  const [researchState, setResearchState] = useState<ResearchState | null>(
    null,
  );
  const [writingState, setWritingState] = useState<WritingState | null>(null);
  const [docSummary, setDocSummary] = useState<string | null>(null);
  const [docIssues, setDocIssues] = useState<string[]>([]);
  const [lastError, setLastError] = useState<string | null>(null);

  const applyPatch = deps?.patchApply ?? defaultPatchApply;
  const applyOrganize = deps?.organizeApply ?? defaultOrganizeApply;

  const clearTaskSurfaces = useCallback(() => {
    setWritingPatches([]);
    setCitationResult(null);
    setOrganizeSuggestions([]);
    setOrganizeSelection(new Set());
    setResearchResult(null);
    setResearchState(null);
    setWritingState(null);
    setDocSummary(null);
    setDocIssues([]);
    setLastError(null);
  }, []);

  const handleAcceptPatch = useCallback(
    async (patch: PatchProposal) => {
      try {
        const result = await applyPatch(patch);
        if (!result.success) {
          throw new Error(result.error ?? "补丁应用失败");
        }
        const noteContent = getNoteContent();
        const stringRange = utf8ByteRangeToStringRange(
          noteContent,
          patch.range,
        );
        if (!stringRange) {
          throw new Error("补丁范围不是有效的 UTF-8 字节边界");
        }
        const before = noteContent.slice(0, stringRange.start);
        const after = noteContent.slice(stringRange.end);
        onPatchApplied?.(before + patch.replacement_text + after);
        setWritingPatches((prev) =>
          prev.filter((item) => item.id !== patch.id),
        );
      } catch (error) {
        setLastError(invokeErrorMessage(error));
      }
    },
    [applyPatch, getNoteContent, onPatchApplied],
  );

  const handleRejectPatch = useCallback((patch: PatchProposal) => {
    setWritingPatches((prev) => prev.filter((item) => item.id !== patch.id));
  }, []);

  const handleCopyPatch = useCallback(async (patch: PatchProposal) => {
    try {
      await navigator.clipboard.writeText(patch.replacement_text);
    } catch {
      /* ignore clipboard failures */
    }
  }, []);

  const handleClearOrganizeSelection = useCallback(() => {
    setOrganizeSelection(new Set());
  }, []);

  const handleToggleOrganizeSuggestion = useCallback((id: string) => {
    setOrganizeSelection((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleAcceptOrganize = useCallback(async () => {
    const selected = organizeSuggestions.filter((item) =>
      organizeSelection.has(item.id),
    );
    if (selected.length === 0) return;
    try {
      const result = await applyOrganize(selected);
      setOrganizeSuggestions((prev) =>
        prev.filter((item) => !result.applied.includes(item.id)),
      );
      setOrganizeSelection(new Set());
      onVaultRefresh?.();
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    }
  }, [applyOrganize, onVaultRefresh, organizeSelection, organizeSuggestions]);

  return {
    writingPatches,
    setWritingPatches,
    citationResult,
    setCitationResult,
    organizeSuggestions,
    setOrganizeSuggestions,
    organizeSelection,
    setOrganizeSelection,
    researchResult,
    setResearchResult,
    researchState,
    setResearchState,
    writingState,
    setWritingState,
    docSummary,
    setDocSummary,
    docIssues,
    setDocIssues,
    lastError,
    setLastError,
    clearTaskSurfaces,
    handleAcceptPatch,
    handleRejectPatch,
    handleCopyPatch,
    handleClearOrganizeSelection,
    handleToggleOrganizeSuggestion,
    handleAcceptOrganize,
  };
}
