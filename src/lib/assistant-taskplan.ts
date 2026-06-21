import type {
  ArtifactPlanItem,
  ContextReference,
  TaskPlan,
  TaskPlanIntent,
} from "@/types/ai";

import type { AssistantRouteInput } from "./assistant-routing";

export const writingKeywordBeforeResearchKeyword = true;

export interface BuildAssistantTaskPlanInput extends AssistantRouteInput {
  contextReferences?: ContextReference[];
  webAuthorized?: boolean;
}

const WRITING_KEYWORDS = [
  "改写",
  "重写",
  "润色",
  "扩写",
  "续写",
  "补写",
  "写出",
  "写一段",
  "写成",
  "创作",
  "描写",
  "剧情",
  "人物心理",
  "总结这段",
  "简化",
  "压缩",
  "重组",
  "小说",
];

const REWRITE_KEYWORDS = [
  "改写",
  "重写",
  "润色",
  "简化",
  "压缩",
  "总结这段",
  "重组",
  "翻译",
];

const CREATIVE_KEYWORDS = [
  "续写",
  "扩写",
  "补写",
  "写出",
  "写一段",
  "创作",
  "描写",
  "剧情",
  "人物心理",
  "小说",
  "更火爆",
  "更诱人",
];

const CITATION_KEYWORDS = [
  "引用",
  "引证",
  "依据",
  "证据",
  "出处",
  "核查",
  "检查",
];

const ORGANIZE_KEYWORDS = [
  "整理",
  "归档",
  "标签",
  "分类",
  "标题",
  "资料库",
  "知识库",
];

const RESEARCH_KEYWORDS = [
  "研究",
  "调研",
  "查资料",
  "找证据",
  "多来源",
  "对比来源",
  "联网",
  "综述",
  "文献",
  "论文",
  "深挖",
  "取舍",
];

const KNOWLEDGE_KEYWORDS = [
  "查一下",
  "查阅",
  "搜索",
  "搜一下",
  "库里",
  "文档里",
  "找一下",
  "什么是",
];

const CHAPTER_KEYWORDS = ["章节", "这一章", "本章", "章内", "heading"];

const DOCUMENT_KEYWORDS = [
  "大纲检查",
  "全文检查",
  "文档检查",
  "风格一致",
  "跨文档",
  "引用缺口",
  "outline",
];

const AMBIGUOUS_HIGH_COST_KEYWORDS = [
  "处理一下",
  "搞一下",
  "完整方案",
  "全面处理",
  "自动完成",
];

function includesAny(haystack: string, needles: string[]): boolean {
  return needles.some((needle) => haystack.includes(needle));
}

function hasChapterCreationIntent(message: string): boolean {
  return /第[一二三四五六七八九十\d]+章/.test(message);
}

function contextReferencesFor(
  input: BuildAssistantTaskPlanInput,
): ContextReference[] {
  return input.contextReferences ?? [];
}

function sourceHintsFor(input: BuildAssistantTaskPlanInput): string[] {
  const hints: string[] = [];

  if (input.uiAction) hints.push(`ui_action:${input.uiAction}`);
  if (input.hasSelection) hints.push("context:selection");
  if (input.notePath) hints.push("context:note");
  if (input.explicitScope) hints.push("context:scope");
  if (input.hasImage) hints.push("context:image");
  if (input.skillMention) hints.push("skill:mention");
  if (contextReferencesFor(input).length > 0) hints.push("context:reference");

  const skillHubSkill = skillHubDirectInstallSkill(input.message);
  if (skillHubSkill) {
    hints.push(`skillhub:direct_install:${skillHubSkill}`);
  }

  return hints;
}

function skillHubDirectInstallSkill(message: string): string | null {
  const raw = message.trim();
  const lower = raw.toLowerCase();
  const hasSkillHubSource =
    lower.includes("skillhub") ||
    lower.includes("skillhub.cn/install/skillhub.md") ||
    lower.includes("skillhub 商店") ||
    lower.includes("skillhub商店");
  if (!hasSkillHubSource) return null;

  const installSkillPattern =
    /(?:安装|install)\s*([a-z0-9][a-z0-9_-]{1,80})\s*(?:技能|skill)/i;
  const match = raw.match(installSkillPattern);
  const skill = match?.[1]?.toLowerCase();
  if (!skill || skill === "skillhub") return null;

  return skill;
}

