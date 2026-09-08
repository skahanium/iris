//! Row windows retain full source while bounding mounted code, table and long-text rows.
import {
  defaultRangeExtractor,
  observeElementOffset,
  useVirtualizer,
} from "@tanstack/react-virtual";
import {
  Fragment,
  memo,
  useLayoutEffect,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import { toTrustedHtml } from "@/lib/sanitize";
import { useStreamingLineBudgetRef } from "@/lib/streaming-line-budget-ref";
import { readTailLineBudget } from "@/lib/streaming-line-fit";
import {
  reconcileMarkdownChildren,
  type StreamingMarkdownBlock,
} from "@/lib/streaming-markdown-splitter";

interface WindowedMarkdownBlockProps {
  block: StreamingMarkdownBlock;
  definitions: string;
  active: boolean;
}
interface SourceRow {
  start: number;
  source: string;
  html?: string;
}

const rowSegmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

function sourceRows(source: string, table: boolean): SourceRow[] {
  const rows: SourceRow[] = [];
  const segments = table ? null : rowSegmenter.segment(source);
  let start = 0;
  while (start < source.length) {
    let end = Math.min(source.length, start + 2048);
    const line = source.lastIndexOf("\n", end);
    if (line > start) end = line + 1;
    else if (table) {
      const nextLine = source.indexOf("\n", end);
      end = nextLine < 0 ? source.length : nextLine + 1;
    } else if (end < source.length) {
      const segment = segments?.containing(end - 1);
      if (segment) end = segment.index + segment.segment.length;
    }
    rows.push({ start, source: source.slice(start, end) });
    start = end;
  }
  return rows;
}

/** Split one sanitized tree, cloning ancestor marks instead of cutting inline syntax. */
function semanticRows(source: string, definitions: string): SourceRow[] {
  const template = document.createElement("template");
  template.innerHTML = toTrustedHtml(
    renderMarkdownWithProfile(`${source}\n\n${definitions}`, "chat_assistant")
      .output,
  ) as string;
  const rows: SourceRow[] = [];
  let container = document.createElement("div");
  let length = 0;
  let offset = 0;
  let ancestorsBySource = new Map<Element, Element>();
  const flush = () => {
    if (!container.hasChildNodes()) return;
    rows.push({
      start: offset,
      source: container.textContent ?? "",
      html: container.innerHTML,
    });
    offset += length;
    container = document.createElement("div");
    length = 0;
    ancestorsBySource = new Map();
  };
  const append = (node: Node, ancestors: Element[]) => {
    let parent: Node = container;
    for (const ancestor of ancestors) {
      let clone = ancestorsBySource.get(ancestor);
      if (!clone) {
        clone = ancestor.cloneNode(false) as Element;
        parent.appendChild(clone);
        ancestorsBySource.set(ancestor, clone);
      }
      parent = clone;
    }
    parent.appendChild(node);
  };
  const walk = (node: Node, ancestors: Element[]) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent ?? "";
      const segments = rowSegmenter.segment(text);
      for (let start = 0; start < text.length; ) {
        if (length >= 2048) flush();
        let end = Math.min(text.length, start + 2048 - length);
        if (end < text.length) {
          const segment = segments.containing(end - 1);
          if (segment) end = segment.index + segment.segment.length;
        }
        append(document.createTextNode(text.slice(start, end)), ancestors);
        length += end - start;
        start = end;
      }
    } else if (node instanceof Element) {
      if (!node.hasChildNodes()) append(node.cloneNode(false), ancestors);
      else
        for (const child of Array.from(node.childNodes))
          walk(child, [...ancestors, node]);
    }
  };
  for (const child of Array.from(template.content.childNodes)) walk(child, []);
  flush();
  return rows;
}

interface ProseRowsCache {
  source: string;
  definitions: string;
  rows: SourceRow[];
  plainPrefix: string | null;
}

// Deliberately conservative: punctuation that can form links, inline marks,
// entities or a new block must go through the authoritative complete parser.
const plainProse = /^[\p{L}\p{N}\p{M} ，。！？、；：“”‘’（）…—]+$/u;

