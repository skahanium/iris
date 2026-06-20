import type { SceneMeta } from "@/types/ai";

export type ActiveAiScene =
  | "knowledge_lookup"
  | "drafting_assist"
  | "research_synthesis";

const ACTIVE_AI_SCENES: ActiveAiScene[] = [
  "knowledge_lookup",
  "drafting_assist",
  "research_synthesis",
];

export const SCENE_META: Record<ActiveAiScene, SceneMeta> = {
  knowledge_lookup: {
    scene: "knowledge_lookup",
    label: "知识查阅",
    description: "查询法规、笔记关联",
    icon: "Search",
    defaultScope: "global",
  },
  drafting_assist: {
    scene: "drafting_assist",
    label: "文稿创作",
    description: "辅助公文写作",
    icon: "PenLine",
    defaultScope: "document",
  },
  research_synthesis: {
    scene: "research_synthesis",
    label: "学术研究",
    description: "多材料论证组织",
    icon: "FlaskConical",
    defaultScope: "global",
  },
};

export const SCENE_OPTIONS: SceneMeta[] = ACTIVE_AI_SCENES.map(
  (scene) => SCENE_META[scene],
);
