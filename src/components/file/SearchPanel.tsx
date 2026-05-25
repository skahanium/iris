import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { searchKeyword, searchSemantic } from "@/lib/ipc";
import type { KeywordHit, SemanticHit } from "@/types/ipc";

interface SearchPanelProps {
  open: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
}

export function SearchPanel({ open, onClose, onOpen }: SearchPanelProps) {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<"keyword" | "semantic">("keyword");
  const [keywordHits, setKeywordHits] = useState<KeywordHit[]>([]);
  const [semanticHits, setSemanticHits] = useState<SemanticHit[]>([]);

  if (!open) return null;

  const runSearch = async () => {
    if (!query.trim()) return;
    if (mode === "keyword") {
      setKeywordHits(await searchKeyword(query, 20));
      setSemanticHits([]);
    } else {
      setSemanticHits(await searchSemantic(query, 5));
      setKeywordHits([]);
    }
  };

  return (
    <div className="fixed inset-y-0 right-0 z-50 flex w-96 flex-col border-l border-border bg-panel shadow-xl">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-medium">搜索</span>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Esc
        </Button>
      </div>
      <div className="space-y-2 border-b border-border p-3">
        <Input
          placeholder="输入关键词或自然语言…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void runSearch()}
        />
        <div className="flex gap-2">
          <Button
            type="button"
            size="sm"
            variant={mode === "keyword" ? "default" : "outline"}
            onClick={() => setMode("keyword")}
          >
            关键词
          </Button>
          <Button
            type="button"
            size="sm"
            variant={mode === "semantic" ? "default" : "outline"}
            onClick={() => setMode("semantic")}
          >
            语义
          </Button>
          <Button type="button" size="sm" onClick={() => void runSearch()}>
            搜索
          </Button>
        </div>
      </div>
      <ScrollArea className="flex-1 px-2">
        {keywordHits.map((h) => (
          <button
            key={h.path}
            type="button"
            className="mb-2 w-full rounded border border-border/50 p-2 text-left text-sm hover:bg-muted"
            onClick={() => {
              onOpen(h.path);
              onClose();
            }}
          >
            <div className="font-medium">{h.title}</div>
            <div className="text-xs text-muted-foreground">{h.path}</div>
            <div
              className="mt-1 text-xs"
              dangerouslySetInnerHTML={{ __html: h.snippet }}
            />
          </button>
        ))}
        {semanticHits.map((h) => (
          <button
            key={`${h.path}-${h.chunk_id}`}
            type="button"
            className="mb-2 w-full rounded border border-border/50 p-2 text-left text-sm hover:bg-muted"
            onClick={() => {
              onOpen(h.path);
              onClose();
            }}
          >
            <div className="font-medium">
              {h.title}{" "}
              <span className="text-primary">{(h.score * 100).toFixed(0)}%</span>
            </div>
            <div className="text-xs text-muted-foreground">{h.snippet}</div>
          </button>
        ))}
      </ScrollArea>
    </div>
  );
}
