import { useState } from "react";

import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
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
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runSearch = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    try {
      if (mode === "keyword") {
        setKeywordHits(await searchKeyword(query, 20));
        setSemanticHits([]);
      } else {
        setSemanticHits(await searchSemantic(query, 5));
        setKeywordHits([]);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "搜索失败");
      setKeywordHits([]);
      setSemanticHits([]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <IrisOverlay open={open} onClose={onClose} title="搜索" size="command">
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
          <Button
            type="button"
            size="sm"
            disabled={loading}
            onClick={() => void runSearch()}
          >
            {loading ? "搜索中…" : "搜索"}
          </Button>
        </div>
        {error && <p className="text-xs text-destructive">{error}</p>}
      </div>
      <ScrollArea className="min-h-0 flex-1 px-2">
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
            <div className="mt-1 line-clamp-3 text-xs text-muted-foreground">
              {h.snippet.replace(/<[^>]+>/g, "")}
            </div>
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
              <span className="text-primary">
                {(h.score * 100).toFixed(0)}%
              </span>
            </div>
            <div className="text-xs text-muted-foreground">{h.snippet}</div>
          </button>
        ))}
      </ScrollArea>
    </IrisOverlay>
  );
}
