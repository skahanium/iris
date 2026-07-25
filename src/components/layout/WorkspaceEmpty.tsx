import { useEffect, useRef, useState } from "react";
import { Search } from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatRelativeTime } from "@/lib/format-relative-time";
import { fileRead } from "@/lib/ipc";
import {
  displayTitleForFileListItem,
  markdownToCardExcerpt,
} from "@/lib/note-display";
import { cn } from "@/lib/utils";
import type { FileListItem } from "@/types/ipc";

export type WorkspaceEmptyMode = "vault" | "workspace";

export interface WorkspaceEmptyProps {
  mode: WorkspaceEmptyMode;
  onNew: () => void | Promise<void>;
  recentNotes?: readonly FileListItem[];
  onOpenNote?: (file: FileListItem) => void | Promise<void>;
  onOpenSearch?: () => void;
  errorMessage?: string | null;
}

const NEW_LABEL: Record<WorkspaceEmptyMode, string> = {
  vault: "新建第一篇",
  workspace: "新建笔记",
};

const LOCKED_PREVIEW = "已锁定";

export function WorkspaceEmpty({
  mode,
  onNew,
  recentNotes = [],
  onOpenNote,
  onOpenSearch,
  errorMessage,
}: WorkspaceEmptyProps) {
  const showRecent =
    mode === "workspace" && recentNotes.length > 0 && onOpenNote;
  const [excerpts, setExcerpts] = useState<Record<string, string>>({});
  const previewGenerationRef = useRef(0);

  useEffect(() => {
    if (!showRecent) {
      setExcerpts((prev) => (Object.keys(prev).length === 0 ? prev : {}));
      return;
    }

    const generation = ++previewGenerationRef.current;
    const paths = recentNotes.map((file) => file.path);

    void (async () => {
      const next: Record<string, string> = {};
      await Promise.all(
        recentNotes.map(async (file) => {
          if (file.isLocked) {
            next[file.path] = LOCKED_PREVIEW;
            return;
          }
          try {
            const result = await fileRead(file.path);
            if (generation !== previewGenerationRef.current) return;
            if (result.isLocked) {
              next[file.path] = LOCKED_PREVIEW;
              return;
            }
            next[file.path] = markdownToCardExcerpt(result.content);
          } catch {
            next[file.path] = "";
          }
        }),
      );
      if (generation !== previewGenerationRef.current) return;
      setExcerpts(() => {
        const merged: Record<string, string> = {};
        for (const path of paths) {
          merged[path] = next[path] ?? "";
        }
        return merged;
      });
    })();
  }, [recentNotes, showRecent]);

  return (
    <div
      data-testid="workspace-empty"
      data-mode={mode}
      className="flex min-h-0 flex-1 flex-col overflow-auto bg-background"
    >
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-6 px-8 py-10 md:px-10 md:py-12">
        <header className="flex flex-wrap items-center justify-end gap-2">
          {onOpenSearch ? (
            <Button
              type="button"
              variant="outline"
              size="default"
              className="gap-2 border-border-subtle text-muted-foreground"
              data-testid="workspace-empty-search"
              onClick={onOpenSearch}
            >
              <Search className="size-4 shrink-0 opacity-70" aria-hidden />
              搜索
            </Button>
          ) : null}
          <Button
            type="button"
            variant="brandOutline"
            data-testid="workspace-empty-new"
            onClick={() => {
              void onNew();
            }}
          >
            {NEW_LABEL[mode]}
          </Button>
        </header>

        {errorMessage ? (
          <p role="status" className="text-sm text-destructive">
            {errorMessage}
          </p>
        ) : null}

        {showRecent ? (
          <div
            className="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-4"
            data-testid="workspace-empty-recent-grid"
          >
            {recentNotes.map((file) => {
              const excerpt = excerpts[file.path];
              return (
                <button
                  key={file.path}
                  type="button"
                  data-testid="workspace-empty-recent-card"
                  data-path={file.path}
                  className={cn(
                    "iris-focus-soft flex min-h-[6.5rem] flex-col gap-2 rounded-lg border border-border-subtle bg-card px-4 py-3.5 text-left",
                    "transition-[background-color,border-color] duration-fast",
                    "hover:border-border hover:bg-[hsl(var(--brand)/0.04)]",
                  )}
                  onClick={() => {
                    void onOpenNote?.(file);
                  }}
                >
                  <span className="line-clamp-2 text-body font-medium leading-snug text-foreground">
                    {displayTitleForFileListItem(file)}
                  </span>
                  {excerpt ? (
                    <span
                      className="line-clamp-2 flex-1 text-sm leading-relaxed text-muted-foreground"
                      data-testid="workspace-empty-recent-excerpt"
                    >
                      {excerpt}
                    </span>
                  ) : (
                    <span className="flex-1" aria-hidden />
                  )}
                  <span className="mt-auto text-caption text-muted-foreground">
                    {formatRelativeTime(file.updatedAt)}
                  </span>
                </button>
              );
            })}
          </div>
        ) : mode === "vault" ? (
          <p className="text-sm text-muted-foreground">还没有笔记</p>
        ) : null}
      </div>
    </div>
  );
}
