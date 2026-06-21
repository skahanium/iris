import type {
  AgentIntent,
  AssistantIntent,
  AssistantTaskStatus,
  IntentDetectionResult,
  ContextReference,
  TaskPlan,
} from "@/types/ai";

import { buildAssistantTaskPlan } from "./assistant-taskplan";

export interface AssistantRouteInput {
  message: string;
  hasSelection: boolean;
  notePath: string | null;
  contextReferences?: ContextReference[];
  explicitScope: boolean;
  uiAction?: string;
  hasImage?: boolean;
  skillMention?: boolean;
}

export interface SkillHubDirectInstall {
  registry: "skillhub";
  skill: string;
  scope: "vault";
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

export function agentIntentForTaskPlan(plan: TaskPlan): AgentIntent {
  if (plan.intent === "creative_write") return "write";
  return plan.intent;
}

function confidenceScore(plan: TaskPlan): number {
  switch (plan.confidence) {
    case "high":
      return 0.9;
    case "medium":
      return 0.78;
    case "low":
      return plan.requiresClarification ? 0.35 : 0.45;
  }
}

function alternativesFor(plan: TaskPlan): AgentIntent[] {
  switch (plan.intent) {
    case "chat":
      return ["ask_notes", "write"];
    case "ask_notes":
      return ["chat", "research"];
    case "creative_write":
      return ["rewrite_selection", "chat"];
    case "rewrite_selection":
      return ["write", "chat"];
    case "citation_check":
      return ["ask_notes", "rewrite_selection"];
    case "research":
      return ["ask_notes", "chat"];
    case "organize":
      return ["ask_notes", "chat"];
    case "document_check":
      return ["chapter", "rewrite_selection"];
    case "chapter":
      return ["write", "document_check"];
    case "vision_chat":
      return ["chat"];
    case "skill_management":
      return ["chat"];
  }
}

function reasonFor(plan: TaskPlan): string {
  if (plan.requiresClarification) {
    return "TaskPlan needs clarification before choosing a costly path.";
  }
  if (plan.sourceHints.some((hint) => hint.startsWith("ui_action:"))) {
    return "UI action selected the TaskPlan intent.";
  }
  switch (plan.intent) {
    case "chat":
      return "No stronger TaskPlan intent matched.";
    case "ask_notes":
      return "TaskPlan selected local note lookup for the current request.";
    case "creative_write":
      return "TaskPlan selected creative writing before research keywords.";
    case "rewrite_selection":
      return "TaskPlan selected selection rewrite.";
    case "citation_check":
      return "TaskPlan selected citation checking.";
    case "research":
      return "TaskPlan selected a multi-evidence research task.";
    case "organize":
      return "TaskPlan selected organization work.";
    case "document_check":
      return "TaskPlan selected document-level checking.";
    case "chapter":
      return "TaskPlan selected chapter writing.";
    case "vision_chat":
      return "TaskPlan selected vision chat for image context.";
    case "skill_management":
      return "TaskPlan selected skill management.";
  }
}

function fallbackFor(plan: TaskPlan): string {
  if (plan.requiresClarification) {
    return plan.clarificationQuestion ?? "Ask a clarification question.";
  }
  switch (plan.intent) {
    case "chat":
      return "Use chat and suggest actions when helpful.";
    case "ask_notes":
      return "Use ask_notes; fall back to chat if note context is insufficient.";
    case "creative_write":
      return "Run writing assistance; fall back to chat if no safe writing context exists.";
    case "rewrite_selection":
      return "Run selection rewrite; fall back to chat if the selection is unavailable.";
    case "citation_check":
      return "Run citation checking; fall back to ask_notes if no claim can be extracted.";
    case "research":
      return "Run research; fall back to ask_notes if the task can be answered from local notes.";
    case "organize":
      return "Run organize suggestions; fall back to chat if no actionable scope exists.";
    case "document_check":
      return "Run document checking; fall back to chat with suggested document actions.";
    case "chapter":
      return "Run chapter writing; fall back to write if no chapter can be parsed.";
    case "vision_chat":
      return "Use vision when available; fall back to chat if no image-capable model is configured.";
    case "skill_management":
      return "Open the skill-management path; fall back to chat with guidance if unsupported.";
  }
}

export function detectAgentIntent(
  input: AssistantRouteInput,
): IntentDetectionResult {
  return intentDetectionForTaskPlan(buildAssistantTaskPlan(input));
}

export function intentDetectionForTaskPlan(
  plan: TaskPlan,
): IntentDetectionResult {
  return {
    detectedIntent: agentIntentForTaskPlan(plan),
    confidence: confidenceScore(plan),
    reason: reasonFor(plan),
    alternatives: alternativesFor(plan),
    fallbackBehavior: fallbackFor(plan),
    sourceHints: plan.sourceHints,
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
    case "paused_budget":
      return "可继续";
    case "paused_recoverable":
      return "可恢复";
    case "completed":
      return "已完成";
    case "error":
      return "出错";
  }
}
