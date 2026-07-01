/** 内联 AI 操作与 LLM 提示前缀 */
export const INLINE_AI_PROMPTS: Record<string, string> = {
  rewrite: "请改写以下文字，保持原意：",
  expand: "请扩写以下文字：",
  translate: "请翻译以下文字：",
  simplify: "请简化以下文字：",
  "fix-grammar": "请修复以下文字的语法：",
};

/**
 * 选区引用型提示词：assistant 执行路径不把完整选区正文拼进 user message，
 * 选区文字通过 `selection` 字段与 `contextReferences`（excerpt 已截断）传入，
 * 因此提示词只引用"当前选区"，避免长选区正文泄露到 prompt/cache。
 */
export const INLINE_AI_SELECTION_REFERENT_PROMPTS: Record<string, string> = {
  rewrite: "请改写当前选区的文字，保持原意。",
  expand: "请扩写当前选区的文字。",
  translate: "请翻译当前选区的文字。",
  simplify: "请简化当前选区的文字。",
  "fix-grammar": "请修复当前选区文字的语法。",
};

export function buildInlineAiUserMessage(
  action: string,
  originalText: string,
): string {
  const prefix = INLINE_AI_PROMPTS[action] ?? "请处理以下文字：";
  return `${prefix}\n\n${originalText}`;
}

/**
 * 构造只引用当前选区的提示词，用于 assistant 执行路径，避免把完整选区
 * 正文内联进 user message。
 */
export function buildInlineAiSelectionReferentPrompt(action: string): string {
  return (
    INLINE_AI_SELECTION_REFERENT_PROMPTS[action] ?? "请处理当前选区的文字。"
  );
}
