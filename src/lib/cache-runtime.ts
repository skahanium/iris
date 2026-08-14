//! 前端进程内加速缓存的统一用户操作入口。

import { clearAllEditorHtmlCache } from "@/lib/editor-html-cache";
import {
  clearNoteOpenPreparationCache,
  clearNoteOpenPerformanceEntries,
} from "@/lib/document-open-runtime";
import { clearMarkdownRenderCache } from "@/lib/markdown-contract/contract";

/** 不触碰活动标签页或涉密会话，仅释放可重建的前端加速结果。 */
export function clearFrontendAccelerationCaches(): void {
  clearAllEditorHtmlCache();
  clearNoteOpenPreparationCache("normal");
  clearMarkdownRenderCache();
  clearNoteOpenPerformanceEntries();
}
