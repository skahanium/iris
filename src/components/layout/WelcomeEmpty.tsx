import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { fileList } from "@/lib/ipc";
import type { FileListItem } from "@/types/ipc";

interface WelcomeEmptyProps {
  onOpen: (path: string) => void;
  onNew: () => void;
}

export function WelcomeEmpty({ onOpen, onNew }: WelcomeEmptyProps) {
  const [recent, setRecent] = useState<FileListItem[]>([]);

  useEffect(() => {
    void fileList().then((files) => setRecent(files.slice(0, 5)));
  }, []);

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-6 font-sans text-editor-muted">
      <div className="text-center">
        <p className="font-editor text-lg text-editor-ink/80">
          铺开纸面，开始写
        </p>
        <p className="mt-2 text-sm">
          ⌘/Ctrl+P 打开 · ⌘/Ctrl+Shift+E 文件 · ⌘/Ctrl+Shift+A AI 侧栏
        </p>
      </div>
      <Button type="button" onClick={onNew}>
        新建笔记
      </Button>
      {recent.length > 0 && (
        <div className="w-full max-w-sm">
          <p className="mb-2 text-xs font-medium text-muted-foreground">
            最近笔记
          </p>
          <ul className="space-y-1">
            {recent.map((f) => (
              <li key={f.path}>
                <button
                  type="button"
                  className="w-full truncate rounded px-2 py-1.5 text-left text-sm text-editor-ink/90 hover:bg-editor-border/40"
                  onClick={() => onOpen(f.path)}
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