function basePlan(input: BuildAssistantTaskPlanInput): Pick<
  TaskPlan,
  | "contextReferences"
  | "retrievalMode"
  | "webMode"
  | "artifactPlan"
  | "requiresClarification"
  | "sourceHints"
> {
  const contextReferences = contextReferencesFor(input);

  return {
    contextReferences,
    retrievalMode:
      input.hasSelection || contextReferences.length > 0
        ? "current_reference"
        : "none",
    webMode: input.webAuthorized ? "brokered" : "disabled",
    artifactPlan: [],
    requiresClarification: false,
    sourceHints: sourceHintsFor(input),
  };
}

function plan(
  input: BuildAssistantTaskPlanInput,
  values: Omit<
    TaskPlan,
    | "contextReferences"
    | "retrievalMode"
    | "webMode"
    | "artifactPlan"
    | "requiresClarification"
    | "sourceHints"
  > &
    Partial<
      Pick<
        TaskPlan,
        | "retrievalMode"
        | "artifactPlan"
        | "requiresClarification"
        | "clarificationQuestion"
      >
    >,
): TaskPlan {
  return {
    ...basePlan(input),
    ...values,
    clarificationQuestion: values.clarificationQuestion ?? null,
  };
}

function writerPlan(
  input: BuildAssistantTaskPlanInput,
  intent: "creative_write" | "rewrite_selection",
): TaskPlan {
  const executionMode =
    intent === "rewrite_selection" ? "patch_proposal" : "writing_candidate";

  return plan(input, {
    intent,
    confidence: "high",
    retrievalMode: "current_reference",
    modelSlot: "writer",
    executionMode,
    outputMode:
      intent === "rewrite_selection"
        ? "confirmation_required"
        : "markdown_message",
    artifactPlan: [],
  });
}

function researchPlan(input: BuildAssistantTaskPlanInput): TaskPlan {
  const artifactPlan: ArtifactPlanItem[] = [
    {
      kind: "evidence_sources",
      reason: "Research tasks need inspectable sources outside the chat stream.",
      valueGate: "non_empty_evidence_sources",
    },
  ];

  return plan(input, {
    intent: "research",
    confidence: "high",
    retrievalMode: input.explicitScope ? "scoped_notes" : "local_notes",
    modelSlot: "reasoner",
    executionMode: "structured_task",
    outputMode: "artifact_backed_message",
    artifactPlan,
  });
}

function clarifyPlan(
  input: BuildAssistantTaskPlanInput,
  question: string,
): TaskPlan {
  return plan(input, {
    intent: "chat",
    confidence: "low",
    modelSlot: "fast",
    executionMode: "clarification",
    outputMode: "markdown_message",
    artifactPlan: [],
    requiresClarification: true,
    clarificationQuestion: question,
  });
}

function chatPlan(input: BuildAssistantTaskPlanInput): TaskPlan {
  return plan(input, {
    intent: "chat",
    confidence: "low",
    retrievalMode: "none",
    modelSlot: "fast",
    executionMode: "direct_answer",
    outputMode: "markdown_message",
    artifactPlan: [],
  });
}

function askNotesPlan(input: BuildAssistantTaskPlanInput): TaskPlan {
  const hasCurrentReference =
    input.hasSelection || contextReferencesFor(input).length > 0;

  return plan(input, {
    intent: "ask_notes",
    confidence: "medium",
    retrievalMode: hasCurrentReference ? "current_reference" : "local_notes",
    modelSlot: "fast",
    executionMode: "context_answer",
    outputMode: "markdown_message",
    artifactPlan: [],
  });
}

function simpleTaskPlan(
  input: BuildAssistantTaskPlanInput,
  intent: Exclude<
    TaskPlanIntent,
    | "chat"
    | "ask_notes"
    | "creative_write"
    | "rewrite_selection"
    | "research"
  >,
): TaskPlan {
  switch (intent) {
    case "citation_check":
      return plan(input, {
        intent,
        confidence: "high",
        retrievalMode: "current_reference",
        modelSlot: "reasoner",
        executionMode: "structured_task",
        outputMode: "markdown_message",
        artifactPlan: [],
      });
    case "organize":
      return plan(input, {
        intent,
        confidence: "medium",
        retrievalMode: "scoped_notes",
        modelSlot: "reasoner",
        executionMode: "structured_task",
        outputMode: "markdown_message",
        artifactPlan: [],
      });
    case "document_check":
      return plan(input, {
        intent,
        confidence: "high",
        retrievalMode: "long_document",
        modelSlot: "long_context",
        executionMode: "long_task",
        outputMode: "markdown_message",
        artifactPlan: [],
      });
    case "chapter":
      return plan(input, {
        intent,
        confidence: "medium",
        retrievalMode: "current_reference",
        modelSlot: "writer",
        executionMode: "writing_candidate",
        outputMode: "markdown_message",
        artifactPlan: [],
      });
    case "vision_chat":
      return plan(input, {
        intent,
        confidence: "high",
        retrievalMode: "none",
        modelSlot: "vision",
        executionMode: "direct_answer",
        outputMode: "markdown_message",
        artifactPlan: [],
      });
    case "skill_management":
      return plan(input, {
        intent,
        confidence: input.skillMention ? "high" : "medium",
        retrievalMode: "none",
        modelSlot: "agent_tools",
        executionMode: "structured_task",
        outputMode: "diagnostic",
        artifactPlan: [],
      });
  }
}

