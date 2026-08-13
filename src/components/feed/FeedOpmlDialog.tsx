//! OPML 导入导出对话框（阶段 5）。
//!
//! 导入：open 对话框选文件 → fs 读内容 → `dryRun` 预览新增/更新/跳过计数 →
//!       确认（可选同步新订阅并选择历史是否未读）→ 执行。
//! 导出：`feedOpmlExport` 生成文档 → save 对话框 → fs 写文件。
//! 文件选择/保存由前端 dialog + fs 完成，Rust 命令只收有界 UTF-8 字符串；
//! 错误只展示稳定错误码的安全文案，不展示 URL/正文/OPML 内容。

import {
  open as openFileDialog,
  save as saveFileDialog,
} from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { useCallback, useEffect, useState } from "react";
import { Download, Loader2, TriangleAlert, Upload } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { feedOpmlExport, feedOpmlImport, feedSyncBatch } from "@/lib/ipc";
import { normalizeOpenDialogPath } from "@/lib/dialog-path";
import type { OpmlImportResult } from "@/types/ipc";

export interface FeedOpmlDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 导入/导出完成后刷新源列表。 */
  onSourcesChanged: () => void;
  /** 无订阅时导出没有语义，明确禁用而非生成空文件。 */
  hasSources?: boolean;
}

interface ImportPreview {
  fileName: string;
  result: OpmlImportResult;
}

/** 稳定错误码 → 安全文案；不展示 URL、正文或 OPML 内容。 */
function importErrorMessage(error: unknown): string {
  const code = (error as { code?: string })?.code;
  switch (code) {
    case "feed_opml_too_large":
      return "OPML 文件超过 5 MiB 上限，无法导入。";
    case "feed_xml_unsafe_declaration":
      return "文件包含不允许的 DTD/ENTITY 声明，已拒绝导入。";
    case "feed_opml_parse_failed":
      return "无法解析该 OPML 文件，请检查文件内容。";
    default:
      return "导入失败，请稍后重试。";
  }
}

