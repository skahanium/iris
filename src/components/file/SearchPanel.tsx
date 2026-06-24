import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Kbd, OverlayFooterHints } from "@/components/ui/kbd";
import {
  OverlayChrome,
  OverlaySearchHeader,
} from "@/components/ui/overlay-chrome";
import { ScrollArea } from "@/components/ui/scroll-area";
import { searchKeyword, searchSemantic } from "@/lib/ipc";
import type { KeywordHit, SemanticHit } from "@/types/ipc";

interface SearchPanelProps {
  open: boolean;
  onClose: () => void;
  onOpen: (path: string) => void | Promise<void>;
  onPrepare?: (path: string, title?: string) => void;
}

export function SearchPanel({
  open,
  onClose,
  onOpen,
  onPrepare,
}: SearchPanelProps) {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<"keyword" | "semantic">("keyword");
  const [keywordHits, setKeywordHits] = useState<KeywordHit[]>([]);
  const [semanticHits, setSemanticHits] = useState<SemanticHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    keywordHits.forEach((hit) => onPrepare?.(hit.path, hit.title));
    semanticHits.forEach((hit) => onPrepare?.(hit.path, hit.title));
  }, [keywordHits, onPrepare, open, semanticHits]);

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
    <IrisOverlay
      open={open}
      onClose={onClose}
      title="全库搜索"
      size="command"
      showTitleBar={false}
      bodyClassName="overflow-hidden"
    >
      <OverlayChrome
        header={
          <>
            <OverlaySearchHeader
              placeholder="输入关键词或自然语言…"
              value={query}
              inputAriaLabel="全库搜索"
              onChange={setQuery}
              onKeyDown={(e) => e.key === "Enter" && void runSearch()}
              onClose={onClose}
            />
            <div className="task-overlay-filter flex flex-wrap items-center gap-2 px-3 py-2">
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
              {error ? (
                <p className="text-xs text-destructive">{error}</p>
              ) : null}
            </div>
          </>
        }
        footer={
          <OverlayFooterHints
            left={
              <>
                <Kbd active>Enter</Kbd> 搜索
              </>
            }
            right={<Kbd>Esc</Kbd>}
          />
        }
      >
        <ScrollArea className="task-overlay-results min-h-0 flex-1 px-2 py-2">
          {keywordHits.map((h) => (
            <button
              key={h.path}
              type="button"
              className="mb-2 w-full rounded-md border border-border/50 p-2 text-left text-sm transition-colors duration-base ease-iris-out hover:bg-surface-inset/80"
              onMouseEnter={() => onPrepare?.(h.path, h.title)}
              onFocus={() => onPrepare?.(h.path, h.title)}
              onClick={() => {
                void (async () => {
                  try {
                    await onOpen(h.path);
                    onClose();
                  } catch {
                    /* Keep Search visible so the user can retry. */
                  }
                })();
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
              className="mb-2 w-full rounded-md border border-border/50 p-2 text-left text-sm transition-colors duration-base ease-iris-out hover:bg-surface-inset/80"
              onMouseEnter={() => onPrepare?.(h.path, h.title)}
              onFocus={() => onPrepare?.(h.path, h.title)}
              onClick={() => {
                void (async () => {
                  try {
                    await onOpen(h.path);
                    onClose();
                  } catch {
                    /* Keep Search visible so the user can retry. */
                  }
                })();
              }}
            >
              <div className="font-medium">
                {h.title}{" "}
                <span className="text-knowledge-foreground">
                  {(h.score * 100).toFixed(0)}%
                </span>
              </div>
              <div className="text-xs text-muted-foreground">{h.snippet}</div>
            </button>
          ))}
        </ScrollArea>
      </OverlayChrome>
    </IrisOverlay>
  );
}
