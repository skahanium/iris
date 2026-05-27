import { useVirtualizer } from "@tanstack/react-virtual";
import { FileDown, FolderPlus, Pencil, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Input } from "@/components/ui/input";
import { PromptDialog } from "@/components/common/PromptDialog";
import {
  exportFile,
  fileCreate,
  fileDelete,
  fileList,
  fileRead,
  fileRename,
  templateCreate,
  templateList,
} from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { markdownToHtmlPage } from "@/lib/markdown";
import type { FileListItem } from "@/types/ipc";

interface FileSheetProps {
  open: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
}

export function FileSheet({ open, onClose, onOpen }: FileSheetProps) {
  const [files, setFiles] = useState<FileListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newName, setNewName] = useState("新笔记.md");
  const [templates, setTemplates] = useState<{ name: string }[]>([]);
  const [showTemplates, setShowTemplates] = useState(false);
  const [renameTarget, setRenameTarget] = useState<FileListItem | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 40,
    overscan: 10,
  });

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    void fileList()
      .then(setFiles)
      .catch((e) =>
        setError(e instanceof Error ? e.message : "加载文件列表失败"),
      )
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (open) {
      refresh();
      void templateList().then(setTemplates);
      setShowTemplates(false);
    }
  }, [open, refresh]);

  const createFromTemplate = async (tmplName: string) => {
    await templateCreate(newName, tmplName);
    refresh();
    onOpen(newName);
    setShowTemplates(false);
  };

  const handleExportHtml = useCallback(async (path: string) => {
    const md = await fileRead(path);
    const title = path.replace(/\.md$/, "").split("/").pop() ?? "note";
    const html = markdownToHtmlPage(md, title);
    const destPath = path.replace(/\.md$/, ".html");
    await exportFile(destPath, html);
  }, []);

  return (
    <>
      <IrisOverlay open={open} onClose={onClose} title="文件" size="command">
        <div className="flex gap-2 border-b border-border p-2">
          <Input
            className="text-xs"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
          />
          <Button
            type="button"
            size="icon"
            variant="outline"
            title="新建"
            onClick={async () => {
              if (showTemplates && templates.length > 0) return;
              const trimmed = newName.trim();
              if (trimmed) {
                await fileCreate(trimmed);
                refresh();
                onOpen(trimmed);
                return;
              }
              const created = await createDefaultNote();
              refresh();
              onOpen(created.path);
            }}
          >
            <FolderPlus className="h-4 w-4" />
          </Button>
        </div>
        {templates.length > 0 && (
          <div className="border-b border-border px-2 pb-2">
            <button
              type="button"
              className="text-xs text-muted-foreground hover:text-primary"
              onClick={() => setShowTemplates(!showTemplates)}
            >
              {showTemplates ? "收起模板" : "从模板新建…"}
            </button>
            {showTemplates && (
              <div className="mt-1 flex flex-wrap gap-1">
                {templates.map((t) => (
                  <Button
                    key={t.name}
                    type="button"
                    size="sm"
                    variant="outline"
                    className="text-xs"
                    onClick={() => void createFromTemplate(t.name)}
                  >
                    {t.name}
                  </Button>
                ))}
              </div>
            )}
          </div>
        )}
        {error && <p className="px-3 py-2 text-xs text-destructive">{error}</p>}
        <div ref={parentRef} className="min-h-0 flex-1 overflow-auto">
          {loading ? (
            <p className="p-3 text-xs text-muted-foreground">加载中…</p>
          ) : (
            <div
              style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}
            >
              {virtualizer.getVirtualItems().map((virtualItem) => {
                const f = files[virtualItem.index]!;
                return (
                  <div
                    key={f.path}
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      width: "100%",
                      height: `${virtualItem.size}px`,
                      transform: `translateY(${virtualItem.start}px)`,
                    }}
                    className="flex items-center gap-1 border-b border-border/50 px-2 py-1.5 text-sm"
                  >
                    <button
                      type="button"
                      className="min-w-0 flex-1 truncate text-left hover:text-primary"
                      onClick={() => {
                        onOpen(f.path);
                        onClose();
                      }}
                    >
                      {f.title}
                    </button>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      onClick={() => setRenameTarget(f)}
                    >
                      <Pencil className="h-3 w-3" />
                    </Button>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      onClick={() => setDeleteTarget(f)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      title="导出 HTML"
                      onClick={() => void handleExportHtml(f.path)}
                    >
                      <FileDown className="h-3 w-3" />
                    </Button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </IrisOverlay>

      <PromptDialog
        open={renameTarget !== null}
        title="重命名文件"
        label="新路径"
        defaultValue={renameTarget?.path ?? ""}
        onCancel={() => setRenameTarget(null)}
        onSubmit={async (next) => {
          if (!renameTarget || !next || next === renameTarget.path) {
            setRenameTarget(null);
            return;
          }
          await fileRename(renameTarget.path, next);
          setRenameTarget(null);
          refresh();
        }}
      />

      <ConfirmDialog
        open={deleteTarget !== null}
        title="删除文件"
        message={`确定删除「${deleteTarget?.title ?? deleteTarget?.path ?? ""}」？正文、时间线快照与定稿将一并移入回收站，15 天内可恢复。`}
        confirmLabel="删除"
        variant="destructive"
        onCancel={() => setDeleteTarget(null)}
        onConfirm={async () => {
          if (!deleteTarget) return;
          await fileDelete(deleteTarget.path);
          setDeleteTarget(null);
          refresh();
        }}
      />
    </>
  );
}