export function FeedOpmlDialog({
  open,
  onOpenChange,
  onSourcesChanged,
  hasSources = true,
}: FeedOpmlDialogProps) {
  // 导入流程状态。
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [rawXml, setRawXml] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // 确认选项：同步新订阅；新订阅历史是否未读。
  const [syncNewSources, setSyncNewSources] = useState(true);
  const [historyUnread, setHistoryUnread] = useState(false);
  // 导出流程状态。
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [exported, setExported] = useState(false);

  // 每次打开重置流程状态。
  useEffect(() => {
    if (open) {
      setPreview(null);
      setRawXml(null);
      setBusy(false);
      setError(null);
      setSyncNewSources(true);
      setHistoryUnread(false);
      setExportError(null);
      setExported(false);
    }
  }, [open]);

  const pickAndPreview = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      const selected = await openFileDialog({
        multiple: false,
        title: "选择 OPML 文件",
        filters: [{ name: "OPML", extensions: ["opml", "xml"] }],
      });
      const path = normalizeOpenDialogPath(selected);
      if (!path) return;
      const xml = await readTextFile(path);
      const result = await feedOpmlImport(xml, true);
      setPreview({
        fileName: path.split("/").pop() ?? path,
        result,
      });
      setRawXml(xml);
    } catch (caught) {
      setError(importErrorMessage(caught));
    } finally {
      setBusy(false);
    }
  }, []);

  const confirmImport = useCallback(async () => {
    if (!rawXml) return;
    setBusy(true);
    setError(null);
    try {
      const result = await feedOpmlImport(rawXml, false);
      // 后端按固定并发上限批量同步，避免 OPML 大批量导入压垮网络与数据库。
      if (syncNewSources && result.addedIds.length > 0) {
        await feedSyncBatch(result.addedIds, !historyUnread);
      }
      onSourcesChanged();
      onOpenChange(false);
    } catch (caught) {
      setError(importErrorMessage(caught));
      setBusy(false);
    }
  }, [historyUnread, onOpenChange, onSourcesChanged, rawXml, syncNewSources]);

  const runExport = useCallback(async () => {
    setExportError(null);
    setExported(false);
    setExporting(true);
    try {
      const xml = await feedOpmlExport();
      const target = await saveFileDialog({
        title: "导出订阅为 OPML",
        defaultPath: "iris-subscriptions.opml",
        filters: [{ name: "OPML", extensions: ["opml"] }],
      });
      if (!target) return;
      await writeTextFile(target, xml);
      setExported(true);
    } catch (caught) {
      setExportError(
        (caught as { code?: string })?.code === "feed_opml_too_large"
          ? "导出内容超过上限，无法保存。"
          : "导出失败，请稍后重试。",
      );
    } finally {
      setExporting(false);
    }
  }, []);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent data-testid="feed-opml-dialog" className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>OPML 导入导出</DialogTitle>
          <DialogDescription>
            通过 OPML 迁移订阅关系，不包含阅读状态与本地信息。
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[min(58vh,28rem)] space-y-3 overflow-y-auto px-4 pb-2">
          {/* 导入区 */}
          <div className="rounded-md border border-border-subtle p-3">
            <p className="mb-2 text-caption font-medium text-muted-foreground">
              导入
            </p>
            {!preview ? (
              <Button
                type="button"
                data-testid="feed-opml-pick"
                variant="outline"
                onClick={() => void pickAndPreview()}
                disabled={busy}
              >
                {busy ? (
                  <Loader2
                    className="h-4 w-4 animate-spin"
                    aria-hidden="true"
                  />
                ) : (
                  <Upload className="h-4 w-4" aria-hidden="true" />
                )}
                选择 OPML 文件
              </Button>
            ) : (
              <div
                data-testid="feed-opml-preview"
                className="space-y-3 rounded-md border border-border-subtle bg-panel px-3 py-2"
              >
                <p className="text-ui">
                  {preview.fileName}：新增{" "}
                  <span data-testid="feed-opml-preview-added">
                    {preview.result.added}
                  </span>{" "}
                  · 更新{" "}
                  <span data-testid="feed-opml-preview-updated">
                    {preview.result.updated}
                  </span>{" "}
                  · 跳过{" "}
                  <span data-testid="feed-opml-preview-skipped">
                    {preview.result.skipped}
                  </span>
                </p>
                <label className="flex items-center gap-2 text-ui">
                  <input
                    type="checkbox"
                    data-testid="feed-opml-sync-new"
                    checked={syncNewSources}
                    onChange={(event) =>
                      setSyncNewSources(event.target.checked)
                    }
                  />
                  导入后同步新订阅
                </label>
                {syncNewSources ? (
                  <label className="flex items-center gap-2 text-ui">
                    <input
                      type="checkbox"
                      data-testid="feed-opml-history-unread"
                      checked={historyUnread}
                      onChange={(event) =>
                        setHistoryUnread(event.target.checked)
                      }
                    />
                    新订阅历史也设为未读
                  </label>
                ) : null}
                <div className="flex gap-2">
                  <Button
                    type="button"
                    data-testid="feed-opml-import-confirm"
                    onClick={() => void confirmImport()}
                    disabled={busy}
                  >
                    {busy ? (
                      <Loader2
                        className="h-4 w-4 animate-spin"
                        aria-hidden="true"
                      />
                    ) : null}
                    导入
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    data-testid="feed-opml-import-cancel"
                    onClick={() => setPreview(null)}
                    disabled={busy}
                  >
                    取消
                  </Button>
                </div>
              </div>
            )}
            {error ? (
              <p
                data-testid="feed-opml-import-error"
                className="flex items-center gap-1 text-caption text-warning"
              >
                <TriangleAlert
                  className="h-3.5 w-3.5 shrink-0"
                  aria-hidden="true"
                />
                {error}
              </p>
            ) : null}
          </div>

          {/* 导出区 */}
          <div className="rounded-md border border-border-subtle p-3">
            <p className="text-caption font-medium text-muted-foreground">
              导出
            </p>
            <Button
              type="button"
              data-testid="feed-opml-export"
              variant="outline"
              onClick={() => void runExport()}
              disabled={exporting || !hasSources}
            >
              {exporting ? (
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              ) : (
                <Download className="h-4 w-4" aria-hidden="true" />
              )}
              导出全部订阅
            </Button>
            {!hasSources ? (
              <p className="mt-2 text-caption text-muted-foreground">
                还没有订阅源，暂不能导出。
              </p>
            ) : null}
            {exported ? (
              <p
                data-testid="feed-opml-export-done"
                className="text-caption text-muted-foreground"
              >
                已导出到所选位置。
              </p>
            ) : null}
            {exportError ? (
              <p
                data-testid="feed-opml-export-error"
                className="flex items-center gap-1 text-caption text-warning"
              >
                <TriangleAlert
                  className="h-3.5 w-3.5 shrink-0"
                  aria-hidden="true"
                />
                {exportError}
              </p>
            ) : null}
          </div>
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="ghost"
            data-testid="feed-opml-close"
            onClick={() => onOpenChange(false)}
            disabled={busy || exporting}
          >
            关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