function planForUiAction(input: BuildAssistantTaskPlanInput): TaskPlan | null {
  const action = input.uiAction?.toLowerCase();
  if (!action) return null;

  if (["rewrite", "summarize", "translate", "simplify"].includes(action)) {
    return writerPlan(input, "rewrite_selection");
  }
  if (["citation", "citation_check"].includes(action)) {
    return simpleTaskPlan(input, "citation_check");
  }
  if (["chapter", "chapter_write"].includes(action)) {
    return simpleTaskPlan(input, "chapter");
  }
  if (["document", "document_check"].includes(action)) {
    return simpleTaskPlan(input, "document_check");
  }
  if (["organize", "organise"].includes(action)) {
    return simpleTaskPlan(input, "organize");
  }
  if (["research"].includes(action)) {
    return researchPlan(input);
  }
  if (["ask_notes", "knowledge"].includes(action)) {
    return askNotesPlan(input);
  }

  return null;
}

export function buildAssistantTaskPlan(
  input: BuildAssistantTaskPlanInput,
): TaskPlan {
  const message = input.message.trim().toLowerCase();
  const uiPlan = planForUiAction(input);
  if (uiPlan) return uiPlan;

  if (input.hasImage) {
    return simpleTaskPlan(input, "vision_chat");
  }

  if (
    input.skillMention ||
    skillHubDirectInstallSkill(input.message) ||
    message.includes("skill") ||
    message.includes("技能")
  ) {
    return simpleTaskPlan(input, "skill_management");
  }

  if (!message) {
    return input.hasSelection
      ? writerPlan(input, "rewrite_selection")
      : chatPlan(input);
  }

  if (input.notePath && includesAny(message, DOCUMENT_KEYWORDS)) {
    return simpleTaskPlan(input, "document_check");
  }

  if (
    input.notePath &&
    includesAny(message, CHAPTER_KEYWORDS) &&
    !input.hasSelection
  ) {
    return simpleTaskPlan(input, "chapter");
  }

  if (input.hasSelection && includesAny(message, CITATION_KEYWORDS)) {
    return simpleTaskPlan(input, "citation_check");
  }

  if (
    includesAny(message, WRITING_KEYWORDS) ||
    hasChapterCreationIntent(message)
  ) {
    if (
      input.hasSelection &&
      (includesAny(message, CREATIVE_KEYWORDS) ||
        hasChapterCreationIntent(message))
    ) {
      return writerPlan(input, "creative_write");
    }

    if (
      input.hasSelection ||
      includesAny(message, REWRITE_KEYWORDS) ||
      contextReferencesFor(input).length > 0
    ) {
      return writerPlan(input, "rewrite_selection");
    }

    if (input.notePath || includesAny(message, CREATIVE_KEYWORDS)) {
      return writerPlan(input, "creative_write");
    }
  }

  if (
    includesAny(message, RESEARCH_KEYWORDS) &&
    (input.explicitScope || !input.notePath || message.length > 12)
  ) {
    return researchPlan(input);
  }

  if (includesAny(message, ORGANIZE_KEYWORDS)) {
    return simpleTaskPlan(input, "organize");
  }

  if (includesAny(message, AMBIGUOUS_HIGH_COST_KEYWORDS)) {
    return clarifyPlan(input, "你希望我先查笔记、做研究，还是直接给建议？");
  }

  if (
    includesAny(message, KNOWLEDGE_KEYWORDS) ||
    input.explicitScope ||
    input.notePath ||
    contextReferencesFor(input).length > 0
  ) {
    return askNotesPlan(input);
  }

  return chatPlan(input);
}
