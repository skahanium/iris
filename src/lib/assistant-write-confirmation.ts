export type PendingWriteConfirmationAction =
  | "none"
  | "apply_single_patch"
  | "clarify_multiple_patches";

interface PendingWriteConfirmationInput {
  message: string;
  pendingPatchCount: number;
}

const CONFIRMATION_PATTERNS = [
  /^我确认[。！!]?$/,
  /^确认[。！!]?$/,
  /^是[。！!]?$/,
  /^好的?[。！!]?$/,
  /^可以[。！!]?$/,
  /^同意[。！!]?$/,
  /^接受[。！!]?$/,
  /^应用[。！!]?$/,
  /^执行[。！!]?$/,
  /^按此修改[。！!]?$/,
  /^就这样[。！!]?$/,
];

export function isWritingConfirmationMessage(message: string): boolean {
  const normalized = message.trim();
  return CONFIRMATION_PATTERNS.some((pattern) => pattern.test(normalized));
}

export function pendingWriteConfirmationAction({
  message,
  pendingPatchCount,
}: PendingWriteConfirmationInput): PendingWriteConfirmationAction {
  if (pendingPatchCount <= 0 || !isWritingConfirmationMessage(message)) {
    return "none";
  }
  return pendingPatchCount === 1
    ? "apply_single_patch"
    : "clarify_multiple_patches";
}
