import type {
  ArtifactPlanItem,
  ContextReference,
  EditTarget,
  TaskPlan,
  TaskPlanIntent,
} from "@/types/ai";

import type { AssistantRouteInput } from "./assistant-routing";
import { isWritingConfirmationMessage } from "./assistant-write-confirmation";

export const writingKeywordBeforeResearchKeyword = true;

export interface BuildAssistantTaskPlanInput extends AssistantRouteInput {
  contextReferences?: ContextReference[];
  webAuthorized?: boolean;
  hasPendingWriteProposal?: boolean;
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

const NOTE_WRITE_VERBS = [
  "补充",
  "插入",
  "写入",
  "写进",
  "追加",
  "添加到",
  "加到",
  "放到",
  "填到",
];

const NOTE_WRITE_TARGETS = [
  "当前标题",
  "标题下",
  "下方",
  "下面",
  "此处",
  "这里",
  "光标",
  "正文",
  "文档",
  "笔记",
];

const CITATION_EVIDENCE_KEYWORDS = [
  "引用",
  "引证",
  "证据",
  "出处",
  "来源",
  "支撑",
];

const CITATION_CHECK_ACTION_KEYWORDS = [
  "核查",
  "检查",
  "验证",
  "是否充分",
  "是否可靠",
  "补充",
  "添加",
  "加上",
  "找",
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

const KNOWLEDGE_KEYWORDS = [
  "查一下",
  "查阅",
  "搜索",
  "搜一下",
  "库里",
  "文档里",
  "找一下",
];

const EXPLICIT_NOTE_REFERENCE_TERMS = [
  "当前笔记",
  "当前文档",
  "本文",
  "这篇笔记",
  "这个文档",
  "这段",
  "选中内容",
  "以上内容",
  "根据当前",
  "基于当前",
];

const EXPLICIT_RESEARCH_TERMS = [
  "联网调研",
  "联网研究",
  "研究综述",
  "文献综述",
  "多来源",
  "对比来源",
  "证据矩阵",
  "查资料",
  "找证据",
];

const RESEARCH_ACTION_TERMS = ["研究", "调研", "深挖", "文献", "论文", "取舍"];

const FRESH_EXTERNAL_FACT_TERMS = [
  "最新",
  "榜单",
  "排名",
  "排行",
  "arena",
  "swe-bench",
  "livebench",
  "新闻",
  "消息",
  "发布",
  "价格",
  "股价",
  "汇率",
  "天气",
  "赛程",
  "日程",
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

function hasExplicitCitationCheckIntent(message: string): boolean {
  return (
    includesAny(message, CITATION_EVIDENCE_KEYWORDS) &&
    includesAny(message, CITATION_CHECK_ACTION_KEYWORDS)
  );
}

function hasChapterCreationIntent(message: string): boolean {
  return /第[一二三四五六七八九十\d]+章/.test(message);
}

function hasExplicitNoteWriteIntent(message: string): boolean {
  return (
    includesAny(message, NOTE_WRITE_VERBS) &&
    includesAny(message, NOTE_WRITE_TARGETS)
  );
}

function parseMentionedMarkdownPath(message: string): string | null {
  const match = /@([^\s，,。；;）)\]]+\.md)/i.exec(message);
  return match?.[1] ?? null;
}

function parseQuotedHeadingText(message: string): string | null {
  const match = /["“'‘]([^"”'’]+)["”'’]/.exec(message);
  return match?.[1]?.trim() || null;
}

function parseOrdinal(message: string): number | null {
  const digitMatch = /第\s*(\d+)\s*个\s*(?:大标题|标题|章节)/.exec(message);
  if (digitMatch) return Number(digitMatch[1]);
  const chineseMatch =
    /第\s*([一二三四五六七八九十])\s*个\s*(?:大标题|标题|章节)/.exec(message);
  const ordinalMap: Record<string, number> = {
    一: 1,
    二: 2,
    三: 3,
    四: 4,
    五: 5,
    六: 6,
    七: 7,
    八: 8,
    九: 9,
    十: 10,
  };
  const chineseOrdinal = chineseMatch?.[1];
  return chineseOrdinal ? (ordinalMap[chineseOrdinal] ?? null) : null;
}

function editSourceForMessage(
  input: BuildAssistantTaskPlanInput,
  message: string,
): EditTarget["source"] {
  if (
    includesAny(message, [
      "刚刚",
      "刚才",
      "以上内容",
      "上述内容",
      "回答",
      "对话",
    ])
  ) {
    return "conversation";
  }
  return input.hasSelection ? "selection" : "prompt";
}

function buildEditTargetForMessage(
  input: BuildAssistantTaskPlanInput,
  rawMessage: string,
  message: string,
): EditTarget | null {
  const mentionedPath = parseMentionedMarkdownPath(rawMessage);
  const hasWriteVerb = includesAny(message, NOTE_WRITE_VERBS);
  const hasConversationSource = includesAny(message, [
    "刚刚",
    "刚才",
    "以上内容",
    "上述内容",
    "回答",
    "对话",
    "总结",
    "整理",
  ]);
  const explicitWriteTarget =
    hasExplicitNoteWriteIntent(message) ||
    Boolean(mentionedPath && hasWriteVerb) ||
    (Boolean(mentionedPath || input.notePath) &&
      hasWriteVerb &&
      hasConversationSource);

  if (!explicitWriteTarget) return null;

  const targetPath = mentionedPath ?? input.notePath ?? null;
  if (!targetPath) return null;

  const ordinal = parseOrdinal(rawMessage);
  const headingText = parseQuotedHeadingText(rawMessage);
  const placement: EditTarget["placement"] = ordinal
    ? "insert_heading_at_ordinal"
    : includesAny(message, [
          "当前标题",
          "标题下",
          "标题下方",
          "标题下面",
          "本标题",
        ])
      ? "after_heading"
      : "append_document";

  return {
    targetPath,
    source: editSourceForMessage(input, message),
    placement,
    headingText,
    headingLevel: placement === "insert_heading_at_ordinal" ? 1 : null,
    ordinal,
    range: null,
    baseContentHash: null,
  };
}

function isSimpleDateQuestion(message: string): boolean {
  return /^(今天|现在|此刻)?(是)?(几月几日|什么日期|星期几|哪一天|日期)[？?。\s]*$/.test(
    message,
  );
}

function hasExplicitContextReference(
  input: BuildAssistantTaskPlanInput,
  message: string,
): boolean {
  return (
    input.hasSelection ||
    input.explicitScope ||
    contextReferencesFor(input).length > 0 ||
    Boolean(
      input.notePath && includesAny(message, EXPLICIT_NOTE_REFERENCE_TERMS),
    )
  );
}

function hasExplicitResearchIntent(
  input: BuildAssistantTaskPlanInput,
  message: string,
): boolean {
  if (includesAny(message, EXPLICIT_RESEARCH_TERMS)) return true;
  if (input.explicitScope && includesAny(message, RESEARCH_ACTION_TERMS)) {
    return true;
  }
  return (
    !input.notePath &&
    includesAny(message, ["研究一下", "调研一下", "深挖一下"])
  );
}

function needsFreshWebEvidence(message: string): boolean {
  if (isSimpleDateQuestion(message)) return false;
  return (
    includesAny(message, FRESH_EXTERNAL_FACT_TERMS) ||
    /\b20\d{2}\b/.test(message)
  );
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
  if (input.hasPendingWriteProposal)
    hints.push("context:pending_write_proposal");
  if (input.skillMention) hints.push("skill:mention");
  if (contextReferencesFor(input).length > 0) hints.push("context:reference");
  if (
    input.webAuthorized &&
    needsFreshWebEvidence(input.message.toLowerCase())
  ) {
    hints.push("web:fresh_required");
  }

  return hints;
}

function basePlan(
  input: BuildAssistantTaskPlanInput,
): Pick<
  TaskPlan,
  | "contextReferences"
  | "retrievalMode"
  | "webMode"
  | "evidenceNeed"
  | "contextNeed"
  | "operationKind"
  | "outputShape"
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
    evidenceNeed: "none",
    contextNeed:
      input.hasSelection || contextReferences.length > 0
        ? "current_reference"
        : "none",
    operationKind: "answer",
    outputShape: "chat",
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
    | "evidenceNeed"
    | "contextNeed"
    | "operationKind"
    | "outputShape"
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
        | "evidenceNeed"
        | "contextNeed"
        | "operationKind"
        | "outputShape"
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
  editTarget?: EditTarget | null,
): TaskPlan {
  const executionMode =
    intent === "rewrite_selection" ? "patch_proposal" : "writing_candidate";
  const target =
    editTarget ??
    (intent === "rewrite_selection" && input.notePath
      ? {
          targetPath: input.notePath,
          source: "selection" as const,
          placement: "replace_selection" as const,
          range: null,
          baseContentHash: null,
        }
      : null);

  return plan(input, {
    intent,
    confidence: "high",
    retrievalMode: "current_reference",
    modelSlot: "writer",
    executionMode,
    outputMode:
      intent === "rewrite_selection" || target
        ? "confirmation_required"
        : "markdown_message",
    artifactPlan: [],
    evidenceNeed: "none",
    contextNeed: "current_reference",
    operationKind: intent === "rewrite_selection" ? "patch" : "create",
    outputShape:
      intent === "rewrite_selection" || target ? "confirmation" : "chat",
    editTarget: target,
  });
}

