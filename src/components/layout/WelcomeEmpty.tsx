import { Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { IrisMark } from "@/components/brand/IrisMark";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { fileDelete, fileList } from "@/lib/ipc";
import type { FileListItem } from "@/types/ipc";

interface WelcomeEmptyProps {
  /** Reload recent list when vault changes. */
  vaultKey?: string | null;
  onOpen: (path: string) => void;
  onNew: () => void | Promise<void>;
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

export function WelcomeEmpty({ vaultKey, onOpen, onNew }: WelcomeEmptyProps) {
  const [recent, setRecent] = useState<FileListItem[]>([]);
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);

  const loadRecent = useCallback(() => {
    void fileList().then((files) => setRecent(dedupeByPath(files).slice(0, 5)));
  }, []);

  useEffect(() => {
    loadRecent();
  }, [loadRecent, vaultKey]);

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-8 bg-background px-6 py-12">
      <div className="w-full max-w-md rounded-lg border border-border bg-card px-8 py-10 text-center shadow-sm">
        <div className="mb-8 flex justify-center">
          <IrisMark size={56} title="Iris" />
        </div>
        <Button
          type="button"
          className="min-w-[8rem]"
          onClick={() => {
            void (async () => {
              await onNew();
              loadRecent();
            })();
          }}
        >
          新建笔记
        </Button>
      </div>
      {recent.length > 0 && (
        <div className="w-full max-w-md rounded-sm border border-border bg-card p-3 shadow-sm">
          <ul className="space-y-0.5">
            {recent.map((f) => (
              <li
                key={f.path}
                className="group flex items-center rounded-md transition-colors hover:bg-muted"
              >
                <button
                  type="button"
                  className="min-w-0 flex-1 truncate px-2 py-2 text-left text-sm text-foreground"
                  onClick={() => onOpen(f.path)}
                >
                  {f.title}
                </button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="mr-0.5 h-8 w-8 shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100 focus-visible:opacity-100"
                  aria-label={`删除 ${f.title}`}
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
        message={
          deleteTarget
            ? `确定删除「${deleteTarget.title}」？正文、时间线快照与定稿将一并移入回收站，15 天内可恢复。`
            : ""
        }
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
