/** `/` 命令对应的用户消息 */
export const SLASH_COMMAND_PROMPTS: Record<string, string> = {
  summarize: "请总结当前笔记要点",
  outline: "请生成结构化大纲",
  brainstorm: "请就此主题头脑风暴",
  "fix-grammar": "请修复语法问题",
  translate: "请翻译全文",
};

export function buildSlashCommandMessage(command: string): string {
  return SLASH_COMMAND_PROMPTS[command] ?? command;
}

export function slashActionId(command: string): string {
  return `slash:${command}`;
}

export function parseSlashActionId(action: string): string | null {
  return action.startsWith("slash:") ? action.slice("slash:".length) : null;
}
