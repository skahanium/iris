import { Bookmark, RotateCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  versionDelete,
  versionFinalize,
  versionList,
  versionPreview,
  versionRestore,
} from "@/lib/ipc";
import type { VersionEntry } from "@/types/ipc";

interface VersionTimelineProps {
  open: boolean;
  onClose: () => void;
  notePath: string | null;
  currentContent: string;
  onRestore: (content: string) => void;
}

export function VersionTimeline({
  open,
  onClose,
  notePath,
  currentContent,
  onRestore,
}: VersionTimelineProps) {
  const [versions, setVersions] = useState<VersionEntry[]>([]);
  const [preview, setPreview] = useState<string | null>(null);
  const [previewId, setPreviewId] = useState<number | null>(null);
  const [label, setLabel] = useState("");
  const [finalizing, setFinalizing] = useState<number | null>(null);

  const refresh = useCallback(() => {
    if (!notePath) return;
    void versionList(notePath).then(setVersions);
  }, [notePath]);

  useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);

  if (!open) return null;

  const handlePreview = async (id: number) => {
    const content = await versionPreview(id);
    setPreview(content);
    setPreviewId(id);
  };

  const handleRestore = async (id: number) => {
    const result = await versionRestore(id, currentContent);
    onRestore(result.content);
    refresh();
    setPreview(null);
    setPreviewId(null);
  };

  const handleDelete = async (id: number) => {
    await versionDelete(id);
    refresh();
    if (previewId === id) {
      setPreview(null);
      setPreviewId(null);
    }
  };

  const handleFinalize = async (id: number) => {
    await versionFinalize(id, label || null);
    setFinalizing(null);
    setLabel("");
    refresh();
  };

  const formatTime = (ts: string) => {
    // version_no is like 20260525143052000
    if (ts.length >= 14) {
      return `${ts.slice(0, 4)}-${ts.slice(4, 6)}-${ts.slice(6, 8)} ${ts.slice(8, 10)}:${ts.slice(10, 12)}:${ts.slice(12, 14)}`;
    }
    return ts;
  };

  return (
    <div className="fixed inset-y-0 right-0 z-50 flex w-80 flex-col border-l border-border bg-panel shadow-xl">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-medium">版本历史</span>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Esc
        </Button>
      </div>

      <ScrollArea className="flex-1">
        {versions.length === 0 ? (
          <p className="p-3 text-xs text-muted-foreground">暂无版本快照</p>
        ) : (
          versions.map((v) => (
            <div
              key={v.id}
              className={`border-b border-border/50 px-3 py-2.5 text-sm ${
                previewId === v.id ? "bg-muted/50" : ""
              }`}
            >
              <button
                type="button"
                className="w-full text-left"
                onClick={() => handlePreview(v.id)}
              >
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">
                    {formatTime(v.version_no)}
                  </span>
                  {v.is_finalized && (
                    <span className="rounded bg-primary/20 px-1 py-0 text-[10px] text-primary">
                      定稿
                    </span>
                  )}
                  {v.label && (
                    <span className="text-xs text-muted-foreground">
                      {v.label}
                    </span>
                  )}
                </div>
                <div className="mt-0.5 text-xs text-muted-foreground/70">
                  {v.word_count.toLocaleString()} 字
                </div>
              </button>

              {previewId === v.id && preview !== null && (
                <div className="mt-2 rounded border border-border bg-muted/30 p-2 text-xs">
                  <pre className="whitespace-pre-wrap font-mono max-h-40 overflow-auto">
                    {preview.slice(0, 1000)}
                    {preview.length > 1000 && "…"}
                  </pre>
                  <div className="mt-2 flex gap-1">
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => handleRestore(v.id)}
                    >
                      <RotateCw className="mr-1 h-3 w-3" />
                      恢复
                    </Button>
                    {!v.is_finalized && finalizing === v.id ? (
                      <div className="flex items-center gap-1">
                        <Input
                          className="h-6 w-20 text-[10px]"
                          placeholder="版本名"
                          value={label}
                          onChange={(e) => setLabel(e.target.value)}
                        />
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          onClick={() => handleFinalize(v.id)}
                        >
                          <Bookmark className="h-3 w-3" />
                        </Button>
                      </div>
                    ) : (
                      !v.is_finalized && (
                        <Button
                          type="button"
                          size="sm"
                          variant="ghost"
                          onClick={() => setFinalizing(v.id)}
                        >
                          <Bookmark className="h-3 w-3" />
                        </Button>
                      )
                    )}
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => handleDelete(v.id)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              )}
            </div>
          ))
        )}
      </ScrollArea>
    </div>
  );
}
