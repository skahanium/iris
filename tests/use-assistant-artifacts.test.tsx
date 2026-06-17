import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantArtifacts } from "@/components/ai/hooks/useAssistantArtifacts";
import type {
  CitationCheckResult,
  OrganizeSuggestion,
  PatchApplyResult,
  PatchProposal,
  ResearchFocusPayload,
} from "@/types/ai";

type HookApi = ReturnType<typeof useAssistantArtifacts>;
type PatchApply = (patch: PatchProposal) => Promise<PatchApplyResult>;
type OrganizeApply = (
  suggestions: OrganizeSuggestion[],
) => Promise<{ applied: string[]; skipped: string[]; errors: string[] }>;

const patchOne: PatchProposal = {
  id: "patch-1",
  target_path: "note.md",
  base_content_hash: "hash",
  range: { start: 6, end: 11 },
  original_text: "world",
  replacement_text: "Iris",
  evidence_packet_ids: [],
  risk_level: "low",
  warnings: [],
  created_at: "2026-06-11T00:00:00.000Z",
};

const patchTwo: PatchProposal = {
  ...patchOne,
  id: "patch-2",
  range: { start: 0, end: 5 },
  original_text: "hello",
  replacement_text: "hi",
};

const suggestionOne: OrganizeSuggestion = {
  id: "org-1",
  suggestion_type: "add_tag",
  target_path: "note.md",
  suggested_value: "ai",
  reason: "related topic",
  source: "assistant",
  confidence: 0.9,
  evidence_packet_ids: [],
};

const suggestionTwo: OrganizeSuggestion = {
  ...suggestionOne,
  id: "org-2",
  suggested_value: "research",
};

const citationResult: CitationCheckResult = {
  request_id: "citation-1",
  claims: [],
  coverage: "well_supported",
  suggestions: [],
  evidence_used: [],
  total_tokens: {
    prompt_tokens: 1,
    completion_tokens: 1,
    total_tokens: 2,
  },
};

const researchResult: ResearchFocusPayload = {
  request_id: "research-1",
  topic: "Iris",
  rounds: 1,
  summary: "summary",
  evidence_matrix: {
    total_evidence_count: 0,
    coverage_score: 0,
    global_gaps: [],
    propositions: [],
  },
  argument_chain: {
    has_contradictions: false,
    chain_strength: 0,
    links: [],
  },
  total_tokens: {
    prompt_tokens: 1,
    completion_tokens: 1,
    total_tokens: 2,
  },
};

function Harness({
  onReady,
  patchApply,
  organizeApply,
  onPatchApplied,
  onVaultRefresh,
}: {
  onReady: (api: HookApi) => void;
  patchApply: (patch: PatchProposal) => Promise<PatchApplyResult>;
  organizeApply: (
    suggestions: OrganizeSuggestion[],
  ) => Promise<{ applied: string[]; skipped: string[]; errors: string[] }>;
  onPatchApplied: (content: string) => void;
  onVaultRefresh: () => void;
}) {
  const api = useAssistantArtifacts({
    getNoteContent: () => "hello world",
    onPatchApplied,
    onVaultRefresh,
    deps: {
      patchApply,
      organizeApply,
    },
  });
  onReady(api);
  return null;
}

describe("useAssistantArtifacts", () => {
  let container: HTMLDivElement;
  let root: Root;
  let api!: HookApi;
  let patchApply: PatchApply;
  let organizeApply: OrganizeApply;
  let onPatchApplied: (content: string) => void;
  let onVaultRefresh: () => void;

  async function render() {
    await act(async () => {
      root.render(
        createElement(Harness, {
          onReady: (value) => {
            api = value;
          },
          patchApply,
          organizeApply,
          onPatchApplied,
          onVaultRefresh,
        }),
      );
    });
  }

  beforeEach(async () => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    patchApply = vi.fn(async () => ({
      success: true,
      warnings: [],
    }));
    organizeApply = vi.fn(async () => ({
      applied: ["org-1"],
      skipped: [],
      errors: [],
    }));
    onPatchApplied = vi.fn();
    onVaultRefresh = vi.fn();
    await render();
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("clears all task artifact state without touching the hook instance", async () => {
    await act(async () => {
      api.setWritingPatches([patchOne]);
      api.setCitationResult(citationResult);
      api.setOrganizeSuggestions([suggestionOne]);
      api.setOrganizeSelection(new Set(["org-1"]));
      api.setResearchResult(researchResult);
      api.setDocSummary("summary");
      api.setDocIssues(["issue"]);
      api.setLastError("boom");
    });

    await act(async () => {
      api.clearTaskSurfaces();
    });

    expect(api.writingPatches).toEqual([]);
    expect(api.citationResult).toBeNull();
    expect(api.organizeSuggestions).toEqual([]);
    expect(api.organizeSelection.size).toBe(0);
    expect(api.researchResult).toBeNull();
    expect(api.docSummary).toBeNull();
    expect(api.docIssues).toEqual([]);
    expect(api.lastError).toBeNull();
  });

  it("accepts and rejects writing patches through the artifact boundary", async () => {
    await act(async () => {
      api.setWritingPatches([patchOne, patchTwo]);
    });

    await act(async () => {
      await api.handleAcceptPatch(patchOne);
    });

    expect(patchApply).toHaveBeenCalledWith(patchOne);
    expect(onPatchApplied).toHaveBeenCalledWith("hello Iris");
    expect(api.writingPatches.map((patch) => patch.id)).toEqual(["patch-2"]);

    await act(async () => {
      api.handleRejectPatch(patchTwo);
    });

    expect(api.writingPatches).toEqual([]);
  });

  it("applies backend UTF-8 byte ranges without corrupting Chinese content", async () => {
    const chinesePatch: PatchProposal = {
      ...patchOne,
      id: "patch-chinese",
      range: {
        start: new TextEncoder().encode("甲").length,
        end: new TextEncoder().encode("甲乙").length,
      },
      original_text: "乙",
      replacement_text: "B",
    };

    function ChineseHarness({ onReady }: { onReady: (api: HookApi) => void }) {
      const api = useAssistantArtifacts({
        getNoteContent: () => "甲乙丙",
        onPatchApplied,
        deps: {
          patchApply,
          organizeApply,
        },
      });
      onReady(api);
      return null;
    }

    await act(async () => {
      root.render(
        createElement(ChineseHarness, {
          onReady: (value) => {
            api = value;
          },
        }),
      );
    });

    await act(async () => {
      api.setWritingPatches([chinesePatch]);
    });

    await act(async () => {
      await api.handleAcceptPatch(chinesePatch);
    });

    expect(onPatchApplied).toHaveBeenCalledWith("甲B丙");
    expect(api.writingPatches).toEqual([]);
  });

  it("applies only selected organize suggestions and refreshes the vault", async () => {
    await act(async () => {
      api.setOrganizeSuggestions([suggestionOne, suggestionTwo]);
      api.setOrganizeSelection(new Set(["org-1", "org-2"]));
      api.handleToggleOrganizeSuggestion("org-2");
    });

    await act(async () => {
      await api.handleAcceptOrganize();
    });

    expect(organizeApply).toHaveBeenCalledWith([suggestionOne]);
    expect(api.organizeSuggestions).toEqual([suggestionTwo]);
    expect(api.organizeSelection.size).toBe(0);
    expect(onVaultRefresh).toHaveBeenCalledTimes(1);
  });
});
