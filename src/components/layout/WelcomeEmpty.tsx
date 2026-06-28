import { Bot, FilePlus2, FolderSearch, Search, Trash2 } from "lucide-react";
import { memo, useState } from "react";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { displayTitleForFileListItem } from "@/lib/note-display";
import { fileDelete } from "@/lib/ipc";
import type { HomePendingOpen } from "@/lib/home-open-transition";
import type { NoteOpenSource } from "@/lib/document-open-runtime";
import type { FileListItem } from "@/types/ipc";

interface WelcomeEmptyProps {
  onOpen: (
    path: string,
    titleHint: string | undefined,
    source: NoteOpenSource,
  ) => void;
  onPrepare?: (file: FileListItem, source: NoteOpenSource) => void;
  onNew: () => void | Promise<void>;
  onQuickOpen?: () => void;
  onRefreshRecent: () => void | Promise<void>;
  onSearch?: () => void;
  onOpenAiManagement?: () => void;
  pendingOpen?: HomePendingOpen | null;
  recentNotes: readonly FileListItem[];
}

export const WelcomeEmpty = memo(function WelcomeEmpty({
  onOpen,
  onNew,
  onPrepare,
  onQuickOpen,
  onRefreshRecent,
  onSearch,
  onOpenAiManagement,
  pendingOpen,
  recentNotes,
}: WelcomeEmptyProps) {
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);

  return (
    <div
      data-testid="home-workbench"
      className="flex flex-1 items-center justify-center bg-background px-6 py-10"
    >
      <div className="home-workbench-grid grid w-full max-w-5xl grid-cols-1 gap-10 lg:grid-cols-[minmax(18rem,0.88fr)_minmax(25rem,1.42fr)]">
        <section className="min-w-0 border-r-0 border-border/70 pr-0 lg:border-r lg:pr-8">
          <div data-testid="home-quick-actions" className="grid gap-5">
            <Button
              type="button"
              className="h-11 justify-start gap-2"
              onClick={() => {
                void (async () => {
                  await onNew();
                  await onRefreshRecent();
                })();
              }}
            >
              <FilePlus2 className="h-4 w-4" />
              新建笔记
            </Button>
            <div className="grid grid-cols-1 gap-5 sm:grid-cols-3 lg:grid-cols-1">
              {onQuickOpen ? (
                <Button
                  type="button"
                  variant="outline"
                  className="justify-start gap-2 border-border/70 bg-transparent"
                  onClick={onQuickOpen}
                >
                  <FolderSearch className="h-4 w-4" />
                  快速打开
                </Button>
              ) : null}
              {onSearch ? (
                <Button
                  type="button"
                  variant="outline"
                  className="justify-start gap-2 border-border/70 bg-transparent"
                  onClick={onSearch}
                >
                  <Search className="h-4 w-4" />
                  全库搜索
                </Button>
              ) : null}
              {onOpenAiManagement ? (
                <Button
                  type="button"
                  variant="outline"
                  className="justify-start gap-2 border-border/70 bg-transparent"
                  onClick={onOpenAiManagement}
                >
                  <Bot className="h-4 w-4" />
                  AI 管理
                </Button>
              ) : null}
            </div>
          </div>
        </section>

        <section className="min-w-0">
          <div className="mb-3 flex items-center justify-between border-b border-border/60 pb-2">
            <div>
              <h2 className="text-sm font-medium text-foreground">最近笔记</h2>
              <p className="mt-1 text-xs text-muted-foreground">
                从上一次中断的地方继续
              </p>
            </div>
            <span className="text-xs tabular-nums text-muted-foreground">
              {recentNotes.length}
            </span>
          </div>
          {recentNotes.length > 0 ? (
            <ul className="divide-y divide-border/50">
              {recentNotes.map((f) => {
                const title = displayTitleForFileListItem(f);
                return (
                  <li
                    key={f.path}
                    className="group flex items-center transition-colors duration-base ease-iris-out hover:bg-surface-inset/60"
                    onMouseEnter={() => onPrepare?.(f, "welcome")}
                  >
                    <button
                      type="button"
                      className="min-w-0 flex-1 px-2 py-3 text-left"
                      onFocus={() => onPrepare?.(f, "welcome")}
                      onClick={() => onOpen(f.path, title, "welcome")}
                    >
                      <span className="block truncate text-sm text-foreground">
                        {title}
                      </span>
                      {pendingOpen?.path === f.path && pendingOpen.error ? (
                        <span className="mt-1 block truncate text-xs text-destructive">
                          {pendingOpen.error}
                        </span>
                      ) : null}
                    </button>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      className="mr-1 h-8 w-8 shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
                      aria-label={`删除 ${title}`}
                      onClick={() => setDeleteTarget(f)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </li>
                );
              })}
            </ul>
          ) : (
            <div className="border border-dashed border-border/70 px-4 py-8 text-sm text-muted-foreground">
              暂无最近笔记。新建第一篇后，这里会成为你的继续工作入口。
            </div>
          )}
        </section>
      </div>

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
            await onRefreshRecent();
          })();
        }}
      />
    </div>
  );
});
