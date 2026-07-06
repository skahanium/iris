export interface AssistantTranscriptLine {
  role: string;
  content?: string;
}

export function isEmptyAssistantPlaceholder(
  message: AssistantTranscriptLine | undefined,
): boolean {
  return message?.role === "assistant" && !message.content?.trim();
}

export function dropTrailingEmptyAssistantPlaceholder<
  T extends AssistantTranscriptLine,
>(messages: T[]): T[] {
  return isEmptyAssistantPlaceholder(messages[messages.length - 1])
    ? messages.slice(0, -1)
    : messages;
}

export function appendSystemMessageAfterDroppingEmptyAssistant<
  T extends AssistantTranscriptLine,
>(messages: T[], content: string): T[] {
  return [
    ...dropTrailingEmptyAssistantPlaceholder(messages),
    { role: "system", content } as T,
  ];
}
