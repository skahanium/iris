export type ArtifactKind =
  | "evidence_sources"
  | "writing_change"
  | "structured_result"
  | "task_process"
  | "session_evidence_detail";

export interface AssistantArtifactDraft {
  kind: ArtifactKind;
  title: string;
  sourceRequestId: string;
  payload: unknown;
  persistent?: boolean;
}

export interface ArtifactTab extends AssistantArtifactDraft {
  id: string;
  createdAt: string;
  readonly: true;
}
