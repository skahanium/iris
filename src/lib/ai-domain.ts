import type { AiDomain } from "@/types/ai";

export type { AiDomain };

export interface AiDomainState {
  domain: AiDomain;
  normalActivePath: string | null;
  classifiedActivePath: string | null;
  classifiedUnlocked: boolean;
}

export function deriveAiDomainState(input: {
  activePath: string | null;
  activeNoteIsClassified: boolean;
  classifiedUnlocked: boolean;
  activeArtifactTab: unknown | null;
  activeMediaTab: unknown | null;
}): AiDomainState {
  const canUseClassified =
    input.activeNoteIsClassified &&
    input.classifiedUnlocked &&
    !input.activeArtifactTab &&
    !input.activeMediaTab &&
    input.activePath !== null;

  return {
    domain: canUseClassified ? "classified" : "normal",
    normalActivePath:
      !input.activeNoteIsClassified &&
      !input.activeArtifactTab &&
      !input.activeMediaTab
        ? input.activePath
        : null,
    classifiedActivePath: canUseClassified ? input.activePath : null,
    classifiedUnlocked: input.classifiedUnlocked,
  };
}

export function shouldAttachNormalCurrentDocument(input: {
  explicitContext: boolean;
  uiAction: "chat" | "editor_action" | "selection_quote" | "mention";
}): boolean {
  return input.explicitContext || input.uiAction !== "chat";
}
