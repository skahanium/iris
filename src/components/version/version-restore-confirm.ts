import type { VersionEntry } from "@/types/ipc";

/**
 * Browser confirm copy before replacing current note body with a snapshot.
 */
export function buildRestoreConfirmMessage(
  target: VersionEntry,
  hasUnsavedEdits: boolean,
  targetPath?: string,
): string {
  let base =
    "将用所选版本替换当前正文。\n恢复前会自动保存一条「恢复前备份」，便于你撤销本次恢复。";

  if (target.is_legacy_unscoped) {
    base += `\n\n此旧历史无法确认原笔记库归属。你将明确将其恢复到当前笔记：${targetPath ?? "未选择目标"}。原历史的未知归属不会因此改变。`;
  }

  if (target.is_finalized || target.kind === "finalize") {
    return `${base}\n\n你正在从定稿版本恢复。未保存的修改将被覆盖。确定继续吗？`;
  }

  if (hasUnsavedEdits) {
    return `${base}\n\n当前笔记有未保存的修改，恢复后将被覆盖。确定继续吗？`;
  }

  return `${base}\n\n确定继续吗？`;
}
