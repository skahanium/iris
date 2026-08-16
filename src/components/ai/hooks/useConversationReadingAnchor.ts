import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";

const SHORT_CONTENT_RATIO = 0.9;
const READING_TAIL_VIEWPORT_RATIO = 0.6;

export interface ReadingAnchorInput {
  scrollHeight: number;
  clientHeight: number;
  tailBottom: number;
}

export interface TailBottomGeometry {
  viewportTop: number;
  viewportScrollTop: number;
  tailBottom: number;
}

/** Maps a viewport-relative tail edge into scroll-content coordinates. */
export function tailBottomInScrollContent({
  viewportTop,
  viewportScrollTop,
  tailBottom,
}: TailBottomGeometry): number {
  return tailBottom - viewportTop + viewportScrollTop;
}

/** Calculates a clamped scroll position for short and long streaming answers. */
export function readingAnchorTarget({
  scrollHeight,
  clientHeight,
  tailBottom,
}: ReadingAnchorInput): number {
  const maxScrollTop = Math.max(0, scrollHeight - clientHeight);
  if (scrollHeight <= clientHeight * SHORT_CONTENT_RATIO) return maxScrollTop;
  return Math.min(
    maxScrollTop,
    Math.max(0, tailBottom - clientHeight * READING_TAIL_VIEWPORT_RATIO),
  );
}

/**
 * Follows a short answer at the bottom, then moves the live tail into the
 * reading zone for long output. Human upward movement permanently detaches
 * until the user explicitly returns to the newest output.
 */
export function useConversationReadingAnchor({
  viewportRef,
  active,
  revision,
  streamKey,
}: {
  viewportRef: RefObject<HTMLDivElement | null>;
  active: boolean;
  revision: number;
  streamKey: string | null;
}) {
  const [following, setFollowing] = useState(true);
  const programmaticWriteRef = useRef(false);
  const activeStreamKeyRef = useRef<string | null>(null);
  const lastObservedScrollTopRef = useRef(0);
  const observedRevisionRef = useRef(0);
  const [tailRevision, setTailRevision] = useState(0);

  const returnToLatest = useCallback(() => {
    setFollowing(true);
  }, []);

  useLayoutEffect(() => {
    if (!active || !streamKey || activeStreamKeyRef.current === streamKey) {
      return;
    }
    activeStreamKeyRef.current = streamKey;
    setFollowing(true);
  }, [active, streamKey]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onScroll = () => {
      const nextScrollTop = viewport.scrollTop;
      const movedUp = nextScrollTop < lastObservedScrollTopRef.current;
      lastObservedScrollTopRef.current = nextScrollTop;
      if (programmaticWriteRef.current || !movedUp) return;
      setFollowing(false);
    };
    lastObservedScrollTopRef.current = viewport.scrollTop;
    viewport.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      viewport.removeEventListener("scroll", onScroll);
    };
  }, [viewportRef]);

  useEffect(() => {
    if (!active || !streamKey) return;
    const viewport = viewportRef.current;
    const tail = viewport?.querySelector<HTMLElement>("[data-streaming-tail]");
    if (!tail || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      observedRevisionRef.current += 1;
      setTailRevision(observedRevisionRef.current);
    });
    observer.observe(tail);
    return () => observer.disconnect();
  }, [active, streamKey, viewportRef]);

  useLayoutEffect(() => {
    if (!active || !following) return;
    const viewport = viewportRef.current;
    if (!viewport) return;
    const tail = viewport.querySelector<HTMLElement>("[data-streaming-tail]");
    const viewportBounds = viewport.getBoundingClientRect();
    const tailBounds = tail?.getBoundingClientRect();
    const target = readingAnchorTarget({
      scrollHeight: viewport.scrollHeight,
      clientHeight: viewport.clientHeight,
      tailBottom: tailBounds
        ? tailBottomInScrollContent({
            viewportTop: viewportBounds.top,
            viewportScrollTop: viewport.scrollTop,
            tailBottom: tailBounds.bottom,
          })
        : viewport.scrollHeight,
    });
    if (Math.abs(viewport.scrollTop - target) < 1) return;
    programmaticWriteRef.current = true;
    lastObservedScrollTopRef.current = target;
    viewport.scrollTop = target;
    window.requestAnimationFrame(() => {
      programmaticWriteRef.current = false;
    });
  }, [active, following, revision, tailRevision, viewportRef]);

  return { following, returnToLatest };
}
