import type { VersionSaveCompleteEvent } from "@/types/ipc";

/** User-facing status line after async version snapshot completes. */
export function formatVersionSaveStatus(
  payload: VersionSaveCompleteEvent,
): string {
  if (payload.error) {
    return `版本快照失败：${payload.error}`;
  }
  if (payload.created) {
    return payload.kind === "auto_idle"
      ? "已创建空闲版本备份"
      : "已创建版本快照";
  }
  return "内容未变化，已跳过版本快照";
}
