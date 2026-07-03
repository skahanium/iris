export function isUnrecoverableResumeError(message: string | null): boolean {
  if (!message) return false;
  const lower = message.toLowerCase();
  return (
    lower.includes("resume_preflight_failed") ||
    lower.includes("checkpoint_missing") ||
    lower.includes("未找到可恢复") ||
    lower.includes("vault scope changed") ||
    lower.includes("note path unavailable")
  );
}

export function resumeRecoveryMessage(message: string): string {
  if (isUnrecoverableResumeError(message)) {
    return "当前库已变更，暂停任务不可恢复。请在当前库重新发起任务。";
  }
  return message;
}
