//! Continuous semantic Markdown with source-offset identities and measured block windows.
import { WindowedMarkdownBlock } from "./WindowedMarkdownBlock";
import {
  defaultRangeExtractor,
  observeElementOffset,
  useVirtualizer,
} from "@tanstack/react-virtual";
import {
  memo,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import {
  updateStreamingMarkdown,
  type StreamingMarkdownBlock,
  type StreamingMarkdownDocument,
} from "@/lib/streaming-markdown-splitter";

export interface StreamingMessageBodyProps {
  content: string;
  contentIdentity?: string;
  streaming?: boolean;
  className?: string;
  dataProseSurface?: string;
  onClick?: (event: ReactMouseEvent<HTMLDivElement>) => void;
}

const MarkdownBlock = memo(function MarkdownBlock({
  block,
  definitions,
  active,
}: {
  block: StreamingMarkdownBlock;
  definitions: string;
  active: boolean;
}) {
  return (
    <div
      className="ai-markdown-block"
      data-markdown-block={block.start}
      data-source-end={block.end}
    >
      <WindowedMarkdownBlock
        block={block}
        definitions={definitions}
        active={active}
      />
    </div>
  );
});

export function StreamingMessageBody({
  content,
  contentIdentity,
  streaming = true,
  className,
  dataProseSurface,
  onClick,
}: StreamingMessageBodyProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  // marked normalizes line endings before lexing; source offsets must use the
  // same representation. The authoritative message/clipboard keeps its source.
  const markdownSource = useMemo(
    () => content.replace(/\r\n?/g, "\n"),
    [content],
  );
  const [document, setDocument] = useState<StreamingMarkdownDocument>(() =>
    updateStreamingMarkdown(markdownSource),
  );
  const [identity, setIdentity] = useState(contentIdentity);
  let current = document;
  if (document.source !== markdownSource || identity !== contentIdentity) {
    current = updateStreamingMarkdown(
      markdownSource,
      identity === contentIdentity ? document : undefined,
    );
    setDocument(current);
    setIdentity(contentIdentity);
  }
  const blocks = current.blocks;
  const blocksRef = useRef(blocks);
  blocksRef.current = blocks;
  const windowed = blocks.length > 60;
  const [pinned, setPinned] = useState<number[]>([]);
  const [scrollMargin, setScrollMargin] = useState(0);
  const virtualizer = useVirtualizer({
    count: blocks.length,
    getScrollElement: () =>
      containerRef.current?.closest<HTMLElement>(
        "[data-radix-scroll-area-viewport]",
      ) ?? null,
    initialOffset: () =>
      containerRef.current?.closest<HTMLElement>(
        "[data-radix-scroll-area-viewport]",
      )?.scrollTop ?? 0,
    // Activation also asks TanStack to scroll; only the reading anchor may write.
    scrollToFn: () => undefined,
    observeElementOffset: (instance, callback) => {
      // Refs were empty during render; seed the mounted viewport before events.
      callback(instance.scrollElement?.scrollTop ?? 0, false);
      return observeElementOffset(instance, callback);
    },
    estimateSize: (index) =>
      Math.max(36, Math.ceil((blocks[index]?.source.length ?? 0) / 75) * 24),
    getItemKey: (index) => blocks[index]?.start ?? index,
    measureElement: (element, entry) =>
      (entry?.borderBoxSize?.[0]?.blockSize ??
        element.getBoundingClientRect().height) ||
      Math.max(
        36,
        Math.ceil(
          (blocks[Number(element.getAttribute("data-index"))]?.source.length ??
            0) / 75,
        ) * 24,
      ),
    overscan: 4,
    scrollMargin,
    enabled: windowed,
    initialRect: { width: 700, height: 800 },
    rangeExtractor: (range) =>
      [
        ...new Set([
          ...defaultRangeExtractor(range),
          ...pinned,
          ...(streaming ? [blocks.length - 1] : []),
        ]),
      ]
        .filter((index) => index >= 0 && index < blocks.length)
        .sort((a, b) => a - b),
  });
  virtualizer.shouldAdjustScrollPositionOnItemSizeChange = () => false;
  useLayoutEffect(() => {
    const element = containerRef.current;
    const viewport = element?.closest<HTMLElement>(
      "[data-radix-scroll-area-viewport]",
    );
    const refreshMargin = () => {
      if (element && viewport)
        setScrollMargin(
          element.getBoundingClientRect().top -
            viewport.getBoundingClientRect().top +
            viewport.scrollTop,
        );
    };
    refreshMargin();
    viewport?.addEventListener("iris-conversation-geometry", refreshMargin);
    const preserveSelection = () => {
      const selection = window.getSelection();
      const next = blocksRef.current.flatMap((block, index) => {
        const node = element?.querySelector(
          `[data-markdown-block="${block.start}"]`,
        );
        return node &&
          (node.contains(globalThis.document.activeElement) ||
            (selection &&
              selection.rangeCount > 0 &&
              !selection.isCollapsed &&
              selection.containsNode(node, true)))
          ? [index]
          : [];
      });
      setPinned((previous) =>
        previous.join() === next.join() ? previous : next,
      );
    };
    globalThis.document.addEventListener("selectionchange", preserveSelection);
    element?.addEventListener("focusin", preserveSelection);
    element?.addEventListener("focusout", preserveSelection);
    return () => {
      viewport?.removeEventListener(
        "iris-conversation-geometry",
        refreshMargin,
      );
      globalThis.document.removeEventListener(
        "selectionchange",
        preserveSelection,
      );
      element?.removeEventListener("focusin", preserveSelection);
      element?.removeEventListener("focusout", preserveSelection);
    };
  }, []);
  const measured = virtualizer.getVirtualItems();
  const fallback = [
    ...new Set([
      ...Array.from(
        { length: Math.min(12, blocks.length) },
        (_, index) => index,
      ),
      ...(streaming ? [blocks.length - 1] : []),
    ]),
  ];
  const items =
    windowed && measured.length === 0
      ? fallback
          .map((index) => virtualizer.measurementsCache[index]!)
          .filter(Boolean)
      : measured;
  const visible = useMemo(
    () => (windowed ? new Map(items.map((item) => [item.index, item])) : null),
    [items, windowed],
  );
  let previousEnd = 0;
  const rows = (
    windowed ? items.map((item) => item.index) : blocks.map((_, index) => index)
  ).map((index) => {
    const block = blocks[index]!;
    const item = visible?.get(index);
    const gap = item ? Math.max(0, item.start - scrollMargin - previousEnd) : 0;
    previousEnd = item ? item.end - scrollMargin : 0;
    return (
      <div key={`${contentIdentity ?? ""}:${block.start}`}>
        {gap > 0 && (
          <div
            className="ai-markdown-window-spacer"
            aria-hidden
            style={{ height: gap }}
          />
        )}
        <div
          data-index={index}
          ref={windowed ? virtualizer.measureElement : undefined}
        >
          <MarkdownBlock
            block={block}
            definitions={current.definitions}
            active={streaming && !block.confirmed}
          />
        </div>
      </div>
    );
  });
  return (
    <div
      ref={containerRef}
      className={className}
      data-prose-surface={dataProseSurface}
      data-content-identity={contentIdentity}
      data-ai-streaming-markdown
      onClick={onClick}
    >
      {rows}
      {windowed && (
        <div
          className="ai-markdown-window-spacer"
          aria-hidden
          style={{
            height: Math.max(0, virtualizer.getTotalSize() - previousEnd),
          }}
        />
      )}
    </div>
  );
}
