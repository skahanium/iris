import type { AiScene, SceneMeta } from "@/types/ai";

export type ActiveAiScene = Exclude<AiScene, "exemplar_learning">;

const ACTIVE_AI_SCENES: ActiveAiScene[] = [
  "knowledge_lookup",
  "drafting_assist",
  "research_synthesis",
];

export const SCENE_META: Record<AiScene, SceneMeta> = {
  knowledge_lookup: {
    scene: "knowledge_lookup",
    label: "知识查阅",
    description: "查询法规、笔记关联",
    icon: "Search",
    defaultScope: "global",
  },
  exemplar_learning: {
    scene: "exemplar_learning",
    label: "文稿学习",
    description: "分析范文结构与表达",
    icon: "BookOpen",
    defaultScope: "document",
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
