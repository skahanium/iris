/** Build a sidebar notice after a confirmed Iris skill draft. */
export function skillConfirmSuccessNotice(input: {
  confirmedSkill?: string | null;
  preview?: Record<string, unknown> | null;
  arguments?: Record<string, unknown> | null;
}): string | null {
  const name =
    input.confirmedSkill?.trim() ||
    (typeof input.preview?.display_name === "string"
      ? input.preview.display_name.trim()
      : "") ||
    (typeof input.arguments?.name === "string"
      ? input.arguments.name.trim()
      : "");
  if (!name) {
    return null;
  }
  return `已确认 Skill「${name}」，可在设置 → Skills 查看。`;
}