function researchPlan(input: BuildAssistantTaskPlanInput): TaskPlan {
  const artifactPlan: ArtifactPlanItem[] = [
    {
      kind: "evidence_sources",
      reason:
        "Research tasks need inspectable sources outside the chat stream.",
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
    evidenceNeed: "multi_source_research",
    contextNeed: input.explicitScope ? "vault_search" : "none",
    operationKind: "diagnose",
    outputShape: "artifact",
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

function freshWebShortAnswerPlan(input: BuildAssistantTaskPlanInput): TaskPlan {
  return plan(input, {
    intent: "chat",
    confidence: "medium",
    retrievalMode: "none",
    modelSlot: "fast",
    executionMode: "direct_answer",
    outputMode: "markdown_message",
    artifactPlan: [],
    evidenceNeed: "fresh_web",
    contextNeed: "none",
    operationKind: "answer",
    outputShape: "chat",
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
    "chat" | "ask_notes" | "creative_write" | "rewrite_selection" | "research"
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

export function shouldAttachNoteContextToTaskPlan(plan: TaskPlan): boolean {
  return plan.retrievalMode !== "none";
}
export function buildAssistantTaskPlan(
  input: BuildAssistantTaskPlanInput,
): TaskPlan {
  const rawMessage = input.message.trim();
  const message = rawMessage.toLowerCase();
  const uiPlan = planForUiAction(input);
  if (uiPlan) return uiPlan;

  if (input.hasImage) {
    return simpleTaskPlan(input, "vision_chat");
  }

  if (
    input.skillMention ||
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

  if (
    hasExplicitNoteWriteIntent(message) &&
    includesAny(message, ["光标", "此处", "这里"])
  ) {
    return clarifyPlan(
      input,
      "当前还没有可确认的光标字节位置。请先选中文本，或指定标题/文末等明确插入位置。",
    );
  }

  const editTarget = buildEditTargetForMessage(input, rawMessage, message);
  if (editTarget) {
    return writerPlan(input, "creative_write", editTarget);
  }

  if (input.hasPendingWriteProposal && isWritingConfirmationMessage(message)) {
    return writerPlan(input, "rewrite_selection");
  }

  if (
    input.hasSelection &&
    (includesAny(message, REWRITE_KEYWORDS) ||
      includesAny(message, ["英文", "中文", "翻成", "译成", "translate"]))
  ) {
    return writerPlan(input, "rewrite_selection");
  }

  if (input.notePath && hasExplicitNoteWriteIntent(message)) {
    return writerPlan(input, "creative_write");
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

  if (input.hasSelection && hasExplicitCitationCheckIntent(message)) {
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

  if (hasExplicitResearchIntent(input, message)) {
    return researchPlan(input);
  }

  if (input.webAuthorized && needsFreshWebEvidence(message)) {
    return freshWebShortAnswerPlan(input);
  }

  if (includesAny(message, ORGANIZE_KEYWORDS)) {
    return simpleTaskPlan(input, "organize");
  }

  if (includesAny(message, AMBIGUOUS_HIGH_COST_KEYWORDS)) {
    return clarifyPlan(input, "你希望我先查笔记、做研究，还是直接给建议？");
  }

  if (
    includesAny(message, KNOWLEDGE_KEYWORDS) ||
    hasExplicitContextReference(input, message)
  ) {
    return askNotesPlan(input);
  }

  return chatPlan(input);
}
