import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { fileList } from "@/lib/ipc";
import type { FileListItem } from "@/types/ipc";

interface WelcomeEmptyProps {
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

export function WelcomeEmpty({ onOpen, onNew }: WelcomeEmptyProps) {
  const [recent, setRecent] = useState<FileListItem[]>([]);

  const loadRecent = useCallback(() => {
    void fileList().then((files) =>
      setRecent(dedupeByPath(files).slice(0, 5)),
    );
  }, []);

  useEffect(() => {
    loadRecent();
  }, [loadRecent]);

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-8 bg-background px-6 py-12">
      <div className="w-full max-w-md rounded-sm border border-editor-border/60 bg-editor-paper px-8 py-10 text-center shadow-sm">
        <p className="font-editor text-2xl font-semibold tracking-tight text-editor-ink">
          铺开纸面，开始写
        </p>
        <p className="mt-3 font-sans text-sm leading-relaxed text-editor-muted">
          ⌘/Ctrl+P 打开 · ⌘/Ctrl+Shift+E 文件 · ⌘/Ctrl+Shift+A AI 侧栏
        </p>
        <Button
          type="button"
          className="mt-6 min-w-[8rem]"
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
          <p className="mb-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">
            最近笔记
          </p>
          <p className="mb-2 text-xs text-muted-foreground/80">
            每篇受追踪文档仅显示其最新版本；历史快照与定稿在版本时间线中查看。
          </p>
          <ul className="space-y-0.5">
            {recent.map((f) => (
              <li key={f.path}>
                <button
                  type="button"
                  className="w-full truncate rounded-md px-2 py-2 text-left text-sm text-foreground transition-colors hover:bg-muted"
                  onClick={() => onOpen(f.path)}
                  title={f.path}
                >
                  {f.title}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
