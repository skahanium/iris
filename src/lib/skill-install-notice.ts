/** Build a sidebar notice after a successful skills_install tool confirmation. */
export function skillInstallSuccessNotice(input: {
  installedSkill?: string | null;
  preview?: Record<string, unknown> | null;
  arguments?: Record<string, unknown> | null;
}): string | null {
  const name =
    input.installedSkill?.trim() ||
    (typeof input.preview?.display_name === "string"
      ? input.preview.display_name.trim()
      : "") ||
    (typeof input.arguments?.path_or_url === "string"
      ? input.arguments.path_or_url.trim()
      : "");
  if (!name) {
    return null;
  }
  return `已安装 Skill「${name}」，可在设置 → Skills 查看。`;
}
