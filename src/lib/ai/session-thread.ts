/** 面板内是否已有带内容的助手回复（用于判断是否应开启新 DB session） */
export function hasAssistantTurnInPanel(
  messages: ReadonlyArray<{ role: string; content?: string }>,
): boolean {
  return messages.some(
    (m) => m.role === "assistant" && Boolean(m.content?.trim()),
  );
}

/**
 * 是否向后端请求 `newSession`。
 * 面板无历史助手回复时视为新线程，避免 UI 已清空仍加载 SQLite 里同 scene+笔记 的旧消息。
 */
export function shouldStartNewAiSession(
  messages: ReadonlyArray<{ role: string; content?: string }>,
  forceNew: boolean,
): boolean {
  if (forceNew) return true;
  return !hasAssistantTurnInPanel(messages);
}
