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

export type ArtifactKind =
  | "message"
  | "patches"
  | "citation_report"
  | "organize_report"
  | "research_report"
  | "document_check"
  | "chapter_writing"
  | "tool_confirmation";

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
  if (payload.content) {
    artifacts.push({
      id: `msg-${payload.request_id}`,
      kind: "message",
      title: "回答",
      status: payload.status === "pending_tools" ? "pending" : "ready",
      sourceTask: "chat",
      evidenceCount: payload.evidence_packets?.length ?? 0,
      payload: {
        content: payload.content,
        citation_valid: payload.citation_valid,
      },
    });
  }
  if (payload.status === "pending_tools") {
    artifacts.push({
      id: `confirm-${payload.request_id}`,
      kind: "tool_confirmation",
      title: "工具确认",
      status: "pending",
      sourceTask: "chat",
      evidenceCount: 0,
      payload: {
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
      kind: "patches",
      title: "写作补丁",
      status: "ready",
      sourceTask: "writing",
      evidenceCount: output.evidence_used?.length ?? 0,
      payload: output.patches,
    },
  ];
}

export function mapCitationToArtifacts(
  result: CitationCheckResult,
): UnifiedArtifact[] {
  return [
    {
      id: `citation-${result.request_id}`,
      kind: "citation_report",
      title: "引用检查",
      status: "ready",
      sourceTask: "citation",
      evidenceCount: 0,
      payload: result,
    },
  ];
}

export function mapOrganizeToArtifacts(
  suggestions: OrganizeSuggestion[],
): UnifiedArtifact[] {
  return [
    {
      id: "organize",
      kind: "organize_report",
      title: "整理建议",
      status: "ready",
      sourceTask: "organize",
      evidenceCount: 0,
      payload: suggestions,
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
      kind: "document_check",
      title: "文档检查",
      status: "ready",
      sourceTask: "document",
      evidenceCount: 0,
      payload: result,
    },
  ];
  if (patches.length > 0) {
    items.push({
      id: `doc-patches-${result.request_id}`,
      kind: "patches",
      title: "修订补丁",
      status: "ready",
      sourceTask: "document",
      evidenceCount: 0,
      payload: patches,
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
