/** 用户可读的助手上下文描述（避免「未绑定文档」等内部术语） */

export interface AssistantContextLabelInput {
  selectionText?: string | null;
  noteDisplayTitle?: string | null;
}

export function describeAssistantContext(
  input: AssistantContextLabelInput,
): string {
  const selection = input.selectionText?.trim();
  if (selection) {
    const preview =
      selection.length > 36 ? `${selection.slice(0, 36)}…` : selection;
    return `已选中文本：${preview}`;
  }
  const title = input.noteDisplayTitle?.trim();
  if (title) {
    return `当前笔记：${title}`;
  }
  return "未打开笔记";
}

/** 空闲时副标题：只展示上下文；忙碌时再带上任务与状态 */
export function describeAssistantSubtitle(input: {
  status: "idle" | "running" | "awaiting_confirmation" | "completed" | "error";
  contextLabel: string;
  intentLabel: string;
  statusLabel: string;
  showTaskHint: boolean;
}): string {
  if (input.status === "idle") {
    return input.contextLabel;
  }
  if (input.showTaskHint) {
    return `${input.intentLabel} · ${input.statusLabel}`;
  }
  return input.statusLabel;
}
