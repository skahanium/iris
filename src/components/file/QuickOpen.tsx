import { FileImage, FileText, FileVideo } from "lucide-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { CommandListOption } from "@/components/ui/command-list";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Kbd, OverlayFooterHints } from "@/components/ui/kbd";
import {
  OverlayChrome,
  OverlaySearchHeader,
} from "@/components/ui/overlay-chrome";
import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import {
  displayTitleForFileListItem,
  noteListSubtitle,
} from "@/lib/note-display";
import { workspaceList } from "@/lib/ipc";
import { ensureOptionVisible } from "@/lib/command-palette-scroll";
import type { NoteOpenSource } from "@/lib/document-open-runtime";
import type { FileListItem, WorkspaceItem } from "@/types/ipc";

const PREPARE_VISIBLE_LIMIT = 3;

interface QuickOpenProps {
  open: boolean;
  onClose: () => void;
  onPrepare?: (file: FileListItem, source: NoteOpenSource) => void;
  onSelect: (path: string, source: NoteOpenSource) => void | Promise<void>;
}

function noteItem(item: WorkspaceItem): FileListItem | null {
  if (item.kind !== "note") return null;
  return {
    isLocked: item.isLocked,
    path: item.path,
    title: item.title,
    updatedAt: item.updatedAt ?? "",
  };
}

function itemTitle(item: WorkspaceItem): string {
  if (item.kind === "note") {
    return displayTitleForFileListItem({
      isLocked: item.isLocked,
      path: item.path,
      title: item.title,
      updatedAt: item.updatedAt ?? "",
    });
  }
  return item.title || item.path.split("/").pop() || item.path;
}

function itemSubtitle(item: WorkspaceItem): string | undefined {
  if (item.kind === "note") return noteListSubtitle(item.path);
  return item.path;
}

function itemIcon(item: WorkspaceItem) {
  if (item.mediaKind === "image") return FileImage;
  if (item.mediaKind === "video") return FileVideo;
  return FileText;
}

export function QuickOpen({
  open,
  onClose,
  onPrepare,
  onSelect,
}: QuickOpenProps) {
  const [query, setQuery] = useState("");
  const [files, setFiles] = useState<WorkspaceItem[]>([]);
  const parentRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef(new Map<string, HTMLButtonElement>());
  const filteredRef = useRef<WorkspaceItem[]>([]);
  const preparedKeysRef = useRef(new Set<string>());
  const onSelectRef = useRef(onSelect);
  const onCloseRef = useRef(onClose);
  onSelectRef.current = onSelect;
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    preparedKeysRef.current.clear();
    void workspaceList().then(setFiles);
    setQuery("");
  }, [open]);

  const prepareWorkspaceItem = useCallback(
    (file: WorkspaceItem) => {
      const note = noteItem(file);
      if (!note) return;
      const key = note.path + "\0" + note.updatedAt;
      if (preparedKeysRef.current.has(key)) return;
      preparedKeysRef.current.add(key);
      onPrepare?.(note, "quick-open");
    },
    [onPrepare],
  );

  const filtered = useMemo(() => {
    const normalizedQuery = query.toLowerCase();
    return files.filter((f) => {
      const label = itemTitle(f);
      return (
        label.toLowerCase().includes(normalizedQuery) ||
        f.path.toLowerCase().includes(normalizedQuery)
      );
    });
  }, [files, query]);
  filteredRef.current = filtered;

  const visibleFiles = useMemo(
    () => filtered.slice(0, PREPARE_VISIBLE_LIMIT),
    [filtered],
  );

  useEffect(() => {
    if (!open) return;
    visibleFiles.forEach(prepareWorkspaceItem);
  }, [open, prepareWorkspaceItem, visibleFiles]);

  const activateItem = useCallback((item: WorkspaceItem) => {
    onCloseRef.current();
    void Promise.resolve(onSelectRef.current(item.path, "quick-open")).catch(
      () => undefined,
    );
  }, []);

  const { highlight, setHighlight, handleKeyDown, navDeltaRef } =
    useListboxKeyboard({
      length: filtered.length,
      enabled: open,
      resetKey: open ? query : "__closed__",
      onActivate: (index) => {
        const item = filteredRef.current[index];
        if (!item) return;
        activateItem(item);
      },
    });

  useEffect(() => {
    if (!open) return;
    const item = filtered[highlight];
    if (item) prepareWorkspaceItem(item);
  }, [filtered, highlight, open, prepareWorkspaceItem]);

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 56,
    overscan: 10,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const renderedItems =
    virtualItems.length > 0
      ? virtualItems
      : filtered.map((_, index) => ({
          index,
          key: filtered[index]!.path,
          size: 56,
          start: index * 56,
        }));
  const listHeight =
    virtualItems.length > 0 ? virtualizer.getTotalSize() : filtered.length * 56;

  const scrollHighlightIntoView = useCallback(() => {
    const item = filteredRef.current[highlight];
    if (!item || !parentRef.current) return;
    const el = itemRefs.current.get(item.path);
    if (!el) return;
    ensureOptionVisible(parentRef.current, el, navDeltaRef.current);
    navDeltaRef.current = 0;
  }, [highlight, navDeltaRef]);

  useLayoutEffect(() => {
    if (!open) return;
    scrollHighlightIntoView();
  }, [open, highlight, scrollHighlightIntoView]);

  return (
    <IrisOverlay
      open={open}
      onClose={onClose}
      title="搜索工作区"
      size="compact"
      showTitleBar={false}
      bodyClassName="overflow-hidden"
    >
      <OverlayChrome
        header={
          <OverlaySearchHeader
            placeholder="搜索工作区…"
            value={query}
            inputAriaLabel="搜索工作区"
            onChange={setQuery}
            onKeyDown={handleKeyDown}
            onClose={onClose}
          />
        }
        footer={
          <OverlayFooterHints
            left={
              <>
                {filtered.length} 条结果 · <Kbd active>↑</Kbd>{" "}
                <Kbd active>↓</Kbd> <Kbd active>Enter</Kbd> 打开
              </>
            }
            right={<Kbd>Esc</Kbd>}
          />
        }
      >
        <div
          ref={parentRef}
          className="task-overlay-results max-h-[min(24rem,52vh)] overflow-auto"
        >
          {filtered.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-6 py-12 text-center">
              <FileText className="h-8 w-8 text-muted-foreground/60" />
              <p className="text-sm text-muted-foreground">无匹配项目</p>
            </div>
          ) : (
            <div
              style={{
                height: `${listHeight}px`,
                position: "relative",
              }}
              role="listbox"
              aria-label="工作区项目列表"
            >
              {renderedItems.map((virtualItem) => {
                const f = filtered[virtualItem.index]!;
                const active = virtualItem.index === highlight;
                return (
                  <div
                    key={f.path}
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      width: "100%",
                      transform: `translateY(${virtualItem.start}px)`,
                    }}
                  >
                    <CommandListOption
                      id={`quick-open-${f.path}`}
                      label={itemTitle(f)}
                      query={query}
                      subtitle={itemSubtitle(f)}
                      active={active}
                      icon={itemIcon(f)}
                      buttonRef={(el) => {
                        if (el) itemRefs.current.set(f.path, el);
                        else itemRefs.current.delete(f.path);
                      }}
                      onMouseEnter={() => {
                        navDeltaRef.current = 0;
                        setHighlight(virtualItem.index);
                        prepareWorkspaceItem(f);
                      }}
                      onSelect={() => {
                        activateItem(f);
                      }}
                    />
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </OverlayChrome>
    </IrisOverlay>
  );
}
