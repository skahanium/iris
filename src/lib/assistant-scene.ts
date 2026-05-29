import { setActiveAiScene } from "@/hooks/useConnectivityStatus";
import type { AiScene, AssistantIntent } from "@/types/ai";

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
