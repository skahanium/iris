import {
  mapChatResultToArtifacts,
  mapCitationToArtifacts,
  mapDocumentToArtifacts,
  mapOrganizeToArtifacts,
  mapWritingToArtifacts,
  type UnifiedArtifact,
} from "@/lib/map-harness-result-to-artifacts";
import type {
  AssistantExecuteResponse,
  CitationCheckResult,
  DocumentCheckResult,
  OrganizeTaskResult,
  WritingTaskResult,
} from "@/types/ai";
import type { ArtifactKind } from "@/types/assistant-artifact";

function isArtifactKind(kind: string): kind is ArtifactKind {
  return (
    kind === "evidence_sources" ||
    kind === "writing_change" ||
    kind === "structured_result" ||
    kind === "task_process"
  );
}

function wireToUnified(
  wires: AssistantExecuteResponse["artifacts"],
): UnifiedArtifact[] {
  return wires.flatMap((w, index) => {
    if (!isArtifactKind(w.kind)) return [];
    return [
      {
        id: `${w.kind}-${w.sourceTask}-${index}`,
        kind: w.kind,
        title: w.title,
        status:
          w.status === "pending" || w.status === "pending_confirmation"
            ? "pending"
            : w.status === "error"
              ? "error"
              : "ready",
        sourceTask: w.sourceTask as UnifiedArtifact["sourceTask"],
        evidenceCount: w.evidenceCount,
        payload: w.payload,
      },
    ];
  });
}

/** Map IPC response to unified artifacts (server wires + typed fallbacks). */
export function mapAssistantExecuteToArtifacts(
  response: AssistantExecuteResponse,
): UnifiedArtifact[] {
  if (response.artifacts?.length) {
    return wireToUnified(response.artifacts);
  }
  switch (response.kind) {
    case "chat":
      return mapChatResultToArtifacts(response.payload);
    case "writing":
      return mapWritingToArtifacts(response.payload as WritingTaskResult);
    case "citation":
      return mapCitationToArtifacts(response.payload as CitationCheckResult);
    case "organize":
      return mapOrganizeToArtifacts(
        (response.payload as OrganizeTaskResult).batch?.suggestions ?? [],
      );
    case "document": {
      const doc = response.payload as DocumentCheckResult;
      return mapDocumentToArtifacts(doc, doc.patches ?? []);
    }
    default:
      return [];
  }
}
