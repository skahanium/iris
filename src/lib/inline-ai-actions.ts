/** 编辑器内联建议块支持的动作（与 `aiStream` 节点 attrs.action 一致） */
export const INLINE_AI_ACTIONS = [
  "continue",
  "rewrite",
  "expand",
  "simplify",
  "cite",
  "check",
] as const;

export type InlineAiAction = (typeof INLINE_AI_ACTIONS)[number];

export const INLINE_AI_ACTION_LABELS: Record<InlineAiAction, string> = {
  continue: "续写",
  rewrite: "改写",
  expand: "扩写",
  simplify: "简化",
  cite: "引用",
  check: "检查",
};

export function inlineAiActionLabel(action: string): string {
  if (action in INLINE_AI_ACTION_LABELS) {
    return INLINE_AI_ACTION_LABELS[action as InlineAiAction];
  }
  return action;
}
