import { setActiveAiScene } from "@/hooks/useConnectivityStatus";
import type { AgentIntent, AiScene, AssistantIntent } from "@/types/ai";

type LegacySceneHint = Exclude<AiScene, "exemplar_learning">;

export function legacySceneHintForAgentIntent(
  intent: AgentIntent,
): LegacySceneHint {
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

export function legacySceneHintForAssistantIntent(
  intent: AssistantIntent,
): LegacySceneHint {
  switch (intent) {
    case "writing":
    case "citation":
    case "chapter":
    case "document":
      return "drafting_assist";
    case "research":
      return "research_synthesis";
    case "organize":
    case "knowledge":
    case "chat":
    default:
      return "knowledge_lookup";
  }
}

export function syncActiveLegacySceneHint(
  intent: AssistantIntent,
): LegacySceneHint {
  const hint = legacySceneHintForAssistantIntent(intent);
  setActiveAiScene(hint);
  return hint;
}
