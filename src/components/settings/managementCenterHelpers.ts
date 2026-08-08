//! Pure helpers and type guards for the Management Center.
//!
//! Kept free of components so Fast Refresh rules hold in the primitives file.

import type { AppUpdateSnapshot } from "@/hooks/useAppUpdate";
import type { ManagementCenterDetail } from "@/hooks/useOverlayManager";
import type { EmbeddingIndexStatus } from "@/types/ipc";

export type AiManagementDetail =
  | "models"
  | "web-search"
  | "persona"
  | "skills"
  | "memory";

export type NotesManagementDetail = "file-sheet" | "recycle-bin";

export function updateStatusText(status: AppUpdateSnapshot["status"]) {
  switch (status) {
    case "checking":
      return "正在检查更新";
    case "up_to_date":
      return "已是最新版";
    case "available":
      return "发现新版本";
    case "downloading":
      return "正在下载更新";
    case "downloaded":
      return "更新已下载";
    case "ready_to_install":
      return "可以安装";
    case "unsupported":
      return "当前平台暂不支持应用内更新";
    case "error":
      return "无法检查更新";
    default:
      return "尚未检查";
  }
}

export function updateProgressText(appUpdate: AppUpdateSnapshot) {
  const progress = appUpdate.progress;
  if (!progress || appUpdate.status !== "downloading") return null;
  if (!progress.contentLength) {
    return `已下载 ${(progress.downloaded / 1024 / 1024).toFixed(1)} MB`;
  }
  const percent = Math.min(
    100,
    Math.round((progress.downloaded / progress.contentLength) * 100),
  );
  return `${percent}% · ${(progress.downloaded / 1024 / 1024).toFixed(1)} MB / ${(
    progress.contentLength /
    1024 /
    1024
  ).toFixed(1)} MB`;
}

export function appUpdateMessageText(message: string) {
  return message.includes("signature") ? "更新包验证失败" : message;
}

export function isAiManagementDetail(
  detail: ManagementCenterDetail,
): detail is AiManagementDetail {
  return (
    detail === "models" ||
    detail === "web-search" ||
    detail === "persona" ||
    detail === "skills" ||
    detail === "memory"
  );
}

export function isNotesManagementDetail(
  detail: ManagementCenterDetail,
): detail is NotesManagementDetail {
  return detail === "file-sheet" || detail === "recycle-bin";
}

export function embeddingFailureDetail(
  failureCode: EmbeddingIndexStatus["failureCode"],
): string {
  switch (failureCode) {
    case "model_unavailable":
      return "模型不可用，可稍后手动重试。";
    case "interrupted_migration":
    case "interrupted_restart":
      return "上一次后台重建已中断，可手动重试。";
    case "database_error":
      return "索引数据库暂不可用，可稍后手动重试。";
    case "embedding_failed":
      return "嵌入生成未完成，可手动重试。";
    default:
      return "后台重建未完成，可手动重试。";
  }
}
