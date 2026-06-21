import type {
  AiChatExecutePayload,
  AssistantIntent,
  CitationCheckResult,
  ContextPacket,
  DocumentCheckResult,
  OrganizeSuggestion,
  PatchProposal,
  WritingTaskResult,
} from "@/types/ai";
import type { ArtifactKind } from "@/types/assistant-artifact";

export interface UnifiedArtifact {
  id: string;
  kind: ArtifactKind;
  title: string;
  status: "ready" | "pending" | "error";
  sourceTask: AssistantIntent;
  evidenceCount: number;
  payload: unknown;
}

export function mapChatResultToArtifacts(
  payload: AiChatExecutePayload,
): UnifiedArtifact[] {
  const artifacts: UnifiedArtifact[] = [];
  if (payload.status === "pending_tools") {
    artifacts.push({
      id: `confirm-${payload.request_id}`,
      kind: "task_process",
      title: "工具确认",
      status: "pending",
      sourceTask: "chat",
      evidenceCount: 0,
      payload: {
        schema: "task_process",
        status: "pending_confirmation",
        tool_calls: payload.tool_calls,
        tool_results: payload.tool_results,
      },
    });
  }
  return artifacts;
}

export function mapWritingToArtifacts(
  output: WritingTaskResult,
): UnifiedArtifact[] {
  return [
    {
      id: `patches-${output.request_id}`,
      kind: "writing_change",
      title: "写作修改",
      status: "ready",
      sourceTask: "writing",
      evidenceCount: output.evidence_used?.length ?? 0,
      payload: {
        schema: "writing_change",
        patches: output.patches,
      },
    },
  ];
}

export function mapCitationToArtifacts(
  result: CitationCheckResult,
): UnifiedArtifact[] {
  return [
    {
      id: `citation-${result.request_id}`,
      kind: "structured_result",
      title: "引用检查",
      status: "ready",
      sourceTask: "citation",
      evidenceCount: 0,
      payload: {
        resultKind: "citation_check",
        result,
      },
    },
  ];
}

export function mapOrganizeToArtifacts(
  suggestions: OrganizeSuggestion[],
): UnifiedArtifact[] {
  return [
    {
      id: "organize",
      kind: "structured_result",
      title: "整理建议",
      status: "ready",
      sourceTask: "organize",
      evidenceCount: 0,
      payload: {
        resultKind: "organize_suggestions",
        suggestions,
      },
    },
  ];
}

export function mapDocumentToArtifacts(
  result: DocumentCheckResult,
  patches: PatchProposal[],
): UnifiedArtifact[] {
  const items: UnifiedArtifact[] = [
    {
      id: `doc-${result.request_id}`,
      kind: "structured_result",
      title: "文档检查",
      status: "ready",
      sourceTask: "document",
      evidenceCount: 0,
      payload: {
        resultKind: "document_issues",
        result,
      },
    },
  ];
  if (patches.length > 0) {
    items.push({
      id: `doc-patches-${result.request_id}`,
      kind: "writing_change",
      title: "写作修改",
      status: "ready",
      sourceTask: "document",
      evidenceCount: 0,
      payload: {
        schema: "writing_change",
        patches,
      },
    });
  }
  return items;
}

export function mergeEvidencePackets(
  prev: ContextPacket[],
  incoming?: ContextPacket[],
): ContextPacket[] {
  if (!incoming?.length) return prev;
  const byId = new Map(prev.map((p) => [p.id, p]));
  for (const p of incoming) {
    byId.set(p.id, p);
  }
  return Array.from(byId.values());
}
