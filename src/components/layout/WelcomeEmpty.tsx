import { Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { IrisMark } from "@/components/brand/IrisMark";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { displayTitleForFileListItem } from "@/lib/note-display";
import { fileDelete, fileList } from "@/lib/ipc";
import type { FileListItem } from "@/types/ipc";

interface WelcomeEmptyProps {
  /** Reload recent list when vault changes. */
  vaultKey?: string | null;
  onOpen: (path: string) => void;
  onNew: () => void | Promise<void>;
  onQuickOpen?: () => void;
  onSearch?: () => void;
  onAiSystemCenter?: () => void;
}

function dedupeByPath(files: FileListItem[]): FileListItem[] {
  const byPath = new Map<string, FileListItem>();
  for (const f of files) {
    if (!byPath.has(f.path)) {
      byPath.set(f.path, f);
    }
  }
  return [...byPath.values()];
}

export function WelcomeEmpty({
  vaultKey,
  onOpen,
  onNew,
  onQuickOpen,
  onSearch,
  onAiSystemCenter,
}: WelcomeEmptyProps) {
  const [recent, setRecent] = useState<FileListItem[]>([]);
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);

  const loadRecent = useCallback(() => {
    void fileList().then((files) => setRecent(dedupeByPath(files).slice(0, 5)));
  }, []);

  useEffect(() => {
    loadRecent();
  }, [loadRecent, vaultKey]);

  return (
    <div
      data-testid="home-workbench"
      className="flex flex-1 flex-col items-center justify-center gap-8 bg-background px-6 py-12"
    >
      <div className="w-full max-w-md rounded-xl border border-border/80 bg-surface-elevated px-8 py-10 text-center shadow-floating">
        <div className="mb-6 flex justify-center">
          <IrisMark size={56} title="Iris" />
        </div>
        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button
            type="button"
            className="min-w-[6.5rem]"
            onClick={() => {
              void (async () => {
                await onNew();
                loadRecent();
              })();
            }}
          >
            新建笔记
          </Button>
          {onQuickOpen ? (
            <Button type="button" variant="outline" onClick={onQuickOpen}>
              快速打开
            </Button>
          ) : null}
          {onSearch ? (
            <Button type="button" variant="outline" onClick={onSearch}>
              全库搜索
            </Button>
          ) : null}
          {onAiSystemCenter ? (
            <Button type="button" variant="outline" onClick={onAiSystemCenter}>
              AI 系统中心
            </Button>
          ) : null}
        </div>
      </div>
      {recent.length > 0 && (
        <div className="w-full max-w-md rounded-lg border border-border/80 bg-surface-elevated p-3 shadow-sm">
          <ul className="space-y-0.5">
            {recent.map((f) => (
              <li
                key={f.path}
                className="group flex items-center rounded-md transition-colors duration-base ease-iris-out hover:bg-surface-inset/80"
              >
                <button
                  type="button"
                  className="min-w-0 flex-1 truncate px-2 py-2 text-left text-sm text-foreground"
                  onClick={() => onOpen(f.path)}
                >
                  {displayTitleForFileListItem(f)}
                </button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="mr-0.5 h-8 w-8 shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
                  aria-label={`删除 ${displayTitleForFileListItem(f)}`}
                  onClick={() => setDeleteTarget(f)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}

      <ConfirmDialog
        open={deleteTarget !== null}
        title="删除笔记"
        message={`确定删除「${deleteTarget ? displayTitleForFileListItem(deleteTarget) : ""}」？`}
        description="正文、时间线快照与定稿将一并移入回收站，15 天内可恢复。"
        confirmLabel="删除"
        variant="destructive"
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => {
          if (!deleteTarget) return;
          void (async () => {
            await fileDelete(deleteTarget.path);
            setDeleteTarget(null);
            loadRecent();
          })();
        }}
      />
    </div>
  );
}
