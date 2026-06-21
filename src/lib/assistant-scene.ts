import type { AgentIntent, TaskPlanIntent } from "@/types/ai";

type LegacySceneHint =
  | "knowledge_lookup"
  | "drafting_assist"
  | "research_synthesis";

export function legacySceneHintForAgentIntent(
  intent: AgentIntent,
): LegacySceneHint {
  // compatibility only: backend context assembly still accepts LegacyAiScene.
  switch (intent) {
    case "rewrite_selection":
    case "write":
    case "chapter":
    case "document_check":
      return "drafting_assist";
    case "citation_check":
    case "research":
      return "research_synthesis";
    case "ask_notes":
    case "organize":
    case "vision_chat":
    case "skill_management":
    case "chat":
    default:
      return "knowledge_lookup";
  }
}

export function legacySceneHintForTaskPlanIntent(
  intent: TaskPlanIntent | null | undefined,
): LegacySceneHint {
  // compatibility only: session history is still bucketed by legacy scene.
  switch (intent) {
    case "creative_write":
      return legacySceneHintForAgentIntent("write");
    case "rewrite_selection":
      return legacySceneHintForAgentIntent("rewrite_selection");
    case "chapter":
      return legacySceneHintForAgentIntent("chapter");
    case "document_check":
      return legacySceneHintForAgentIntent("document_check");
    case "citation_check":
      return legacySceneHintForAgentIntent("citation_check");
    case "research":
      return legacySceneHintForAgentIntent("research");
    case "ask_notes":
      return legacySceneHintForAgentIntent("ask_notes");
    case "organize":
      return legacySceneHintForAgentIntent("organize");
    case "vision_chat":
      return legacySceneHintForAgentIntent("vision_chat");
    case "skill_management":
      return legacySceneHintForAgentIntent("skill_management");
    case "chat":
    default:
      return legacySceneHintForAgentIntent("chat");
  }
}