function proseRows(
  source: string,
  definitions: string,
  previous: ProseRowsCache | null,
): ProseRowsCache {
  if (previous?.source === source && previous.definitions === definitions)
    return previous;
  const appending = previous && source.startsWith(previous.source);
  const prefix = /^(?:#{1,6} |- )/.exec(source)?.[0] ?? "";
  const plain = appending
    ? previous.plainPrefix !== null &&
      plainProse.test(source.slice(previous.source.length))
    : source[prefix.length] !== " " &&
      plainProse.test(source.slice(prefix.length));
  if (!plain)
    return {
      source,
      definitions,
      rows: semanticRows(source, definitions),
      plainPrefix: null,
    };

  // Only a plain single-line paragraph/heading/list item can use this path.
  // Keep the parser's structural template and change only its plain text leaf.
  // Independently parsing slices would turn four boundary spaces into code.
  // A syntax change invalidates this proof and the cache.
  const reusable =
    appending && previous.definitions === definitions
      ? previous.rows.slice(0, -1)
      : [];
  const start = reusable.length
    ? reusable[reusable.length - 1]!.start +
      reusable[reusable.length - 1]!.source.length
    : 0;
  const template = document.createElement("template");
  template.innerHTML = toTrustedHtml(
    appending && previous.rows[0]?.html
      ? previous.rows[0].html
      : renderMarkdownWithProfile(`${prefix}text`, "chat_assistant").output,
  ) as string;
  const leaf = template.content.querySelector("p,li,h1,h2,h3,h4,h5,h6");
  if (!leaf)
    return {
      source,
      definitions,
      rows: semanticRows(source, definitions),
      plainPrefix: null,
    };
  const rows = sourceRows(source.slice(prefix.length + start), false).map(
    (row) => {
      leaf.textContent = row.source;
      return {
        start: row.start + start,
        source: row.source,
        html: template.innerHTML,
      };
    },
  );
  return {
    source,
    definitions,
    rows: [...reusable, ...rows],
    plainPrefix: prefix,
  };
}

function tablePart(source: string, part: "thead" | "tbody"): string {
  const html = renderMarkdownWithProfile(source, "chat_assistant").output;
  const template = document.createElement("template");
  template.innerHTML = toTrustedHtml(html) as string;
  return template.content.querySelector(part)?.innerHTML ?? "";
}

const MarkdownRows = memo(function MarkdownRows({
  source,
  header,
  definitions,
  table,
  active,
  providedHtml,
}: {
  source: string;
  header: string;
  definitions: string;
  table: boolean;
  active: boolean;
  providedHtml?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const html = useMemo(
    () =>
      providedHtml ??
      (table
        ? tablePart(`${header}${source}\n\n${definitions}`, "tbody")
        : renderMarkdownWithProfile(
            `${source}\n\n${definitions}`,
            "chat_assistant",
            { streaming: active },
          ).output),
    [source, header, definitions, table, active, providedHtml],
  );
  useLayoutEffect(() => {
    if (!ref.current) return;
    const template = document.createElement("template");
    template.innerHTML = toTrustedHtml(html) as string;
    reconcileMarkdownChildren(ref.current, template.content);
  }, [html]);
  return table ? (
    <tbody dangerouslySetInnerHTML={{ __html: toTrustedHtml(html) }} />
  ) : (
    <div ref={ref} />
  );
});

export function WindowedMarkdownBlock({
  block,
  definitions,
  active,
}: WindowedMarkdownBlockProps) {
  const ref = useRef<HTMLDivElement>(null);
  const budgetRef = useStreamingLineBudgetRef();
  const [margin, setMargin] = useState(0);
  const [pinned, setPinned] = useState<number[]>([]);
  const code = block.type === "code";
  const table = block.type === "table";
  const language = code
    ? (/^ {0,3}(?:`{3,}|~{3,})([^\s`~]*)/.exec(block.source)?.[1] ?? "")
    : "";
  const source = useMemo(() => {
    if (!code) return block.source;
    const opening = /^ {0,3}(`{3,}|~{3,})[^\n]*\n/.exec(block.source);
    if (!opening)
      return /^ {0,3}(`{3,}|~{3,})[^\n]*$/.test(block.source)
        ? ""
        : block.source.replace(/^ {4}/gm, "");
    const fence = opening[1]!;
    return block.source
      .slice(opening[0].length)
      .replace(
        new RegExp(
          `(?:^|\\n) {0,3}${fence[0]}{${fence.length},}[ \\t]*(?:\\n)*$`,
        ),
        "",
      );
  }, [block.source, code]);
  const headerEnd = table
    ? source.indexOf("\n", source.indexOf("\n") + 1) + 1
    : 0;
  const header = table ? source.slice(0, headerEnd || source.length) : "";
  const headerHtml = useMemo(
    () => (table ? tablePart(`${header}\n\n${definitions}`, "thead") : ""),
    [header, table, definitions],
  );
  const bodySource = table ? source.slice(headerEnd || source.length) : source;
  const windowed = source.length > 8192;
  const proseCache = useRef<ProseRowsCache | null>(null);
  const rows = useMemo(() => {
    if (!windowed) {
      proseCache.current = null;
      return [{ start: 0, source: bodySource }];
    }
    if (code || table) return sourceRows(bodySource, table);
    proseCache.current = proseRows(bodySource, definitions, proseCache.current);
    return proseCache.current.rows;
  }, [bodySource, windowed, table, code, definitions]);
  const virtualizer = useVirtualizer({
    count: rows.length,
    enabled: windowed,
    getScrollElement: () =>
      ref.current?.closest<HTMLElement>("[data-radix-scroll-area-viewport]") ??
      null,
    initialOffset: () =>
      ref.current?.closest<HTMLElement>("[data-radix-scroll-area-viewport]")
        ?.scrollTop ?? 0,
    // Activation also asks TanStack to scroll; only the reading anchor may write.
    scrollToFn: () => undefined,
    observeElementOffset: (instance, callback) => {
      // Refs were empty during render; seed the mounted viewport before events.
      callback(instance.scrollElement?.scrollTop ?? 0, false);
      return observeElementOffset(instance, callback);
    },
    estimateSize: (index) =>
      Math.max(
        24,
        (rows[index]?.source.split("\n").length ?? 1) * 24,
        Math.ceil((rows[index]?.source.length ?? 0) / 80) * 24,
      ),
    measureElement: (element, entry) =>
      (entry?.borderBoxSize?.[0]?.blockSize ??
        element.getBoundingClientRect().height) ||
      Math.max(
        24,
        Math.ceil(
          (rows[Number(element.getAttribute("data-index"))]?.source.length ??
            0) / 80,
        ) * 24,
      ),
    overscan: 2,
    scrollMargin: margin,
    initialRect: { width: 700, height: 800 },
    rangeExtractor: (range) =>
      [
        ...new Set([
          ...defaultRangeExtractor(range),
          ...pinned,
          ...(active ? [rows.length - 1] : []),
        ]),
      ]
        .filter((index) => index >= 0 && index < rows.length)
        .sort((a, b) => a - b),
  });
  virtualizer.shouldAdjustScrollPositionOnItemSizeChange = () => false;
  useLayoutEffect(() => {
    const element = ref.current;
    const viewport = element?.closest<HTMLElement>(
      "[data-radix-scroll-area-viewport]",
    );
    const refreshMargin = () => {
      if (element && viewport)
        setMargin(
          element.getBoundingClientRect().top -
            viewport.getBoundingClientRect().top +
            viewport.scrollTop,
        );
    };
    refreshMargin();
    viewport?.addEventListener("iris-conversation-geometry", refreshMargin);
    const preserve = () => {
      const selection = window.getSelection();
      const next = Array.from(
        element?.querySelectorAll<HTMLElement>("[data-source-row]") ?? [],
      )
        .filter(
          (node) =>
            node.contains(document.activeElement) ||
            (selection &&
              !selection.isCollapsed &&
              selection.containsNode(node, true)),
        )
        .map((node) => Number(node.dataset.index));
      setPinned((previous) =>
        previous.join() === next.join() ? previous : next,
      );
    };
    document.addEventListener("selectionchange", preserve);
    element?.addEventListener("focusin", preserve);
    element?.addEventListener("focusout", preserve);
    return () => {
      viewport?.removeEventListener(
        "iris-conversation-geometry",
        refreshMargin,
      );
      document.removeEventListener("selectionchange", preserve);
      element?.removeEventListener("focusin", preserve);
      element?.removeEventListener("focusout", preserve);
    };
  }, []);
  useLayoutEffect(() => {
    if (!active || !ref.current) return;
    const leaves = ref.current.querySelectorAll<HTMLElement>(
      "td,th,code,[data-source-row] p,[data-source-row] li,[data-source-row] h1,[data-source-row] h2,[data-source-row] h3,[data-source-row] h4,[data-source-row] h5,[data-source-row] h6",
    );
    const leaf = leaves[leaves.length - 1];
    if (leaf) {
      leaf.setAttribute("data-streaming-tail", "");
      // Measure in the next frame's read phase, after semantic DOM writes have
      // committed. Parent geometry events also refresh font/width-dependent pace.
      const readBudget = () => {
        if (budgetRef) budgetRef.current = readTailLineBudget(leaf);
      };
      const frame = window.requestAnimationFrame(readBudget);
      const viewport = ref.current.closest("[data-radix-scroll-area-viewport]");
      viewport?.addEventListener("iris-conversation-geometry", readBudget);
      return () => {
        window.cancelAnimationFrame(frame);
        viewport?.removeEventListener("iris-conversation-geometry", readBudget);
        leaf.removeAttribute("data-streaming-tail");
      };
    }
  }, [active, bodySource, budgetRef]);
  const measured = virtualizer.getVirtualItems();
  const fallback = [
    ...new Set([0, 1, 2, ...(active ? [rows.length - 1] : [])]),
  ].filter((index) => index < rows.length);
  const items = !windowed
    ? [{ index: 0, start: 0, end: 0 }]
    : measured.length
      ? measured
      : fallback
          .map((index) => virtualizer.measurementsCache[index]!)
          .filter(Boolean);
  function spacer(height: number) {
    if (height <= 0) return null;
    if (table)
      return (
        <tbody aria-hidden>
          <tr>
            <td
              className="ai-markdown-window-spacer"
              colSpan={1000}
              style={{ height, padding: 0, border: 0 }}
            />
          </tr>
        </tbody>
      );
    return (
      <span
        className="ai-markdown-window-spacer"
        aria-hidden
        style={{ display: "block", height }}
      />
    );
  }
  let previousEnd = 0;
  const content = items.map((item) => {
    const row = rows[item.index]!;
    const gap = windowed ? Math.max(0, item.start - margin - previousEnd) : 0;
    previousEnd = item.end - margin;
    if (table)
      return (
        <Fragment key={row.start}>
          {spacer(gap)}
          <TableRows
            row={row}
            header={header}
            definitions={definitions}
            index={item.index}
            measure={windowed ? virtualizer.measureElement : undefined}
          />
        </Fragment>
      );
    return (
      <Fragment key={row.start}>
        {spacer(gap)}
        <span
          style={{ display: "block" }}
          data-index={item.index}
          data-source-row={row.start}
          ref={windowed ? virtualizer.measureElement : undefined}
        >
          {code ? (
            <CodeText source={row.source} language={language} active={active} />
          ) : (
            <MarkdownRows
              source={row.source}
              providedHtml={row.html}
              header=""
              definitions={definitions}
              table={false}
              active={active && item.index === rows.length - 1}
            />
          )}
        </span>
      </Fragment>
    );
  });
  const bottom = windowed
    ? spacer(Math.max(0, virtualizer.getTotalSize() - previousEnd))
    : null;
  return (
    <div
      ref={ref}
      className={
        code
          ? "ai-code-block ai-markdown-row-window"
          : table
            ? "ai-table-wrap ai-markdown-row-window"
            : "ai-markdown-row-window"
      }
      data-source-length={source.length}
      data-windowed={windowed ? "true" : undefined}
    >
      {code && (
        <button
          type="button"
          className="ai-code-copy-button"
          aria-label="复制代码"
          onClick={(event) => {
            event.stopPropagation();
            void navigator.clipboard?.writeText(source).catch(() => undefined);
          }}
        >
          复制
        </button>
      )}
      {code ? (
        <pre>
          <code className={language ? `language-${language}` : undefined}>
            {content}
            {bottom}
          </code>
        </pre>
      ) : table ? (
        <table>
          <thead
            dangerouslySetInnerHTML={{ __html: toTrustedHtml(headerHtml) }}
          />
          {content}
          {bottom}
        </table>
      ) : (
        <>
          {content}
          {bottom}
        </>
      )}
    </div>
  );
}

const CodeText = memo(function CodeText({
  source,
  language,
  active,
}: {
  source: string;
  language: string;
  active: boolean;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    if (
      node.childNodes.length === 1 &&
      node.firstChild?.nodeType === Node.TEXT_NODE
    )
      node.firstChild.nodeValue = source;
    else node.textContent = source;
  }, [source]);
  useEffect(() => {
    if (active || !language || source.length > 8192) return;
    // Highlight confirmed, bounded code rows outside the reveal/layout task.
    const timer = window.setTimeout(() => {
      if (!ref.current || ref.current.contains(document.activeElement)) return;
      const selection = window.getSelection();
      if (
        selection &&
        !selection.isCollapsed &&
        selection.containsNode(ref.current, true)
      )
        return;
      const fence = "`".repeat(
        Math.max(
          3,
          ...[...source.matchAll(/`+/g)].map((match) => match[0].length + 1),
        ),
      );
      const html = renderMarkdownWithProfile(
        `${fence}${language}\n${source}\n${fence}`,
        "chat_assistant",
      ).output;
      const template = document.createElement("template");
      template.innerHTML = toTrustedHtml(html) as string;
      const code = template.content.querySelector("code");
      if (code && code.textContent === source)
        reconcileMarkdownChildren(ref.current, code);
    }, 0);
    return () => window.clearTimeout(timer);
  }, [source, language, active]);
  return <span ref={ref} />;
});

const TableRows = memo(function TableRows({
  row,
  header,
  definitions,
  index,
  measure,
}: {
  row: SourceRow;
  header: string;
  definitions: string;
  index: number;
  measure?: (element: Element | null) => void;
}) {
  const html = useMemo(
    () => tablePart(`${header}${row.source}\n\n${definitions}`, "tbody"),
    [header, row.source, definitions],
  );
  const ref = useRef<HTMLTableSectionElement>(null);
  useLayoutEffect(() => {
    if (!ref.current) return;
    const template = document.createElement("template");
    template.innerHTML = toTrustedHtml(html) as string;
    reconcileMarkdownChildren(ref.current, template.content);
    measure?.(ref.current);
  }, [html, measure]);
  return <tbody data-index={index} data-source-row={row.start} ref={ref} />;
});
