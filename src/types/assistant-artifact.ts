export type ArtifactKind =
  | "evidence_sources"
  | "writing_change"
  | "structured_result"
  | "task_process";

export interface AssistantArtifactDraft {
  kind: ArtifactKind;
  title: string;
  sourceRequestId: string;
  payload: unknown;
}

export interface ArtifactTab extends AssistantArtifactDraft {
  id: string;
  createdAt: string;
  readonly: true;
}
