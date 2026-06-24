import { useEffect, useMemo, useState } from "react";

import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { displayTitleForFileListItem } from "@/lib/note-display";
import { cn } from "@/lib/utils";
import { fileBacklinks, tagList } from "@/lib/ipc";
import type { BacklinkEntry, TagGroup } from "@/types/ipc";

interface KnowledgeRelationsPanelProps {
  open: boolean;
  onClose: () => void;
  notePath: string | null;
  onOpen: (path: string) => void | Promise<void>;
  onPreparePath?: (path: string, titleHint?: string) => void;
}

type KnowledgeRelationsTab = "backlinks" | "tags";

export function KnowledgeRelationsPanel({
  open,
  onClose,
  notePath,
  onOpen,
  onPreparePath,
}: KnowledgeRelationsPanelProps) {
  const [activeTab, setActiveTab] =
    useState<KnowledgeRelationsTab>("backlinks");
  const [backlinks, setBacklinks] = useState<BacklinkEntry[]>([]);
  const [tags, setTags] = useState<TagGroup[]>([]);
  const [tagsLoading, setTagsLoading] = useState(false);
  const [tagsError, setTagsError] = useState<string | null>(null);
  const [expandedTag, setExpandedTag] = useState<string | null>(null);
  const [tagSearch, setTagSearch] = useState("");

  useEffect(() => {
    if (!open || !notePath) return;
    void fileBacklinks(notePath).then(setBacklinks);
  }, [open, notePath]);

  useEffect(() => {
    if (!open) return;
    setTagsLoading(true);
    setTagsError(null);
    void tagList()
      .then(setTags)
      .catch((e) =>
        setTagsError(e instanceof Error ? e.message : "加载标签失败"),
      )
      .finally(() => setTagsLoading(false));
  }, [open]);

  const filteredTags = useMemo(
    () =>
      tags.filter((tag) =>
        tag.name.toLowerCase().includes(tagSearch.toLowerCase()),
      ),
    [tags, tagSearch],
  );

  const totalTaggedNotes = useMemo(
    () =>
      new Set(tags.flatMap((tag) => tag.files.map((file) => file.path))).size,
    [tags],
  );

  const openPath = async (path: string) => {
    await onOpen(path);
    onClose();
  };

  return (
    <IrisOverlay open={open} onClose={onClose} title="知识关联" size="command">
      <div
        className="flex min-h-0 flex-1 flex-col"
        data-testid="knowledge-relations-panel"
      >
        <div className="task-overlay-filter flex shrink-0 items-center justify-between gap-3 border-b border-border/60 bg-surface-inset/30 px-4 py-2">
          <div
            className="inline-flex rounded-md border border-border/60 bg-background/45 p-0.5"
            role="tablist"
            aria-label="知识关联"
          >
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "backlinks"}
              data-testid="knowledge-relations-tab-backlinks"
              className={cn(
                "rounded px-2.5 py-1 text-xs transition-colors",
                activeTab === "backlinks"
                  ? "bg-task-selected text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
              onClick={() => setActiveTab("backlinks")}
            >
              反向链接
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "tags"}
              data-testid="knowledge-relations-tab-tags"
              className={cn(
                "rounded px-2.5 py-1 text-xs transition-colors",
                activeTab === "tags"
                  ? "bg-task-selected text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
              onClick={() => setActiveTab("tags")}
            >
              标签
            </button>
          </div>
          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            <span>{backlinks.length} 反链</span>
            <span>{tags.length} 标签</span>
            <span>{totalTaggedNotes} 笔记</span>
          </div>
        </div>

        {activeTab === "tags" ? (
          <div className="border-b border-border/60 px-4 py-2">
            <Input
              className="h-7 text-xs"
              placeholder="过滤标签..."
              value={tagSearch}
              onChange={(e) => setTagSearch(e.target.value)}
            />
          </div>
        ) : null}

        {tagsError ? (
          <p className="px-3 py-2 text-xs text-destructive">{tagsError}</p>
        ) : null}

        <ScrollArea className="task-overlay-results min-h-0 flex-1">
          {activeTab === "backlinks" ? (
            backlinks.length === 0 ? (
              <p className="p-3 text-xs text-muted-foreground">无反向链接</p>
            ) : (
              backlinks.map((backlink) => (
                <button
                  key={backlink.source_path}
                  type="button"
                  className="w-full border-b border-border/50 px-4 py-2.5 text-left text-sm transition-colors duration-base ease-iris-out hover:bg-surface-inset/80"
                  onMouseEnter={() =>
                    onPreparePath?.(backlink.source_path, backlink.source_title)
                  }
                  onFocus={() =>
                    onPreparePath?.(backlink.source_path, backlink.source_title)
                  }
                  onClick={() => void openPath(backlink.source_path)}
                >
                  <div className="font-medium text-primary">
                    {backlink.source_title}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {backlink.source_path}
                  </div>
                  {backlink.context ? (
                    <div className="mt-1 line-clamp-2 text-xs text-muted-foreground/70">
                      {backlink.context}
                    </div>
                  ) : null}
                </button>
              ))
            )
          ) : tagsLoading ? (
            <p className="p-3 text-xs text-muted-foreground">加载中...</p>
          ) : filteredTags.length === 0 ? (
            <p className="p-3 text-xs text-muted-foreground">无标签</p>
          ) : (
            filteredTags.map((tag) => (
              <div key={tag.name}>
                <button
                  type="button"
                  className="flex w-full items-center justify-between border-b border-border/50 px-4 py-2 text-left text-sm transition-colors duration-base ease-iris-out hover:bg-surface-inset/80"
                  onClick={() =>
                    setExpandedTag(expandedTag === tag.name ? null : tag.name)
                  }
                >
                  <span className="text-primary">#{tag.name}</span>
                  <span className="text-xs text-muted-foreground">
                    {tag.files.length}
                  </span>
                </button>
                {expandedTag === tag.name ? (
                  <div className="bg-muted/30">
                    {tag.files.map((file) => (
                      <button
                        key={file.path}
                        type="button"
                        className="w-full border-b border-border/30 px-5 py-1 text-left text-xs text-muted-foreground hover:text-primary"
                        onMouseEnter={() =>
                          onPreparePath?.(
                            file.path,
                            displayTitleForFileListItem(file),
                          )
                        }
                        onFocus={() =>
                          onPreparePath?.(
                            file.path,
                            displayTitleForFileListItem(file),
                          )
                        }
                        onClick={() => void openPath(file.path)}
                      >
                        {displayTitleForFileListItem(file)}
                      </button>
                    ))}
                  </div>
                ) : null}
              </div>
            ))
          )}
        </ScrollArea>
      </div>
    </IrisOverlay>
  );
}
