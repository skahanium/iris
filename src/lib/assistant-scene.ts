import { setActiveAiScene } from "@/hooks/useConnectivityStatus";
import type { AgentIntent, AiScene, AssistantIntent } from "@/types/ai";

/** 由 Phase2 AgentIntent 推导后端旧场景策略（仅内部兼容层）。 */
export function resolveAiSceneForAgentIntent(intent: AgentIntent): AiScene {
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

/** 由助手意图推导后端场景策略（用户不可手动切换） */
export function resolveAiSceneForIntent(intent: AssistantIntent): AiScene {
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

export function syncActiveAiScene(intent: AssistantIntent): AiScene {
  const scene = resolveAiSceneForIntent(intent);
  setActiveAiScene(scene);
  return scene;
}
