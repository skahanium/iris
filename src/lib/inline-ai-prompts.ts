/** 内联 AI 操作与 LLM 提示前缀 */
export const INLINE_AI_PROMPTS: Record<string, string> = {
  rewrite: "请改写以下文字，保持原意：",
  expand: "请扩写以下文字：",
  translate: "请翻译以下文字：",
  simplify: "请简化以下文字：",
  "fix-grammar": "请修复以下文字的语法：",
};

export function buildInlineAiUserMessage(
  action: string,
  originalText: string,
): string {
  const prefix = INLINE_AI_PROMPTS[action] ?? "请处理以下文字：";
  return `${prefix}\n\n${originalText}`;
}
