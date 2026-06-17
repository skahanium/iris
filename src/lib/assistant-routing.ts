import type {
  AgentIntent,
  AssistantIntent,
  AssistantTaskStatus,
  IntentDetectionResult,
} from "@/types/ai";

export interface AssistantRouteInput {
  message: string;
  hasSelection: boolean;
  notePath: string | null;
  explicitScope: boolean;
  uiAction?: string;
  hasImage?: boolean;
  skillMention?: boolean;
}

const WRITING_KEYWORDS = [
  "改写",
  "重写",
  "润色",
  "扩写",
  "续写",
  "简化",
  "压缩",
  "总结这段",
  "重组",
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
  "对比",
  "取舍",
  "综述",
  "深挖",
  "分析",
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

export interface SkillHubDirectInstall {
  registry: "skillhub";
  skill: string;
  scope: "vault";
}

function includesAny(haystack: string, needles: string[]): boolean {
  return needles.some((needle) => haystack.includes(needle));
}

export function detectSkillHubDirectInstall(
  message: string,
): SkillHubDirectInstall | null {
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

  return {
    registry: "skillhub",
    skill,
    scope: "vault",
  };
}

function detection(
  detectedIntent: AgentIntent,
  confidence: number,
  reason: string,
  alternatives: AgentIntent[],
  fallbackBehavior: string,
  sourceHints: string[],
): IntentDetectionResult {
  return {
    detectedIntent,
    confidence,
    reason,
    alternatives,
    fallbackBehavior,
    sourceHints,
  };
}

export function legacyIntentForAgentIntent(
  intent: AgentIntent,
): AssistantIntent {
  switch (intent) {
    case "ask_notes":
      return "knowledge";
    case "rewrite_selection":
    case "write":
      return "writing";
    case "citation_check":
      return "citation";
    case "document_check":
      return "document";
    case "vision_chat":
    case "skill_management":
      return "chat";
    case "research":
    case "organize":
    case "chapter":
    case "chat":
      return intent;
  }
}

export function detectAgentIntent(
  input: AssistantRouteInput,
): IntentDetectionResult {
  const message = input.message.trim().toLowerCase();
  const hints: string[] = [];

  if (input.uiAction) hints.push(`ui_action:${input.uiAction}`);
  if (input.hasSelection) hints.push("context:selection");
  if (input.notePath) hints.push("context:note");
  if (input.explicitScope) hints.push("context:scope");
  if (input.hasImage) hints.push("context:image");
  if (input.skillMention) hints.push("skill:mention");
  const skillHubDirectInstall = detectSkillHubDirectInstall(input.message);
  if (skillHubDirectInstall) {
    hints.push(`skillhub:direct_install:${skillHubDirectInstall.skill}`);
  }

  if (!message) {
    return detection(
      input.hasSelection ? "rewrite_selection" : "chat",
      input.hasSelection ? 0.72 : 0.45,
      input.hasSelection
        ? "Selection context suggests a rewrite task."
        : "Empty input defaults to chat.",
      input.hasSelection ? ["chat"] : [],
      "Use chat and suggest likely actions if the intent remains unclear.",
      hints,
    );
  }

  if (input.uiAction) {
    const action = input.uiAction.toLowerCase();
    if (["rewrite", "summarize", "translate", "simplify"].includes(action)) {
      return detection(
        "rewrite_selection",
        0.95,
        `UI action ${input.uiAction} explicitly requested a selection rewrite.`,
        ["write", "chat"],
        "Run the selected action; fall back to chat if the selection is missing.",
        hints,
      );
    }
    if (["citation", "citation_check"].includes(action)) {
      return detection(
        "citation_check",
        0.95,
        `UI action ${input.uiAction} explicitly requested citation checking.`,
        ["ask_notes"],
        "Run citation checking; fall back to note lookup if no paragraph is available.",
        hints,
      );
    }
  }

  if (input.hasImage) {
    return detection(
      "vision_chat",
      0.9,
      "Attached image context requires a vision-capable assistant path.",
      ["chat"],
      "Use vision when available; fall back to chat if no image-capable model is configured.",
      hints,
    );
  }

  if (
    input.skillMention ||
    skillHubDirectInstall ||
    message.includes("skill") ||
    message.includes("技能")
  ) {
    return detection(
      "skill_management",
      input.skillMention ? 0.9 : 0.78,
      "The request mentions skill management.",
      ["chat"],
      "Open the skill-management path; fall back to chat with guidance if unsupported.",
      hints,
    );
  }

  if (
    includesAny(message, RESEARCH_KEYWORDS) &&
    (input.explicitScope || !input.notePath || message.length > 12)
  ) {
    return detection(
      "research",
      0.84,
      "Research keywords and scope suggest a multi-evidence research task.",
      ["ask_notes", "chat"],
      "Run research; fall back to ask_notes if the task can be answered from local notes.",
      hints,
    );
  }

  if (input.notePath && includesAny(message, DOCUMENT_KEYWORDS)) {
    return detection(
      "document_check",
      0.86,
      "Document-level check keywords were found in an open note.",
      ["chapter", "rewrite_selection"],
      "Run document checking; fall back to chat with suggested document actions.",
      hints,
    );
  }

  if (input.notePath && includesAny(message, CHAPTER_KEYWORDS)) {
    return detection(
      "chapter",
      0.84,
      "Chapter-level keywords were found in an open note.",
      ["write", "document_check"],
      "Run chapter writing; fall back to write if no chapter can be parsed.",
      hints,
    );
  }

  if (includesAny(message, ORGANIZE_KEYWORDS)) {
    return detection(
      "organize",
      0.82,
      "Organization keywords suggest metadata or vault cleanup.",
      ["ask_notes", "chat"],
      "Run organize suggestions; fall back to chat if no actionable scope exists.",
      hints,
    );
  }

  if (input.hasSelection && includesAny(message, CITATION_KEYWORDS)) {
    return detection(
      "citation_check",
      0.88,
      "Selection plus citation keywords suggest citation checking.",
      ["ask_notes", "rewrite_selection"],
      "Run citation checking; fall back to ask_notes if no claim can be extracted.",
      hints,
    );
  }

  if (input.hasSelection && includesAny(message, WRITING_KEYWORDS)) {
    return detection(
      "rewrite_selection",
      0.88,
      "Selection plus writing keywords suggest rewriting selected text.",
      ["write", "chat"],
      "Run selection rewrite; fall back to chat if the selection is unavailable.",
      hints,
    );
  }

  if (input.notePath && includesAny(message, WRITING_KEYWORDS)) {
    return detection(
      "write",
      0.76,
      "Writing keywords in an open note suggest a note-level writing task.",
      ["rewrite_selection", "chat"],
      "Run writing assistance; fall back to chat if no safe patch target is available.",
      hints,
    );
  }

  if (includesAny(message, KNOWLEDGE_KEYWORDS) || input.explicitScope) {
    return detection(
      "ask_notes",
      0.78,
      "Lookup keywords or explicit scope suggest asking over local notes.",
      ["chat", "research"],
      "Use ask_notes; fall back to chat and suggest research if evidence is broad.",
      hints,
    );
  }

  if (input.hasSelection && input.notePath) {
    return detection(
      "rewrite_selection",
      0.7,
      "Selection in an open note suggests a writing action.",
      ["chat"],
      "Use selection rewrite; fall back to chat if the request is conversational.",
      hints,
    );
  }

  return detection(
    "chat",
    0.45,
    "No stronger Phase2 intent matched.",
    ["ask_notes", "write"],
    "Use chat and suggest actions when helpful.",
    hints,
  );
}

export function resolveAssistantIntent(
  input: AssistantRouteInput,
): AssistantIntent {
  return legacyIntentForAgentIntent(detectAgentIntent(input).detectedIntent);
}

export function assistantIntentLabel(intent: AssistantIntent): string {
  switch (intent) {
    case "knowledge":
      return "知识查阅";
    case "writing":
      return "改写选区";
    case "citation":
      return "检查引用";
    case "organize":
      return "整理建议";
    case "research":
      return "研究中";
    case "chapter":
      return "章节写作";
    case "document":
      return "文档检查";
    case "chat":
      return "对话";
  }
}

export function assistantStatusText(status: AssistantTaskStatus): string {
  switch (status) {
    case "idle":
      return "待命";
    case "running":
      return "处理中";
    case "awaiting_confirmation":
      return "等待确认";
    case "completed":
      return "已完成";
    case "error":
      return "出错";
  }
}
