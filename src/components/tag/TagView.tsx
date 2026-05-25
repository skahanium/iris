import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { tagList } from "@/lib/ipc";
import type { TagGroup } from "@/types/ipc";

interface TagViewProps {
  open: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
}

export function TagView({ open, onClose, onOpen }: TagViewProps) {
  const [tags, setTags] = useState<TagGroup[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    setError(null);
    void tagList()
      .then(setTags)
      .catch((e) => setError(e instanceof Error ? e.message : "加载标签失败"))
      .finally(() => setLoading(false));
  }, [open]);

  if (!open) return null;

  const filtered = tags.filter((t) =>
    t.name.toLowerCase().includes(search.toLowerCase()),
  );

  const totalNotes = new Set(tags.flatMap((t) => t.files.map((f) => f.path)))
    .size;

  return (
    <div className="fixed inset-y-0 right-0 z-50 flex w-72 flex-col border-l border-border bg-panel shadow-xl">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-medium">标签</span>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Esc
        </Button>
      </div>

      <div className="flex gap-3 border-b border-border px-3 py-2 text-xs text-muted-foreground">
        <span>{totalNotes} 笔记</span>
        <span>{tags.length} 标签</span>
      </div>

      <div className="border-b border-border px-2 py-1.5">
        <Input
          className="h-7 text-xs"
          placeholder="过滤标签…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      {error && <p className="px-3 py-2 text-xs text-red-400/90">{error}</p>}

      <ScrollArea className="flex-1">
        {loading ? (
          <p className="p-3 text-xs text-muted-foreground">加载中…</p>
        ) : filtered.length === 0 ? (
          <p className="p-3 text-xs text-muted-foreground">无标签</p>
        ) : (
          filtered.map((t) => (
            <div key={t.name}>
              <button
                type="button"
                className="flex w-full items-center justify-between border-b border-border/50 px-3 py-2 text-left text-sm hover:bg-muted"
                onClick={() => setExpanded(expanded === t.name ? null : t.name)}
              >
                <span className="text-primary">#{t.name}</span>
                <span className="text-xs text-muted-foreground">
                  {t.files.length}
                </span>
              </button>
              {expanded === t.name && (
                <div className="bg-muted/30">
                  {t.files.map((f) => (
                    <button
                      key={f.path}
                      type="button"
                      className="w-full border-b border-border/30 px-5 py-1 text-left text-xs text-muted-foreground hover:text-primary"
                      onClick={() => {
                        onOpen(f.path);
                        onClose();
                      }}
                    >
                      {f.title}
                    </button>
                  ))}
                </div>
              )}
            </div>
          ))
        )}
      </ScrollArea>
    </div>
  );
}
