import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";

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

/**
 * Calculates the scroll position that keeps the newest output above the
 * viewport bottom edge.
 *
 * `AiMessageList` reserves a bottom spacer after the virtual rows, so scrolling
 * to `maxScrollTop` leaves that spacer visible below the latest assistant
 * content. The tail geometry is accepted for callers that still locate the
 * streaming tail, but the anchor itself intentionally follows the scroll
 * content bottom rather than pinning the tail to the edge.
 */
export function readingAnchorTarget({
  scrollHeight,
  clientHeight,
  tailBottom: _tailBottom,
}: ReadingAnchorInput): number {
  return Math.max(0, scrollHeight - clientHeight);
}

/**
 * Follows the latest streaming content while reserving a fixed bottom gap.
 * Human upward movement permanently detaches until the user explicitly returns
 * to the newest output.
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
