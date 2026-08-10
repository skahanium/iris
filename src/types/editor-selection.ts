import type { ContextReference } from "./ai";

export type EditorSelectionCandidateStatus =
  | "validating"
  | "ready"
  | "save_required"
  | "invalid";

/** Local-only projection of the current editor selection for the Agent UI. */
export interface EditorSelectionCandidate {
  key: string;
  preview: string;
  status: EditorSelectionCandidateStatus;
  reference: ContextReference | null;
  message: string | null;
}
